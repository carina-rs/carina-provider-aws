//! Round-trip regressions against scripted AWS protocol responses.
//! No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::sync::{Arc, Mutex};

use carina_core::differ::{Diff, Orphans, PlanPriorStates, create_plan, diff};
use carina_core::effect::Effect;
use carina_core::provider::{
    AppliedIdentifier, CreateRequest, NoopNormalizer, Provider, ProviderNormalizer, RawSavedAttrs,
    ReadRequest, UpdateRequest,
};
use carina_core::resource::{
    ConcreteValue, Current, ResolvedResource, Resource, State, Value, into_plan_input_current_map,
};
use carina_core::schema::{SchemaKind, SchemaRegistry};
use carina_provider_aws::{AwsNormalizer, AwsProvider, BoundedSdkConfig, test_existing_state};
use indexmap::IndexMap;
use winterbaume_core::{
    MockAws, MockRequest, MockResponse, MockService, StatefulService, default_account_id,
    rest_json_error,
};
use winterbaume_s3::views::BucketStateView;
use winterbaume_s3::{S3Service, S3StateView};

const REGION: &str = "ap-northeast-1";
const LOG_GROUP_NAME: &str = "/carina/read-roundtrip";
const CERTIFICATE_ARN: &str =
    "arn:aws:acm:ap-northeast-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef";
const QUEUE_NAME: &str = "carina-read-roundtrip";
const QUEUE_URL: &str =
    "https://sqs.ap-northeast-1.amazonaws.com/123456789012/carina-read-roundtrip";
const FIFO_QUEUE_NAME: &str = "carina-read-roundtrip.fifo";
const FIFO_QUEUE_URL: &str =
    "https://sqs.ap-northeast-1.amazonaws.com/123456789012/carina-read-roundtrip.fifo";
const VPC_ID: &str = "vpc-0123456789abcdef0";
const SUBNET_ID: &str = "subnet-0123456789abcdef0";
const EIP_ALLOCATION_ID: &str = "eipalloc-0123456789abcdef0";
const NAT_GATEWAY_ID: &str = "nat-0123456789abcdef0";
const SECURITY_GROUP_ID: &str = "sg-0123456789abcdef0";
const SOURCE_SECURITY_GROUP_ID: &str = "sg-0fedcba9876543210";
const SECURITY_GROUP_RULE_ID: &str = "sgr-0123456789abcdef0";
const FLOW_LOG_ID: &str = "fl-0123456789abcdef0";
const VPC_ENDPOINT_ID: &str = "vpce-0123456789abcdef0";
const REPLICATION_BUCKET: &str = "carina-read-roundtrip-replication";
const ORGANIZATIONS_ACCOUNT_ID: &str = "123456789012";

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn bool_value(value: bool) -> Value {
    Value::Concrete(ConcreteValue::Bool(value))
}

fn list(values: &[&str]) -> Value {
    Value::Concrete(ConcreteValue::List(
        values.iter().map(|value| string(value)).collect(),
    ))
}

fn empty_map() -> Value {
    Value::Concrete(ConcreteValue::Map(IndexMap::new()))
}

fn assert_api_text(value: Option<&Value>, expected: &str) {
    let actual = match value {
        Some(Value::Concrete(ConcreteValue::String(value))) => value.expose_plaintext(),
        Some(Value::Concrete(ConcreteValue::CanonicalEnum(value))) => value.api_value(),
        other => panic!("expected string-like value {expected:?}, got {other:?}"),
    };
    assert_eq!(actual, expected);
}

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
}

fn query_body(request: &MockRequest) -> &str {
    str::from_utf8(&request.body).expect("AWS Query body must be UTF-8")
}

fn query_action(request: &MockRequest) -> &str {
    query_body(request)
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("Action="))
        .expect("AWS Query body must contain Action")
}

fn assert_query_contains(request: &MockRequest, parameter: &str) {
    let body = query_body(request);
    assert!(
        body.split('&').any(|actual| actual == parameter),
        "expected query parameter {parameter:?}, got {body}"
    );
}

fn schema_registry() -> SchemaRegistry {
    let mut schemas = SchemaRegistry::new();
    for schema in carina_provider_aws::schemas::all_schemas() {
        schemas.insert("aws", schema);
    }
    schemas
}

async fn provider_from_service(service: impl MockService + 'static) -> AwsProvider {
    let mock = MockAws::builder().with_service(service).build();
    let sdk_config = mock.sdk_config(REGION).await;
    AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    )
}

struct ReplicationRoundTripService {
    inner: S3Service,
}

impl MockService for ReplicationRoundTripService {
    fn service_name(&self) -> &str {
        self.inner.service_name()
    }

    fn url_patterns(&self) -> Vec<&str> {
        self.inner.url_patterns()
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let is_replication = request.uri.split_once('?').is_some_and(|(_, query)| {
            query
                .split('&')
                .any(|parameter| parameter.split('=').next() == Some("replication"))
        });
        if request.method == "PUT" && is_replication {
            let body =
                str::from_utf8(&request.body).expect("replication request XML must be UTF-8");
            assert!(
                body.contains(
                    "<DeleteMarkerReplication><Status>Disabled</Status></DeleteMarkerReplication>"
                ),
                "normalized replication request must send the canonical Disabled default: {body}"
            );
        }
        self.inner.handle(request)
    }
}

async fn provider_with_replication_bucket() -> AwsProvider {
    let service = S3Service::new();
    service
        .restore(
            default_account_id(),
            REGION,
            S3StateView {
                buckets: HashMap::from([(
                    REPLICATION_BUCKET.to_string(),
                    BucketStateView {
                        name: REPLICATION_BUCKET.to_string(),
                        region: REGION.to_string(),
                        ..Default::default()
                    },
                )]),
            },
        )
        .await
        .expect("stage source bucket for replication");
    provider_from_service(ReplicationRoundTripService { inner: service }).await
}

fn request_target(request: &MockRequest) -> &str {
    request
        .headers
        .get("x-amz-target")
        .and_then(|value| value.to_str().ok())
        .expect("AWS JSON request must carry x-amz-target")
}

fn assert_no_diff(desired: Resource, current: State) {
    let registry = schema_registry();
    let schema = registry
        .get(
            "aws",
            desired.id.resource_type.as_str(),
            SchemaKind::Resource,
        )
        .expect("managed resource schema");
    let saved = carina_core::provider::RawSavedAttrs::default().lift(&registry);
    let result = diff(
        &ResolvedResource::new(desired),
        &current,
        &saved,
        None,
        Some(schema.schema().as_schema()),
    );
    assert!(matches!(result, Diff::NoChange(_)), "{result:?}");
}

async fn create_request_for_test(resource: Resource) -> CreateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
    let schemas = schema_registry();
    let preparation = carina_core::executor::ProviderPreparationContext::new(
        &bindings,
        &module_gate,
        &[],
        &AwsNormalizer,
        &[],
        &schemas,
    );
    carina_core::executor::prepare_create_request(resource, &preparation)
        .await
        .expect("test resource should pass checked create preparation")
}

async fn create_request_without_schema_for_test(resource: Resource) -> CreateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
    let schemas = SchemaRegistry::new();
    let preparation = carina_core::executor::ProviderPreparationContext::new(
        &bindings,
        &module_gate,
        &[],
        &NoopNormalizer,
        &[],
        &schemas,
    );
    carina_core::executor::prepare_create_request(resource, &preparation)
        .await
        .expect("test resource should become a provider-ready create request")
}

async fn normalize_desired_for_test(mut resource: Resource) -> Resource {
    let mut resources = vec![resource.clone().into()];
    AwsNormalizer
        .normalize_desired(&mut resources)
        .await
        .expect("test resource should normalize");
    resource.attributes = resources
        .pop()
        .expect("one normalized test resource")
        .attributes;
    resource
}

async fn create_round_trip_state(provider: &AwsProvider, desired: &Resource) -> State {
    let outcome = carina_core::executor::dispatch_create(
        provider,
        &desired.id,
        create_request_without_schema_for_test(desired.clone()).await,
    )
    .await
    .expect("scripted AWS create should succeed");
    carina_core::executor::complete_create_for_writeback(outcome).0
}

async fn update_request_for_test(
    resource: Resource,
    from: State,
    changed_attributes: &[String],
) -> UpdateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
    let schemas = schema_registry();
    let preparation = carina_core::executor::ProviderPreparationContext::new(
        &bindings,
        &module_gate,
        &[],
        &AwsNormalizer,
        &[],
        &schemas,
    );
    carina_core::executor::prepare_update_request(resource, from, changed_attributes, &preparation)
        .await
        .expect("test resource should pass checked update preparation")
}

struct LogGroupService;

impl MockService for LogGroupService {
    fn service_name(&self) -> &str {
        "logs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://logs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "Logs_20140328.DescribeLogGroups" => MockResponse::json(
                200,
                format!(r#"{{"logGroups":[{{"logGroupName":"{LOG_GROUP_NAME}"}}]}}"#),
            ),
            "Logs_20140328.ListTagsLogGroup" => MockResponse::json(200, r#"{"tags":{}}"#),
            target => panic!("unexpected CloudWatch Logs operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct LogGroupTagsDeniedService;

impl MockService for LogGroupTagsDeniedService {
    fn service_name(&self) -> &str {
        "logs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://logs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "Logs_20140328.DescribeLogGroups" => MockResponse::json(
                200,
                format!(r#"{{"logGroups":[{{"logGroupName":"{LOG_GROUP_NAME}"}}]}}"#),
            ),
            "Logs_20140328.ListTagsLogGroup" => MockResponse::json(
                403,
                r#"{"__type":"AccessDeniedException","message":"not authorized"}"#,
            ),
            target => panic!("unexpected CloudWatch Logs operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct AcmService;

impl MockService for AcmService {
    fn service_name(&self) -> &str {
        "acm"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://acm\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "CertificateManager.DescribeCertificate" => MockResponse::json(
                200,
                format!(
                    r#"{{"Certificate":{{"CertificateArn":"{CERTIFICATE_ARN}","DomainName":"example.com","SubjectAlternativeNames":["www.example.com","example.com"],"Status":"ISSUED","Type":"AMAZON_ISSUED","KeyAlgorithm":"POST_QUANTUM_2048"}}}}"#
                ),
            ),
            "CertificateManager.ListTagsForCertificate" => {
                MockResponse::json(200, r#"{"Tags":[]}"#)
            }
            target => panic!("unexpected ACM operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct ProtectedLogGroupUpdateService;

impl MockService for ProtectedLogGroupUpdateService {
    fn service_name(&self) -> &str {
        "logs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://logs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "Logs_20140328.TagLogGroup" => MockResponse::json(200, r#"{}"#),
            "Logs_20140328.DescribeLogGroups" => MockResponse::json(
                200,
                format!(
                    r#"{{"logGroups":[{{"logGroupName":"{LOG_GROUP_NAME}","deletionProtectionEnabled":true}}]}}"#
                ),
            ),
            "Logs_20140328.ListTagsLogGroup" => {
                MockResponse::json(200, r#"{"tags":{"purpose":"roundtrip"}}"#)
            }
            "Logs_20140328.PutLogGroupDeletionProtection" => {
                panic!("an omitted desired attribute must not disable deletion protection")
            }
            target => panic!("unexpected CloudWatch Logs operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct OrganizationsAccountService;

impl MockService for OrganizationsAccountService {
    fn service_name(&self) -> &str {
        "organizations"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://organizations\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "AWSOrganizationsV20161128.CreateAccount" => {
                let body: serde_json::Value =
                    serde_json::from_slice(&request.body).expect("CreateAccount body must be JSON");
                assert_eq!(body["AccountName"], "member");
                assert_eq!(body["Email"], "member@example.com");
                assert_eq!(body["RoleName"], "OrganizationAccountAccessRole");
                assert_eq!(body["IamUserAccessToBilling"], "ALLOW");
                MockResponse::json(
                    200,
                    r#"{"CreateAccountStatus":{"Id":"car-create","State":"IN_PROGRESS"}}"#,
                )
            }
            "AWSOrganizationsV20161128.DescribeAccount" => MockResponse::json(
                200,
                r#"{"Account":{"Id":"123456789012","Arn":"arn:aws:organizations::111111111111:account/o-example/123456789012","Email":"member@example.com","Name":"member","Status":"ACTIVE","JoinedMethod":"CREATED"}}"#,
            ),
            "AWSOrganizationsV20161128.DescribeCreateAccountStatus" => MockResponse::json(
                200,
                r#"{"CreateAccountStatus":{"Id":"car-create","State":"SUCCEEDED","AccountId":"123456789012"}}"#,
            ),
            "AWSOrganizationsV20161128.ListTagsForResource" => {
                MockResponse::json(200, r#"{"Tags":[]}"#)
            }
            target => panic!("unexpected Organizations operation {target}"),
        };
        Box::pin(async move { response })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountRenameMode {
    Success,
    AccessDenied,
}

#[derive(Debug)]
struct AccountRenameState {
    account_name: String,
    calls: Vec<(String, String)>,
    mode: AccountRenameMode,
}

struct OrganizationsAccountRenameService {
    state: Arc<Mutex<AccountRenameState>>,
}

impl MockService for OrganizationsAccountRenameService {
    fn service_name(&self) -> &str {
        "organizations"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://organizations\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "AWSOrganizationsV20161128.DescribeAccount" => {
                let account_name = self
                    .state
                    .lock()
                    .expect("account rename state lock")
                    .account_name
                    .clone();
                MockResponse::json(
                    200,
                    serde_json::json!({
                        "Account": {
                            "Id": ORGANIZATIONS_ACCOUNT_ID,
                            "Arn": "arn:aws:organizations::111111111111:account/o-example/123456789012",
                            "Email": "member@example.com",
                            "Name": account_name,
                            "Status": "ACTIVE",
                            "JoinedMethod": "CREATED"
                        }
                    })
                    .to_string(),
                )
            }
            "AWSOrganizationsV20161128.ListTagsForResource" => {
                MockResponse::json(200, r#"{"Tags":[]}"#)
            }
            target => panic!("unexpected Organizations operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct AccountManagementRenameService {
    state: Arc<Mutex<AccountRenameState>>,
}

impl MockService for AccountManagementRenameService {
    fn service_name(&self) -> &str {
        "account"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://account\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        assert_eq!(request.method, "POST");
        assert!(
            request.uri.ends_with("/putAccountName"),
            "unexpected Account Management URI: {}",
            request.uri
        );
        let body: serde_json::Value =
            serde_json::from_slice(&request.body).expect("PutAccountName body must be JSON");
        let account_id = body["AccountId"]
            .as_str()
            .expect("PutAccountName AccountId")
            .to_string();
        let account_name = body["AccountName"]
            .as_str()
            .expect("PutAccountName AccountName")
            .to_string();

        let mut state = self.state.lock().expect("account rename state lock");
        state.calls.push((account_id, account_name.clone()));
        let response = match state.mode {
            AccountRenameMode::Success => {
                state.account_name = account_name;
                MockResponse::rest_json(200, r#"{}"#)
            }
            AccountRenameMode::AccessDenied => rest_json_error(
                403,
                "AccessDeniedException",
                "Account Management trusted access is not enabled",
            ),
        };
        Box::pin(async move { response })
    }
}

async fn provider_with_account_rename(
    initial_name: &str,
    mode: AccountRenameMode,
) -> (AwsProvider, Arc<Mutex<AccountRenameState>>) {
    let state = Arc::new(Mutex::new(AccountRenameState {
        account_name: initial_name.to_string(),
        calls: Vec::new(),
        mode,
    }));
    let mock = MockAws::builder()
        .with_service(OrganizationsAccountRenameService {
            state: Arc::clone(&state),
        })
        .with_service(AccountManagementRenameService {
            state: Arc::clone(&state),
        })
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    );
    (provider, state)
}

fn organizations_account_resource(account_name: &str) -> Resource {
    let mut resource = Resource::with_provider("aws", "organizations.Account", "member", None);
    resource.set_attr("account_name", string(account_name));
    resource.set_attr("email", string("member@example.com"));
    resource.set_attr("tags", empty_map());
    resource
}

fn organizations_account_state(resource: &Resource, account_name: &str) -> State {
    test_existing_state(
        resource.id.clone(),
        HashMap::from([
            ("account_name".to_string(), string(account_name)),
            ("email".to_string(), string("member@example.com")),
            ("tags".to_string(), empty_map()),
        ]),
    )
    .with_identifier(ORGANIZATIONS_ACCOUNT_ID)
}

#[derive(Clone, Copy)]
enum Ec2RoundTripCase {
    VpcIpam,
    SubnetIpamIpv6,
    EipAddress,
    NatRegional,
    SecurityGroupIngress,
    FlowLog,
    VpcEndpoint,
}

struct Ec2RoundTripService(Ec2RoundTripCase);

impl MockService for Ec2RoundTripService {
    fn service_name(&self) -> &str {
        "ec2"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        assert_eq!(request.method, "POST", "EC2 Query requests use POST");
        let response = match (self.0, query_action(&request)) {
            (Ec2RoundTripCase::VpcIpam, "CreateVpc") => {
                assert_query_contains(&request, "Ipv4IpamPoolId=ipam-pool-0123456789abcdef0");
                assert_query_contains(&request, "Ipv4NetmaskLength=16");
                MockResponse::xml(
                    200,
                    format!(
                        r#"<CreateVpcResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>vpc-create</requestId><vpc><vpcId>{VPC_ID}</vpcId><cidrBlock>10.42.0.0/16</cidrBlock><state>pending</state><instanceTenancy>default</instanceTenancy></vpc></CreateVpcResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::VpcIpam, "DescribeVpcs") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeVpcsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>vpc-read</requestId><vpcSet><item><vpcId>{VPC_ID}</vpcId><cidrBlock>10.42.0.0/16</cidrBlock><state>available</state><instanceTenancy>default</instanceTenancy><tagSet/></item></vpcSet></DescribeVpcsResponse>"#,
                ),
            ),
            (Ec2RoundTripCase::VpcIpam, "DescribeVpcAttribute") => MockResponse::xml(
                200,
                r#"<DescribeVpcAttributeResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>vpc-attribute</requestId><enableDnsSupport><value>false</value></enableDnsSupport><enableDnsHostnames><value>false</value></enableDnsHostnames></DescribeVpcAttributeResponse>"#,
            ),
            (Ec2RoundTripCase::SubnetIpamIpv6, "CreateSubnet") => {
                assert_query_contains(&request, &format!("VpcId={VPC_ID}"));
                assert_query_contains(&request, "Ipv4IpamPoolId=ipam-pool-0123456789abcdef0");
                assert_query_contains(&request, "Ipv4NetmaskLength=24");
                assert_query_contains(&request, "Ipv6CidrBlock=2001%3Adb8%3A%3A%2F64");
                MockResponse::xml(
                    200,
                    format!(
                        r#"<CreateSubnetResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>subnet-create</requestId><subnet><subnetId>{SUBNET_ID}</subnetId><vpcId>{VPC_ID}</vpcId><cidrBlock>10.42.1.0/24</cidrBlock><availabilityZone>ap-northeast-1a</availabilityZone><availabilityZoneId>apne1-az4</availabilityZoneId><state>pending</state></subnet></CreateSubnetResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::SubnetIpamIpv6, "DescribeSubnets") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeSubnetsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>subnet-read</requestId><subnetSet><item><subnetId>{SUBNET_ID}</subnetId><vpcId>{VPC_ID}</vpcId><cidrBlock>10.42.1.0/24</cidrBlock><availabilityZone>ap-northeast-1a</availabilityZone><availabilityZoneId>apne1-az4</availabilityZoneId><state>available</state><ipv6CidrBlockAssociationSet><item><associationId>subnet-cidr-assoc-pending</associationId><ipv6CidrBlock>2001:db8:1::/64</ipv6CidrBlock><ipv6CidrBlockState><state>associating</state></ipv6CidrBlockState></item><item><associationId>subnet-cidr-assoc-ready</associationId><ipv6CidrBlock>2001:db8::/64</ipv6CidrBlock><ipv6CidrBlockState><state>associated</state></ipv6CidrBlockState></item></ipv6CidrBlockAssociationSet><tagSet/></item></subnetSet></DescribeSubnetsResponse>"#,
                ),
            ),
            (Ec2RoundTripCase::EipAddress, "AllocateAddress") => {
                assert_query_contains(&request, "Address=198.51.100.42");
                MockResponse::xml(
                    200,
                    format!(
                        r#"<AllocateAddressResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>eip-create</requestId><publicIp>198.51.100.42</publicIp><allocationId>{EIP_ALLOCATION_ID}</allocationId><domain>vpc</domain></AllocateAddressResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::EipAddress, "DescribeAddresses") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeAddressesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>eip-read</requestId><addressesSet><item><publicIp>198.51.100.42</publicIp><allocationId>{EIP_ALLOCATION_ID}</allocationId><domain>vpc</domain><tagSet/></item></addressesSet></DescribeAddressesResponse>"#,
                ),
            ),
            (Ec2RoundTripCase::NatRegional, "CreateNatGateway") => {
                let body = query_body(&request);
                assert_query_contains(&request, "AvailabilityMode=regional");
                assert_query_contains(&request, "ConnectivityType=private");
                assert_query_contains(&request, &format!("VpcId={VPC_ID}"));
                assert_query_contains(&request, "PrivateIpAddress=10.42.0.5");
                assert!(body.contains("ap-northeast-1a"), "{body}");
                assert!(body.contains(EIP_ALLOCATION_ID), "{body}");
                assert!(!body.contains("SubnetId="), "{body}");
                MockResponse::xml(
                    200,
                    format!(
                        r#"<CreateNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>nat-create</requestId><natGateway><natGatewayId>{NAT_GATEWAY_ID}</natGatewayId><state>pending</state></natGateway></CreateNatGatewayResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::NatRegional, "DescribeNatGateways") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeNatGatewaysResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>nat-read</requestId><natGatewaySet><item><natGatewayId>{NAT_GATEWAY_ID}</natGatewayId><state>available</state><availabilityMode>regional</availabilityMode><connectivityType>private</connectivityType><vpcId>{VPC_ID}</vpcId><natGatewayAddressSet><item><allocationId>{EIP_ALLOCATION_ID}</allocationId><privateIp>10.42.0.5</privateIp><isPrimary>true</isPrimary><availabilityZone>ap-northeast-1a</availabilityZone><availabilityZoneId>apne1-az4</availabilityZoneId></item></natGatewayAddressSet><tagSet/></item></natGatewaySet></DescribeNatGatewaysResponse>"#,
                ),
            ),
            (Ec2RoundTripCase::SecurityGroupIngress, "AuthorizeSecurityGroupIngress") => {
                let body = query_body(&request);
                assert!(
                    body.contains("IpPermissions.1.Groups.1.UserId=123456789012"),
                    "{body}"
                );
                MockResponse::xml(
                    200,
                    format!(
                        r#"<AuthorizeSecurityGroupIngressResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>sgr-create</requestId><return>true</return><securityGroupRuleSet><item><securityGroupRuleId>{SECURITY_GROUP_RULE_ID}</securityGroupRuleId><groupId>{SECURITY_GROUP_ID}</groupId><groupOwnerId>123456789012</groupOwnerId><isEgress>false</isEgress><ipProtocol>tcp</ipProtocol><fromPort>443</fromPort><toPort>443</toPort><referencedGroupInfo><groupId>{SOURCE_SECURITY_GROUP_ID}</groupId><userId>123456789012</userId></referencedGroupInfo></item></securityGroupRuleSet></AuthorizeSecurityGroupIngressResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::SecurityGroupIngress, "DescribeSecurityGroupRules") => {
                MockResponse::xml(
                    200,
                    format!(
                        r#"<DescribeSecurityGroupRulesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>sgr-read</requestId><securityGroupRuleSet><item><securityGroupRuleId>{SECURITY_GROUP_RULE_ID}</securityGroupRuleId><groupId>{SECURITY_GROUP_ID}</groupId><groupOwnerId>123456789012</groupOwnerId><isEgress>false</isEgress><ipProtocol>tcp</ipProtocol><fromPort>443</fromPort><toPort>443</toPort><referencedGroupInfo><groupId>{SOURCE_SECURITY_GROUP_ID}</groupId><userId>123456789012</userId></referencedGroupInfo></item></securityGroupRuleSet></DescribeSecurityGroupRulesResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::FlowLog, "CreateFlowLogs") => {
                assert_query_contains(&request, "ResourceType=VPC");
                assert_query_contains(&request, &format!("ResourceId.1={VPC_ID}"));
                MockResponse::xml(
                    200,
                    format!(
                        r#"<CreateFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>flow-create</requestId><flowLogIdSet><item>{FLOW_LOG_ID}</item></flowLogIdSet><unsuccessful/></CreateFlowLogsResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::FlowLog, "DescribeFlowLogs") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>flow-read</requestId><flowLogSet><item><flowLogId>{FLOW_LOG_ID}</flowLogId><resourceId>{VPC_ID}</resourceId><trafficType>ALL</trafficType><flowLogStatus>ACTIVE</flowLogStatus><tagSet/></item></flowLogSet></DescribeFlowLogsResponse>"#,
                ),
            ),
            (Ec2RoundTripCase::VpcEndpoint, "CreateVpcEndpoint") => {
                let body = query_body(&request);
                assert!(body.contains("ResourceConfigurationArn="), "{body}");
                assert!(
                    body.contains("resourceconfiguration%2Frcfg-0123456789abcdef0"),
                    "{body}"
                );
                assert!(body.contains("ServiceNetworkArn="), "{body}");
                assert!(
                    body.contains("servicenetwork%2Fsn-0123456789abcdef0"),
                    "{body}"
                );
                assert_query_contains(&request, "ServiceRegion=ap-northeast-1");
                MockResponse::xml(
                    200,
                    format!(
                        r#"<CreateVpcEndpointResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>vpce-create</requestId><vpcEndpoint><vpcEndpointId>{VPC_ENDPOINT_ID}</vpcEndpointId><vpcId>{VPC_ID}</vpcId><serviceName>com.amazonaws.vpce.ap-northeast-1.vpce-svc-0123456789abcdef0</serviceName><vpcEndpointType>Resource</vpcEndpointType></vpcEndpoint></CreateVpcEndpointResponse>"#,
                    ),
                )
            }
            (Ec2RoundTripCase::VpcEndpoint, "DescribeVpcEndpoints") => MockResponse::xml(
                200,
                format!(
                    r#"<DescribeVpcEndpointsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>vpce-read</requestId><vpcEndpointSet><item><vpcEndpointId>{VPC_ENDPOINT_ID}</vpcEndpointId><vpcId>{VPC_ID}</vpcId><serviceName>com.amazonaws.vpce.ap-northeast-1.vpce-svc-0123456789abcdef0</serviceName><state>available</state><vpcEndpointType>Resource</vpcEndpointType><resourceConfigurationArn>arn:aws:vpc-lattice:ap-northeast-1:123456789012:resourceconfiguration/rcfg-0123456789abcdef0</resourceConfigurationArn><serviceNetworkArn>arn:aws:vpc-lattice:ap-northeast-1:123456789012:servicenetwork/sn-0123456789abcdef0</serviceNetworkArn><serviceRegion>ap-northeast-1</serviceRegion><routeTableIdSet/><subnetIdSet/><groupSet/><tagSet/></item></vpcEndpointSet></DescribeVpcEndpointsResponse>"#,
                ),
            ),
            (_, action) => panic!(
                "unexpected EC2 operation {action}: {}",
                query_body(&request)
            ),
        };
        Box::pin(async move { response })
    }
}

struct SqsService;

impl MockService for SqsService {
    fn service_name(&self) -> &str {
        "sqs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://sqs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "AmazonSQS.CreateQueue" => {
                let body: serde_json::Value =
                    serde_json::from_slice(&request.body).expect("CreateQueue body must be JSON");
                let attributes = body
                    .get("Attributes")
                    .and_then(serde_json::Value::as_object);
                assert!(
                    attributes.is_none_or(|attributes| !attributes.contains_key("FifoQueue")),
                    "CreateQueue must not send FifoQueue=false for a standard queue: {body}"
                );
                assert!(
                    attributes.is_none_or(|attributes| {
                        !attributes.contains_key("ContentBasedDeduplication")
                    }),
                    "CreateQueue must not send ContentBasedDeduplication=false for a standard queue: {body}"
                );
                MockResponse::json(200, format!(r#"{{"QueueUrl":"{QUEUE_URL}"}}"#))
            }
            "AmazonSQS.GetQueueAttributes" => MockResponse::json(
                200,
                r#"{"Attributes":{"QueueArn":"arn:aws:sqs:ap-northeast-1:123456789012:carina-read-roundtrip","VisibilityTimeout":"30","MaximumMessageSize":"262144","MessageRetentionPeriod":"345600","DelaySeconds":"0","ReceiveMessageWaitTimeSeconds":"0","CreatedTimestamp":"1793260800","LastModifiedTimestamp":"1793260800"}}"#,
            ),
            "AmazonSQS.ListQueueTags" => MockResponse::json(200, r#"{"Tags":{}}"#),
            target => panic!("unexpected SQS operation {target}"),
        };
        Box::pin(async move { response })
    }
}

struct SqsTagsDeniedService;

impl MockService for SqsTagsDeniedService {
    fn service_name(&self) -> &str {
        "sqs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://sqs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "AmazonSQS.GetQueueAttributes" => MockResponse::json(
                200,
                r#"{"Attributes":{"QueueArn":"arn:aws:sqs:ap-northeast-1:123456789012:carina-read-roundtrip","VisibilityTimeout":"30","MaximumMessageSize":"262144","MessageRetentionPeriod":"345600","DelaySeconds":"0","ReceiveMessageWaitTimeSeconds":"0","CreatedTimestamp":"1793260800","LastModifiedTimestamp":"1793260800"}}"#,
            ),
            "AmazonSQS.ListQueueTags" => MockResponse::json(
                403,
                r#"{"__type":"AccessDeniedException","message":"not authorized"}"#,
            ),
            target => panic!("unexpected SQS operation {target}"),
        };
        Box::pin(async move { response })
    }
}

#[derive(Clone, Copy)]
enum SqsRoundTripCase {
    FifoDefaults,
    RedriveStringCount,
    KmsAlias,
}

struct SqsRoundTripService(SqsRoundTripCase);

impl MockService for SqsRoundTripService {
    fn service_name(&self) -> &str {
        "sqs"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://sqs\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request_target(&request) {
            "AmazonSQS.CreateQueue" => {
                let body: serde_json::Value =
                    serde_json::from_slice(&request.body).expect("CreateQueue body must be JSON");
                let attributes = body
                    .get("Attributes")
                    .and_then(serde_json::Value::as_object)
                    .expect("round-trip fixture sends queue attributes");
                let queue_url = match self.0 {
                    SqsRoundTripCase::FifoDefaults => {
                        assert_eq!(
                            attributes.get("FifoQueue"),
                            Some(&serde_json::json!("true"))
                        );
                        assert_eq!(
                            attributes.get("ContentBasedDeduplication"),
                            Some(&serde_json::json!("true"))
                        );
                        assert!(!attributes.contains_key("DeduplicationScope"));
                        assert!(!attributes.contains_key("FifoThroughputLimit"));
                        FIFO_QUEUE_URL
                    }
                    SqsRoundTripCase::RedriveStringCount => {
                        let policy: serde_json::Value = serde_json::from_str(
                            attributes
                                .get("RedrivePolicy")
                                .and_then(serde_json::Value::as_str)
                                .expect("RedrivePolicy string"),
                        )
                        .expect("RedrivePolicy JSON");
                        assert_eq!(policy["maxReceiveCount"], 5);
                        QUEUE_URL
                    }
                    SqsRoundTripCase::KmsAlias => {
                        assert_eq!(
                            attributes.get("KmsMasterKeyId"),
                            Some(&serde_json::json!("alias/aws/sqs"))
                        );
                        QUEUE_URL
                    }
                };
                MockResponse::json(200, serde_json::json!({"QueueUrl": queue_url}).to_string())
            }
            "AmazonSQS.GetQueueAttributes" => {
                let mut attributes = serde_json::Map::from_iter([
                    (
                        "QueueArn".to_string(),
                        serde_json::json!(
                            "arn:aws:sqs:ap-northeast-1:123456789012:carina-read-roundtrip"
                        ),
                    ),
                    ("VisibilityTimeout".to_string(), serde_json::json!("30")),
                    (
                        "MaximumMessageSize".to_string(),
                        serde_json::json!("262144"),
                    ),
                    (
                        "MessageRetentionPeriod".to_string(),
                        serde_json::json!("345600"),
                    ),
                    ("DelaySeconds".to_string(), serde_json::json!("0")),
                    (
                        "ReceiveMessageWaitTimeSeconds".to_string(),
                        serde_json::json!("0"),
                    ),
                    (
                        "CreatedTimestamp".to_string(),
                        serde_json::json!("1793260800"),
                    ),
                    (
                        "LastModifiedTimestamp".to_string(),
                        serde_json::json!("1793260800"),
                    ),
                ]);
                match self.0 {
                    SqsRoundTripCase::FifoDefaults => {
                        attributes.insert("FifoQueue".to_string(), serde_json::json!("true"));
                        attributes.insert(
                            "ContentBasedDeduplication".to_string(),
                            serde_json::json!("true"),
                        );
                        attributes
                            .insert("DeduplicationScope".to_string(), serde_json::json!("queue"));
                        attributes.insert(
                            "FifoThroughputLimit".to_string(),
                            serde_json::json!("perQueue"),
                        );
                    }
                    SqsRoundTripCase::RedriveStringCount => {
                        attributes.insert(
                            "RedrivePolicy".to_string(),
                            serde_json::json!(
                                r#"{"deadLetterTargetArn":"arn:aws:sqs:ap-northeast-1:123456789012:dead","maxReceiveCount":"5"}"#
                            ),
                        );
                    }
                    SqsRoundTripCase::KmsAlias => {
                        attributes.insert(
                            "KmsMasterKeyId".to_string(),
                            serde_json::json!("alias/aws/sqs"),
                        );
                        attributes.insert(
                            "KmsDataKeyReusePeriodSeconds".to_string(),
                            serde_json::json!("300"),
                        );
                    }
                }
                MockResponse::json(
                    200,
                    serde_json::json!({"Attributes": attributes}).to_string(),
                )
            }
            "AmazonSQS.ListQueueTags" => MockResponse::json(200, r#"{"Tags":{}}"#),
            target => panic!("unexpected SQS operation {target}"),
        };
        Box::pin(async move { response })
    }
}

#[tokio::test]
async fn log_group_false_deletion_protection_round_trips() {
    let provider = provider_from_service(LogGroupService).await;
    let mut desired = Resource::with_provider("aws", "logs.LogGroup", "roundtrip", None);
    desired.set_attr("log_group_name", string(LOG_GROUP_NAME));
    desired.set_attr("deletion_protection_enabled", bool_value(false));
    desired.set_attr("tags", empty_map());

    let current = provider
        .read(
            &desired.id,
            &applied_identifier(LOG_GROUP_NAME),
            ReadRequest,
        )
        .await
        .expect("read log group")
        .into_state();

    assert_eq!(
        current.attributes.get("deletion_protection_enabled"),
        Some(&bool_value(false))
    );
    assert_eq!(current.attributes.get("tags"), Some(&empty_map()));
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn log_group_tag_side_read_failure_names_required_permission() {
    let provider = provider_from_service(LogGroupTagsDeniedService).await;
    let id = Resource::with_provider("aws", "logs.LogGroup", "roundtrip", None).id;

    let error = provider
        .read(&id, &applied_identifier(LOG_GROUP_NAME), ReadRequest)
        .await
        .expect_err("a denied ListTagsLogGroup call must fail the managed read");
    let message = error.to_string();
    assert!(message.contains("ListTagsLogGroup"), "{message}");
    assert!(message.contains("logs:ListTagsLogGroup"), "{message}");
}

#[tokio::test]
async fn unrelated_log_group_update_preserves_omitted_deletion_protection() {
    let provider = provider_from_service(ProtectedLogGroupUpdateService).await;
    let id = Resource::with_provider("aws", "logs.LogGroup", "roundtrip", None).id;
    let from = test_existing_state(
        id.clone(),
        HashMap::from([
            ("log_group_name".to_string(), string(LOG_GROUP_NAME)),
            ("deletion_protection_enabled".to_string(), bool_value(true)),
            ("tags".to_string(), empty_map()),
        ]),
    )
    .with_identifier(LOG_GROUP_NAME);
    let mut desired = Resource::from_id(id.clone());
    desired.set_attr("log_group_name", string(LOG_GROUP_NAME));
    desired.set_attr(
        "tags",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "purpose".to_string(),
            string("roundtrip"),
        )]))),
    );
    let request = update_request_for_test(desired, from, &["tags".to_string()]).await;

    let state = provider
        .update(&id, &applied_identifier(LOG_GROUP_NAME), request)
        .await
        .expect("update tags without changing deletion protection")
        .into_state_for_writeback();

    assert_eq!(
        state.attributes.get("deletion_protection_enabled"),
        Some(&bool_value(true))
    );
}

#[tokio::test]
async fn certificate_sans_and_unknown_enum_value_round_trip() {
    let provider = provider_from_service(AcmService).await;
    let mut desired = Resource::with_provider("aws", "acm.Certificate", "roundtrip", None);
    desired.set_attr("domain_name", string("example.com"));
    desired.set_attr("subject_alternative_names", list(&["www.example.com"]));
    desired.set_attr("tags", empty_map());

    let current = provider
        .read(
            &desired.id,
            &applied_identifier(CERTIFICATE_ARN),
            ReadRequest,
        )
        .await
        .expect("read ACM certificate")
        .into_state();

    assert_eq!(
        current.attributes.get("subject_alternative_names"),
        Some(&list(&["www.example.com"]))
    );
    assert_eq!(
        current.attributes.get("key_algorithm"),
        Some(&string("POST_QUANTUM_2048")),
        "AWS values newer than the pinned closed enum must survive refresh"
    );
    assert_eq!(current.attributes.get("tags"), Some(&empty_map()));
    assert_no_diff(desired, current);
}

#[tokio::test(start_paused = true)]
async fn account_write_only_create_inputs_and_import_read_round_trip() {
    let provider = provider_from_service(OrganizationsAccountService).await;
    let mut desired = Resource::with_provider("aws", "organizations.Account", "member", None);
    desired.set_attr("account_name", string("member"));
    desired.set_attr("email", string("member@example.com"));
    desired.set_attr("role_name", string("OrganizationAccountAccessRole"));
    desired.set_attr("iam_user_access_to_billing", string("ALLOW"));
    desired.set_attr("tags", empty_map());
    let id = desired.id.clone();

    let created = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(
            &provider,
            &id,
            create_request_without_schema_for_test(desired.clone()).await,
        )
        .await
        .expect("create and read back Organizations account"),
    )
    .0;
    assert!(!created.attributes.contains_key("role_name"));
    assert!(
        !created
            .attributes
            .contains_key("iam_user_access_to_billing")
    );
    assert_no_diff(desired.clone(), created);

    let imported = provider
        .read(&id, &applied_identifier("123456789012"), ReadRequest)
        .await
        .expect("read imported Organizations account")
        .into_state();
    assert_eq!(
        imported.attributes.get("account_name"),
        Some(&string("member"))
    );
    assert_eq!(
        imported.attributes.get("email"),
        Some(&string("member@example.com"))
    );
    assert!(!imported.attributes.contains_key("role_name"));
    assert!(
        !imported
            .attributes
            .contains_key("iam_user_access_to_billing")
    );
    assert_no_diff(desired, imported);
}

#[test]
fn served_account_schema_updates_name_but_not_email_in_place() {
    let schema = carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .find(|schema| schema.parts().resource_type == "organizations.Account")
        .expect("served Organizations account schema");

    assert!(
        !schema
            .effective_attribute("account_name")
            .expect("account_name attribute")
            .is_create_only(),
        "account_name must not force account replacement"
    );
    assert!(
        schema
            .effective_attribute("email")
            .expect("email attribute")
            .is_create_only(),
        "email changes still require replacement"
    );
}

#[tokio::test]
async fn account_name_change_plans_an_update_instead_of_replacement() {
    let provider = provider_from_service(OrganizationsAccountService).await;
    let schemas = schema_registry();
    let desired = organizations_account_resource("desired-name");
    let current = organizations_account_state(&desired, "renamed-outside-carina");
    let current_states = into_plan_input_current_map(
        HashMap::from([(desired.id.clone(), Current::from_state(current))]),
        &schemas,
        std::slice::from_ref(&desired),
    );
    let managed = [ResolvedResource::new(desired)];
    let orphans = Orphans::new();
    let saved = RawSavedAttrs::default().lift(&schemas);

    let plan = create_plan(
        &managed,
        &[],
        &provider,
        PlanPriorStates::new(&current_states, &orphans),
        &HashMap::new(),
        &schemas,
        &saved,
        &HashMap::new(),
        &[],
    )
    .expect("plan Organizations account rename");

    assert!(
        matches!(
            plan.effects(),
            [Effect::Update {
                changed_attributes,
                ..
            }] if changed_attributes == &["account_name".to_string()]
        ),
        "account rename must be one in-place update, got {:?}",
        plan.effects()
    );
}

#[tokio::test]
async fn account_name_update_sends_put_account_name_and_read_converges() {
    let (provider, observed) =
        provider_with_account_rename("old-name", AccountRenameMode::Success).await;
    let desired = organizations_account_resource("new-name");
    let current = organizations_account_state(&desired, "old-name");
    let request =
        update_request_for_test(desired.clone(), current, &["account_name".to_string()]).await;

    let updated = provider
        .update(
            &desired.id,
            &applied_identifier(ORGANIZATIONS_ACCOUNT_ID),
            request,
        )
        .await
        .expect("rename Organizations account")
        .into_state_for_writeback();

    assert_eq!(
        observed.lock().expect("account rename state lock").calls,
        vec![(ORGANIZATIONS_ACCOUNT_ID.to_string(), "new-name".to_string())]
    );
    assert_eq!(
        updated.attributes.get("account_name"),
        Some(&string("new-name"))
    );
    assert_no_diff(desired, updated);
}

#[tokio::test]
async fn unchanged_account_name_sends_no_put_account_name() {
    let (provider, observed) =
        provider_with_account_rename("same-name", AccountRenameMode::Success).await;
    let desired = organizations_account_resource("same-name");
    let current = organizations_account_state(&desired, "same-name");
    let request =
        update_request_for_test(desired.clone(), current, &["account_name".to_string()]).await;

    provider
        .update(
            &desired.id,
            &applied_identifier(ORGANIZATIONS_ACCOUNT_ID),
            request,
        )
        .await
        .expect("unchanged account name update");

    assert!(
        observed
            .lock()
            .expect("account rename state lock")
            .calls
            .is_empty(),
        "an unchanged account name must not call PutAccountName"
    );
}

#[tokio::test]
async fn account_name_access_denied_error_names_required_setup() {
    let (provider, _observed) =
        provider_with_account_rename("old-name", AccountRenameMode::AccessDenied).await;
    let desired = organizations_account_resource("new-name");
    let current = organizations_account_state(&desired, "old-name");
    let request =
        update_request_for_test(desired.clone(), current, &["account_name".to_string()]).await;

    let error = provider
        .update(
            &desired.id,
            &applied_identifier(ORGANIZATIONS_ACCOUNT_ID),
            request,
        )
        .await
        .expect_err("AccessDenied must fail account rename");
    let message = error.to_string();
    assert!(message.contains("account:PutAccountName"), "{message}");
    assert!(message.contains("trusted access"), "{message}");
    assert!(
        message.contains(
            "aws organizations enable-aws-service-access --service-principal account.amazonaws.com"
        ),
        "{message}"
    );
}

#[tokio::test]
async fn vpc_ipam_pool_and_netmask_round_trip() {
    let provider = provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::VpcIpam)).await;
    let mut desired = Resource::with_provider("aws", "ec2.Vpc", "ipam", None);
    desired.set_attr("ipv4_ipam_pool_id", string("ipam-pool-0123456789abcdef0"));
    desired.set_attr(
        "ipv4_netmask_length",
        Value::Concrete(ConcreteValue::Int(16)),
    );
    desired.set_attr("tags", empty_map());

    let current = create_round_trip_state(&provider, &desired).await;
    assert_eq!(
        current.attributes.get("ipv4_netmask_length"),
        Some(&Value::Concrete(ConcreteValue::Int(16)))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn subnet_ipam_and_associated_ipv6_cidr_round_trip() {
    let provider =
        provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::SubnetIpamIpv6)).await;
    let mut desired = Resource::with_provider("aws", "ec2.Subnet", "ipam", None);
    desired.set_attr("vpc_id", string(VPC_ID));
    desired.set_attr("ipv4_ipam_pool_id", string("ipam-pool-0123456789abcdef0"));
    desired.set_attr(
        "ipv4_netmask_length",
        Value::Concrete(ConcreteValue::Int(24)),
    );
    desired.set_attr("ipv6_cidr_block", string("2001:0DB8:0:0:0:0:0:0/64"));
    desired.set_attr("tags", empty_map());
    let desired = normalize_desired_for_test(desired).await;
    assert_eq!(
        desired.get_attr("ipv6_cidr_block"),
        Some(&string("2001:db8::/64"))
    );

    let current = create_round_trip_state(&provider, &desired).await;
    assert_eq!(
        current.attributes.get("ipv6_cidr_block"),
        Some(&string("2001:db8::/64"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn eip_requested_address_round_trip() {
    let provider = provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::EipAddress)).await;
    let mut desired = Resource::with_provider("aws", "ec2.Eip", "address", None);
    desired.set_attr("address", string("198.51.100.42"));
    desired.set_attr("domain", string("vpc"));
    desired.set_attr("tags", empty_map());

    let current = create_round_trip_state(&provider, &desired).await;
    assert_eq!(
        current.attributes.get("public_ip"),
        Some(&string("198.51.100.42"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test(start_paused = true)]
async fn regional_nat_private_address_and_az_addresses_round_trip() {
    let provider = provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::NatRegional)).await;
    let mut desired = Resource::with_provider("aws", "ec2.NatGateway", "regional", None);
    desired.set_attr("availability_mode", string("regional"));
    desired.set_attr("connectivity_type", string("private"));
    desired.set_attr("vpc_id", string(VPC_ID));
    desired.set_attr("private_ip_address", string("10.42.0.5"));
    desired.set_attr(
        "availability_zone_addresses",
        Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
            ConcreteValue::Map(IndexMap::from([
                ("allocation_ids".to_string(), list(&[EIP_ALLOCATION_ID])),
                ("availability_zone".to_string(), string("ap-northeast-1a")),
            ])),
        )])),
    );
    desired.set_attr("tags", empty_map());

    let current = create_round_trip_state(&provider, &desired).await;
    assert!(!current.attributes.contains_key("subnet_id"));
    assert_eq!(
        current.attributes.get("private_ip_address"),
        Some(&string("10.42.0.5"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn security_group_source_owner_round_trip() {
    let provider =
        provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::SecurityGroupIngress)).await;
    let mut desired =
        Resource::with_provider("aws", "ec2.SecurityGroupIngress", "source-owner", None);
    desired.set_attr("group_id", string(SECURITY_GROUP_ID));
    desired.set_attr("ip_protocol", string("tcp"));
    desired.set_attr("from_port", Value::Concrete(ConcreteValue::Int(443)));
    desired.set_attr("to_port", Value::Concrete(ConcreteValue::Int(443)));
    desired.set_attr("source_security_group_id", string(SOURCE_SECURITY_GROUP_ID));
    desired.set_attr("source_security_group_owner_id", string("123456789012"));

    let current = create_round_trip_state(&provider, &desired).await;
    assert_eq!(
        current.attributes.get("source_security_group_owner_id"),
        Some(&string("123456789012"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn flow_log_resource_type_round_trip() {
    let provider = provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::FlowLog)).await;
    let mut desired = Resource::with_provider("aws", "ec2.FlowLog", "vpc", None);
    desired.set_attr("resource_ids", list(&[VPC_ID]));
    desired.set_attr("resource_type", string("VPC"));
    desired.set_attr("traffic_type", string("ALL"));
    desired.set_attr("tags", empty_map());

    let current = create_round_trip_state(&provider, &desired).await;
    assert_api_text(current.attributes.get("resource_type"), "VPC");
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn vpc_endpoint_new_create_attributes_round_trip() {
    let provider = provider_from_service(Ec2RoundTripService(Ec2RoundTripCase::VpcEndpoint)).await;
    let mut desired = Resource::with_provider("aws", "ec2.VpcEndpoint", "resource", None);
    desired.set_attr("vpc_id", string(VPC_ID));
    desired.set_attr(
        "service_name",
        string("com.amazonaws.vpce.ap-northeast-1.vpce-svc-0123456789abcdef0"),
    );
    desired.set_attr("vpc_endpoint_type", string("Resource"));
    desired.set_attr(
        "resource_configuration_arn",
        string(
            "arn:aws:vpc-lattice:ap-northeast-1:123456789012:resourceconfiguration/rcfg-0123456789abcdef0",
        ),
    );
    desired.set_attr(
        "service_network_arn",
        string(
            "arn:aws:vpc-lattice:ap-northeast-1:123456789012:servicenetwork/sn-0123456789abcdef0",
        ),
    );
    desired.set_attr("service_region", string(REGION));
    desired.set_attr("tags", empty_map());

    let current = create_round_trip_state(&provider, &desired).await;
    assert_eq!(
        current.attributes.get("resource_configuration_arn"),
        desired.get_attr("resource_configuration_arn")
    );
    assert_eq!(
        current.attributes.get("service_network_arn"),
        desired.get_attr("service_network_arn")
    );
    assert_api_text(current.attributes.get("service_region"), REGION);
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn standard_queue_false_fifo_flags_are_not_sent_and_round_trip() {
    let provider = provider_from_service(SqsService).await;
    let mut desired = Resource::with_provider("aws", "sqs.Queue", "roundtrip", None);
    desired.set_attr("queue_name", string(QUEUE_NAME));
    desired.set_attr("fifo_queue", bool_value(false));
    desired.set_attr("content_based_deduplication", bool_value(false));
    desired.set_attr("tags", empty_map());
    let id = desired.id.clone();
    let request = create_request_for_test(desired.clone()).await;

    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create standard queue"),
    )
    .0;

    assert_eq!(
        current.attributes.get("fifo_queue"),
        Some(&bool_value(false))
    );
    assert_eq!(
        current.attributes.get("content_based_deduplication"),
        Some(&bool_value(false))
    );
    assert_eq!(current.attributes.get("tags"), Some(&empty_map()));
    assert!(
        !current
            .attributes
            .contains_key("kms_data_key_reuse_period_seconds"),
        "GetQueueAttributes(All) omits the reuse period when no KMS key is configured"
    );
    for optional_attribute in [
        "sqs_managed_sse_enabled",
        "approximate_number_of_messages",
        "approximate_number_of_messages_delayed",
        "approximate_number_of_messages_not_visible",
    ] {
        assert!(
            !current.attributes.contains_key(optional_attribute),
            "GetQueueAttributes(All) may legitimately omit {optional_attribute}"
        );
    }
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn fifo_queue_aws_defaults_round_trip_without_becoming_desired() {
    let provider = provider_from_service(SqsRoundTripService(SqsRoundTripCase::FifoDefaults)).await;
    let mut desired = Resource::with_provider("aws", "sqs.Queue", "fifo", None);
    desired.set_attr("queue_name", string(FIFO_QUEUE_NAME));
    desired.set_attr("fifo_queue", bool_value(true));
    desired.set_attr("content_based_deduplication", bool_value(true));
    desired.set_attr("tags", empty_map());
    let id = desired.id.clone();
    let request = create_request_for_test(desired.clone()).await;
    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create FIFO queue"),
    )
    .0;

    assert_eq!(
        current.attributes.get("fifo_queue"),
        Some(&bool_value(true))
    );
    assert_eq!(
        current.attributes.get("content_based_deduplication"),
        Some(&bool_value(true))
    );
    assert_eq!(
        current.attributes.get("deduplication_scope"),
        Some(&string("queue"))
    );
    assert_eq!(
        current.attributes.get("fifo_throughput_limit"),
        Some(&string("perQueue"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn redrive_numeric_string_count_round_trips_as_an_integer() {
    let provider =
        provider_from_service(SqsRoundTripService(SqsRoundTripCase::RedriveStringCount)).await;
    let mut desired = Resource::with_provider("aws", "sqs.Queue", "redrive", None);
    desired.set_attr("queue_name", string(QUEUE_NAME));
    desired.set_attr(
        "redrive_policy",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "dead_letter_target_arn".to_string(),
                string("arn:aws:sqs:ap-northeast-1:123456789012:dead"),
            ),
            (
                "max_receive_count".to_string(),
                Value::Concrete(ConcreteValue::Int(5)),
            ),
        ]))),
    );
    desired.set_attr("tags", empty_map());
    let id = desired.id.clone();
    let request = create_request_for_test(desired.clone()).await;
    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create redrive queue"),
    )
    .0;

    assert_eq!(
        current.attributes.get("redrive_policy"),
        desired.get_attr("redrive_policy")
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn kms_alias_round_trips_without_arn_rewriting() {
    let provider = provider_from_service(SqsRoundTripService(SqsRoundTripCase::KmsAlias)).await;
    let mut desired = Resource::with_provider("aws", "sqs.Queue", "kms", None);
    desired.set_attr("queue_name", string(QUEUE_NAME));
    desired.set_attr("kms_master_key_id", string("alias/aws/sqs"));
    desired.set_attr("tags", empty_map());
    let id = desired.id.clone();
    let request = create_request_for_test(desired.clone()).await;
    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create KMS-encrypted queue"),
    )
    .0;

    assert_eq!(
        current.attributes.get("kms_master_key_id"),
        Some(&string("alias/aws/sqs"))
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn queue_tag_side_read_failure_names_required_permission() {
    let provider = provider_from_service(SqsTagsDeniedService).await;
    let id = Resource::with_provider("aws", "sqs.Queue", "roundtrip", None).id;

    let error = provider
        .read(&id, &applied_identifier(QUEUE_URL), ReadRequest)
        .await
        .expect_err("a denied ListQueueTags call must fail the managed read");
    let message = error.to_string();
    assert!(message.contains("ListQueueTags"), "{message}");
    assert!(message.contains("sqs:ListQueueTags"), "{message}");
}

#[tokio::test]
async fn queue_kms_reuse_period_without_master_key_is_rejected_before_dispatch() {
    let mut resource = Resource::with_provider("aws", "sqs.Queue", "invalid-kms", None);
    resource.set_attr("queue_name", string(QUEUE_NAME));
    resource.set_attr(
        "kms_data_key_reuse_period_seconds",
        Value::Concrete(ConcreteValue::Int(300)),
    );
    let mut resources = vec![resource.into()];

    let error = AwsNormalizer
        .normalize_desired(&mut resources)
        .await
        .expect_err("reuse period without kms_master_key_id must be rejected");
    let message = error.to_string();
    assert!(
        message.contains("kms_data_key_reuse_period_seconds"),
        "{message}"
    );
    assert!(message.contains("kms_master_key_id"), "{message}");
}

#[tokio::test]
async fn replication_omitted_delete_marker_default_has_no_diff() {
    let provider = provider_with_replication_bucket().await;
    let rule = Value::Concrete(ConcreteValue::Map(IndexMap::from([
        ("id".to_string(), string("replicate-all")),
        ("status".to_string(), string("Enabled")),
        (
            "destination".to_string(),
            Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "bucket".to_string(),
                string("arn:aws:s3:::destination"),
            )]))),
        ),
    ])));
    let mut desired = Resource::with_provider(
        "aws",
        "s3.BucketReplicationConfiguration",
        "roundtrip",
        None,
    );
    desired.set_attr("bucket", string(REPLICATION_BUCKET));
    desired.set_attr("role", string("arn:aws:iam::123456789012:role/replication"));
    desired.set_attr("rules", Value::Concrete(ConcreteValue::List(vec![rule])));
    let mut normalized = vec![desired.clone().into()];
    AwsNormalizer
        .normalize_desired(&mut normalized)
        .await
        .expect("replication defaults should normalize");
    desired.attributes = normalized.remove(0).attributes;

    let Value::Concrete(ConcreteValue::List(rules)) = desired
        .get_attr("rules")
        .expect("normalized replication rules")
    else {
        panic!("rules must stay a list");
    };
    let Value::Concrete(ConcreteValue::Map(rule)) = &rules[0] else {
        panic!("rule must stay a map");
    };
    assert_eq!(
        rule.get("delete_marker_replication"),
        Some(&Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "status".to_string(),
            string("Disabled"),
        )]))))
    );

    let id = desired.id.clone();
    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(
            &provider,
            &id,
            create_request_without_schema_for_test(desired.clone()).await,
        )
        .await
        .expect("put and read back S3 replication configuration"),
    )
    .0;
    assert_no_diff(desired, current);
}
