//! Provider-level coverage for host-driven pending operations against a
//! scripted in-process EC2 Query service. No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aws_sdk_acm::Client as AcmClient;
use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_identitystore::Client as IdentityStoreClient;
use aws_sdk_organizations::Client as OrganizationsClient;
use aws_sdk_route53::Client as Route53Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sts::Client as StsClient;
use carina_core::provider::{
    AppliedIdentifier, AppliedState, CreateRequest, DeleteRequest, Observation, ProviderError,
    ReadOutcome, Sent,
};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};
use carina_provider_aws::{
    AwsCreateAttempt, AwsCreatePoll, AwsCreateResume, AwsCreateToken, AwsDeleteToken,
    AwsIssuedCreate, AwsNormalizer, AwsPollAttempt, AwsProvider, AwsResumeAttempt,
    AwsResumedCreate,
};
use carina_provider_protocol::OperationStep;
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

const REGION: &str = "us-east-1";
const NAT_GATEWAY_ID: &str = "nat-0123456789abcdef0";
const SUBNET_ID: &str = "subnet-0123456789abcdef0";
const RESOURCE_TYPE: &str = "ec2.NatGateway";
const FLOW_LOG_ID: &str = "fl-0123456789abcdef0";
const FLOW_LOG_RESOURCE_ID: &str = "vpc-0123456789abcdef0";
const REJECTED_FLOW_LOG_RESOURCE_ID: &str = "vpc-0fedcba9876543210";

#[derive(Debug)]
enum TestCreateStep {
    Success(AppliedState),
    Partial {
        state: AppliedState,
        reason: String,
        missing_attributes: Vec<String>,
    },
    Pending {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
    },
}

#[derive(Debug)]
enum TestCreateVerdict {
    Untracked(ProviderError),
    Created(Box<TestCreatedVerdict>),
    Discarded(ProviderError),
}

#[derive(Debug)]
struct TestCreatedVerdict {
    identifier: AppliedIdentifier,
    observation: Observation,
    sent: Sent,
    error: ProviderError,
}

impl TestCreateVerdict {
    fn provider_error(&self) -> &ProviderError {
        match self {
            Self::Untracked(error) | Self::Discarded(error) => error,
            Self::Created(created) => &created.error,
        }
    }
}

struct TestCreateAttempt {
    id: ResourceId,
}

struct TestPollAttempt {
    id: ResourceId,
}

struct TestIssuedCreate {
    id: ResourceId,
    identifier: AppliedIdentifier,
    sent: Sent,
}

impl AwsCreateAttempt for TestCreateAttempt {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;
    type Issued = TestIssuedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::Untracked(error)
    }

    fn issued(self, identifier: AppliedIdentifier, sent: Sent) -> Self::Issued {
        TestIssuedCreate {
            id: self.id,
            identifier,
            sent,
        }
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _sent: Sent,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token,
            identifier: None,
        })
    }
}

impl AwsPollAttempt for TestPollAttempt {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;
    type Issued = TestIssuedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::Untracked(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Issued {
        TestIssuedCreate {
            id: self.id,
            identifier,
            sent: Sent::AllRequestAttributes,
        }
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token,
            identifier: None,
        })
    }
}

impl AwsIssuedCreate for TestIssuedCreate {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    fn succeeded(self, attributes: std::collections::HashMap<String, Value>) -> Self::Step {
        TestCreateStep::Success(AppliedState::from_identifier(
            self.id,
            self.identifier,
            attributes,
        ))
    }

    fn partial(
        self,
        attributes: std::collections::HashMap<String, Value>,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self::Step {
        TestCreateStep::Partial {
            state: AppliedState::from_identifier(self.id, self.identifier, attributes),
            reason,
            missing_attributes,
        }
    }

    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::Created(Box::new(TestCreatedVerdict {
            identifier: self.identifier,
            observation,
            sent: self.sent,
            error,
        }))
    }

    fn discarded(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::Discarded(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token,
            identifier: Some(self.identifier),
        })
    }
}

#[derive(Debug)]
enum TestResumeStep {
    Completed(AppliedState),
    Pending {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
    },
}

#[derive(Debug)]
enum TestResumeVerdict {
    Untracked(ProviderError),
    Undetermined(ProviderError),
    Created(ProviderError),
    Discarded(ProviderError),
}

impl TestResumeVerdict {
    fn provider_error(&self) -> &ProviderError {
        match self {
            Self::Untracked(error)
            | Self::Undetermined(error)
            | Self::Created(error)
            | Self::Discarded(error) => error,
        }
    }
}

struct TestResumeAttempt;

struct TestResumedCreate {
    identifier: AppliedIdentifier,
}

impl AwsResumeAttempt for TestResumeAttempt {
    type Step = TestResumeStep;
    type Verdict = TestResumeVerdict;
    type Resumed = TestResumedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::Untracked(error)
    }

    fn undetermined(self, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::Undetermined(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Resumed {
        TestResumedCreate { identifier }
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Pending {
            token,
            identifier: None,
        })
    }
}

impl AwsResumedCreate for TestResumedCreate {
    type Step = TestResumeStep;
    type Verdict = TestResumeVerdict;

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    fn completed(self, state: AppliedState) -> Self::Step {
        TestResumeStep::Completed(state)
    }

    fn failed(self, _observation: Observation, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::Created(error)
    }

    fn discarded(self, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::Discarded(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Pending {
            token,
            identifier: Some(self.identifier),
        })
    }
}

struct ScriptedEc2Service {
    describe_states: Mutex<VecDeque<String>>,
    reject_create: bool,
    reject_tags: bool,
    create_nat_gateway_calls: AtomicUsize,
    create_tags_calls: AtomicUsize,
    describe_nat_gateways_calls: AtomicUsize,
    delete_nat_gateway_calls: AtomicUsize,
}

impl ScriptedEc2Service {
    fn new(describe_states: &[&str]) -> Self {
        Self::new_with_failures(describe_states, false, false)
    }

    fn new_with_failures(describe_states: &[&str], reject_create: bool, reject_tags: bool) -> Self {
        assert!(
            !describe_states.is_empty(),
            "a scripted service needs at least one describe state"
        );
        Self {
            describe_states: Mutex::new(
                describe_states
                    .iter()
                    .map(|state| (*state).to_string())
                    .collect(),
            ),
            reject_create,
            reject_tags,
            create_nat_gateway_calls: AtomicUsize::new(0),
            create_tags_calls: AtomicUsize::new(0),
            describe_nat_gateways_calls: AtomicUsize::new(0),
            delete_nat_gateway_calls: AtomicUsize::new(0),
        }
    }

    fn create_nat_gateway_calls(&self) -> usize {
        self.create_nat_gateway_calls.load(Ordering::Relaxed)
    }

    fn create_tags_calls(&self) -> usize {
        self.create_tags_calls.load(Ordering::Relaxed)
    }

    fn describe_nat_gateways_calls(&self) -> usize {
        self.describe_nat_gateways_calls.load(Ordering::Relaxed)
    }

    fn delete_nat_gateway_calls(&self) -> usize {
        self.delete_nat_gateway_calls.load(Ordering::Relaxed)
    }

    fn next_describe_state(&self) -> String {
        let mut states = self
            .describe_states
            .lock()
            .expect("describe-state script lock");
        if states.len() > 1 {
            states.pop_front().expect("non-empty describe-state script")
        } else {
            states
                .front()
                .expect("non-empty describe-state script")
                .clone()
        }
    }

    fn response_for(&self, request: &MockRequest) -> MockResponse {
        assert_eq!(request.method, "POST", "EC2 Query requests use POST");
        match query_action(request) {
            "CreateNatGateway" => {
                self.create_nat_gateway_calls
                    .fetch_add(1, Ordering::Relaxed);
                if self.reject_create {
                    MockResponse::error(400, "InvalidParameterValue", "scripted create rejection")
                } else {
                    MockResponse::xml(200, create_nat_gateway_response())
                }
            }
            "CreateTags" => {
                self.create_tags_calls.fetch_add(1, Ordering::Relaxed);
                if self.reject_tags {
                    MockResponse::error(400, "InvalidParameterValue", "scripted tag rejection")
                } else {
                    MockResponse::xml(
                        200,
                        r#"<CreateTagsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"/>"#,
                    )
                }
            }
            "DescribeNatGateways" => {
                self.describe_nat_gateways_calls
                    .fetch_add(1, Ordering::Relaxed);
                let state = self.next_describe_state();
                if state == "read_error" {
                    MockResponse::error(
                        400,
                        "InvalidParameterValue",
                        "scripted NAT gateway read-back failure",
                    )
                } else {
                    MockResponse::xml(200, describe_nat_gateways_response(&state))
                }
            }
            "DeleteNatGateway" => {
                self.delete_nat_gateway_calls
                    .fetch_add(1, Ordering::Relaxed);
                MockResponse::xml(200, delete_nat_gateway_response())
            }
            action => panic!("unexpected EC2 action {action}: {:?}", request.body),
        }
    }
}

impl MockService for ScriptedEc2Service {
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
        let response = self.response_for(&request);
        Box::pin(async move { response })
    }
}

struct PartialFlowLogEc2Service;

impl PartialFlowLogEc2Service {
    fn response_for(request: &MockRequest) -> MockResponse {
        assert_eq!(request.method, "POST", "EC2 Query requests use POST");
        match query_action(request) {
            "CreateFlowLogs" => MockResponse::xml(200, partial_create_flow_logs_response()),
            "DescribeFlowLogs" => MockResponse::xml(200, empty_describe_flow_logs_response()),
            action => panic!("unexpected EC2 action {action}: {:?}", request.body),
        }
    }
}

impl MockService for PartialFlowLogEc2Service {
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
        let response = Self::response_for(&request);
        Box::pin(async move { response })
    }
}

fn query_action(request: &MockRequest) -> &str {
    str::from_utf8(&request.body)
        .expect("EC2 Query body should be UTF-8")
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("Action="))
        .expect("EC2 Query body should contain Action")
}

fn create_nat_gateway_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000001</requestId>
  <natGateway>
    <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
    <state>pending</state>
    <subnetId>{SUBNET_ID}</subnetId>
  </natGateway>
</CreateNatGatewayResponse>"#
    )
}

fn describe_nat_gateways_response(state: &str) -> String {
    if state == "not_found" {
        return r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeNatGatewaysResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000002</requestId>
  <natGatewaySet/>
</DescribeNatGatewaysResponse>"#
            .to_string();
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeNatGatewaysResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000002</requestId>
  <natGatewaySet>
    <item>
      <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
      <state>{state}</state>
      <subnetId>{SUBNET_ID}</subnetId>
    </item>
  </natGatewaySet>
</DescribeNatGatewaysResponse>"#
    )
}

fn delete_nat_gateway_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000003</requestId>
  <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
</DeleteNatGatewayResponse>"#
    )
}

fn partial_create_flow_logs_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000004</requestId>
  <flowLogIdSet>
    <item>{FLOW_LOG_ID}</item>
  </flowLogIdSet>
  <unsuccessful>
    <item>
      <error>
        <code>InvalidParameterValue</code>
        <message>scripted per-resource rejection</message>
      </error>
      <resourceId>{REJECTED_FLOW_LOG_RESOURCE_ID}</resourceId>
    </item>
  </unsuccessful>
</CreateFlowLogsResponse>"#
    )
}

fn empty_describe_flow_logs_response() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000005</requestId>
  <flowLogSet/>
</DescribeFlowLogsResponse>"#
}

fn provider_from_config(sdk_config: &aws_types::SdkConfig) -> AwsProvider {
    AwsProvider::from_clients(
        REGION.to_string(),
        Vec::new(),
        Vec::new(),
        S3Client::new(sdk_config),
        Ec2Client::new(sdk_config),
        IamClient::new(sdk_config),
        CloudWatchLogsClient::new(sdk_config),
        StsClient::new(sdk_config),
        OrganizationsClient::new(sdk_config),
        IdentityStoreClient::new(sdk_config),
        Route53Client::new(sdk_config),
        AcmClient::new(sdk_config),
        SqsClient::new(sdk_config),
    )
}

async fn provider_with_ec2(describe_states: &[&str]) -> (AwsProvider, Arc<ScriptedEc2Service>) {
    let service = Arc::new(ScriptedEc2Service::new(describe_states));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    (provider_from_config(&sdk_config), service)
}

async fn provider_with_ec2_failures(
    describe_states: &[&str],
    reject_create: bool,
    reject_tags: bool,
) -> (AwsProvider, Arc<ScriptedEc2Service>) {
    let service = Arc::new(ScriptedEc2Service::new_with_failures(
        describe_states,
        reject_create,
        reject_tags,
    ));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    (provider_from_config(&sdk_config), service)
}

async fn provider_with_partial_flow_log() -> AwsProvider {
    let mock = MockAws::builder()
        .with_service(PartialFlowLogEc2Service)
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    provider_from_config(&sdk_config)
}

async fn provider_without_services() -> AwsProvider {
    let mock = MockAws::builder().build();
    let sdk_config = mock.sdk_config(REGION).await;
    provider_from_config(&sdk_config)
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn nat_gateway_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "pending", None);
    resource.set_attr("subnet_id", string(SUBNET_ID));
    resource.set_attr(
        "tags",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "Name".to_string(),
            string("pending-operation-test"),
        )]))),
    );
    resource
}

fn flow_log_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", "ec2.FlowLog", "partial", None);
    resource.set_attr(
        "resource_ids",
        Value::Concrete(ConcreteValue::List(vec![
            string(FLOW_LOG_RESOURCE_ID),
            string(REJECTED_FLOW_LOG_RESOURCE_ID),
        ])),
    );
    resource.set_attr("resource_type", string("VPC"));
    resource.set_attr("traffic_type", string("ALL"));
    resource
}

fn schema_registry() -> carina_core::schema::SchemaRegistry {
    let mut schemas = carina_core::schema::SchemaRegistry::new();
    for schema in carina_provider_aws::schemas::all_schemas() {
        schemas.insert("aws", schema);
    }
    schemas
}

async fn create_request_for_test(resource: Resource) -> CreateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::new(&[]);
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
        .expect("test NAT gateway should pass checked create preparation")
}

fn expect_create_pending(step: TestCreateStep) -> AwsCreateToken {
    match step {
        TestCreateStep::Pending { token, identifier } => {
            assert_eq!(
                token,
                AwsCreateToken::NatGateway {
                    nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
                }
            );
            assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
            token
        }
        outcome => panic!("expected pending create, got {outcome:?}"),
    }
}

fn expect_delete_pending(step: OperationStep<(), ProviderError, AwsDeleteToken>) -> AwsDeleteToken {
    match step {
        OperationStep::Pending {
            pending,
            poll_after_secs,
        } => {
            assert_eq!(poll_after_secs, 5);
            assert_eq!(
                pending,
                AwsDeleteToken::NatGateway {
                    nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
                }
            );
            pending
        }
        OperationStep::Done(()) => panic!("expected pending delete"),
        OperationStep::Verdict(error) => panic!("expected pending delete, got {error:?}"),
    }
}

fn poll_attempt(resource: &Resource) -> AwsCreatePoll<TestPollAttempt, TestIssuedCreate> {
    AwsCreatePoll::Unissued(TestPollAttempt {
        id: resource.id.clone(),
    })
}

fn issued_poll(
    resource: &Resource,
    sent: Sent,
) -> AwsCreatePoll<TestPollAttempt, TestIssuedCreate> {
    AwsCreatePoll::Issued(TestIssuedCreate {
        id: resource.id.clone(),
        identifier: applied_identifier(NAT_GATEWAY_ID),
        sent,
    })
}

fn issued_resume() -> AwsCreateResume<TestResumeAttempt, TestResumedCreate> {
    AwsCreateResume::Issued(TestResumedCreate {
        identifier: applied_identifier(NAT_GATEWAY_ID),
    })
}

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
}

fn assert_nat_gateway_state_identifier(state: &AppliedState) {
    assert_eq!(state.identifier().as_str(), NAT_GATEWAY_ID);
}

#[tokio::test]
async fn rejected_create_is_untracked() {
    let (provider, service) = provider_with_ec2_failures(&["pending"], true, false).await;
    let resource = nat_gateway_resource();

    let verdict = provider
        .create_step(
            &resource,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect_err("the scripted CreateNatGateway rejection must fail");

    assert!(matches!(verdict, TestCreateVerdict::Untracked(_)));
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 0);
}

#[tokio::test]
async fn accepted_create_then_transient_waiter_failure_stays_in_flight_with_identifier() {
    let (provider, _service) = provider_with_ec2(&["read_error"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let step = provider
        .poll_create_step(
            &resource,
            token.clone(),
            issued_poll(&resource, Sent::only(["subnet_id"])),
        )
        .await
        .expect("a transient waiter error must re-pend");

    let TestCreateStep::Pending {
        token: repended,
        identifier,
    } = step
    else {
        panic!("expected the transient waiter failure to stay pending");
    };
    assert_eq!(repended, token);
    assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
}

#[tokio::test]
async fn accepted_create_then_post_create_failure_is_created_and_excludes_failed_step_from_sent() {
    let (provider, service) =
        provider_with_ec2_failures(&["available", "available"], false, true).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let verdict = provider
        .poll_create_step(
            &resource,
            token,
            issued_poll(&resource, Sent::only(["subnet_id"])),
        )
        .await
        .expect_err("the scripted CreateTags rejection must fail after issue");

    let TestCreateVerdict::Created(created) = verdict else {
        panic!("a post-create failure must be classified as created");
    };
    let TestCreatedVerdict {
        identifier,
        observation,
        sent,
        error: _,
    } = *created;
    assert_eq!(identifier.as_str(), NAT_GATEWAY_ID);
    assert!(matches!(
        observation,
        Observation::Read(ReadOutcome::Found { .. })
    ));
    assert_eq!(sent, Sent::only(["subnet_id"]));
    let Sent::Only(names) = sent else {
        panic!("NAT gateway base create must report an explicit sent subset");
    };
    assert!(!names.contains("tags"));
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn partial_flow_log_create_excludes_resource_ids_from_sent_when_read_back_is_absent() {
    let provider = provider_with_partial_flow_log().await;
    let resource = flow_log_resource();

    let verdict = provider
        .create_step(
            &resource,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect_err("one rejected resource ID must make the issued create a created verdict");

    let TestCreateVerdict::Created(created) = verdict else {
        panic!("partial CreateFlowLogs must retain the IDs that AWS did create");
    };
    assert_eq!(created.identifier.as_str(), FLOW_LOG_ID);
    assert!(matches!(
        created.observation,
        Observation::Read(ReadOutcome::NotFound { .. })
    ));
    assert_eq!(
        created.sent,
        Sent::only(["resource_type", "traffic_type"]),
        "the rejected resource_ids value must not be recorded as applied"
    );
}

#[tokio::test]
async fn post_create_failure_with_failed_read_back_is_created_with_read_failed_observation() {
    let (provider, _service) =
        provider_with_ec2_failures(&["available", "read_error"], false, true).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let verdict = provider
        .poll_create_step(
            &resource,
            token,
            issued_poll(&resource, Sent::only(["subnet_id"])),
        )
        .await
        .expect_err("the scripted CreateTags rejection must fail after issue");

    let TestCreateVerdict::Created(created) = verdict else {
        panic!("a post-create failure must be classified as created");
    };
    assert!(matches!(created.observation, Observation::ReadFailed(_)));
}

#[tokio::test]
async fn resume_of_issued_create_still_creating_repends_with_same_identifier() {
    let (provider, _service) = provider_with_ec2(&["pending"]).await;
    let resource = nat_gateway_resource();
    let token = AwsCreateToken::NatGateway {
        nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
    };

    let step = provider
        .resume_create_step(&resource, token.clone(), issued_resume())
        .await
        .expect("a still-creating NAT gateway must re-pend");
    let TestResumeStep::Pending {
        token: repended,
        identifier,
    } = step
    else {
        panic!("expected resumed NAT gateway to remain pending");
    };
    assert_eq!(repended, token);
    assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
}

#[tokio::test]
async fn resume_of_issued_create_found_completes() {
    let (provider, service) = provider_with_ec2(&["available", "available"]).await;
    let resource = nat_gateway_resource();
    let token = AwsCreateToken::NatGateway {
        nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
    };

    let step = provider
        .resume_create_step(&resource, token, issued_resume())
        .await
        .expect("an available resumed NAT gateway must complete");
    let TestResumeStep::Completed(state) = step else {
        panic!("expected resumed NAT gateway completion");
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn resume_transient_error_without_identifier_is_undetermined() {
    let provider = provider_without_services().await;
    let resource = Resource::with_provider("aws", "organizations.Account", "resume", None);
    let token = AwsCreateToken::OrganizationsAccount {
        create_account_request_id: "car-test-request".to_string(),
    };
    let resume = AwsCreateResume::Unissued(TestResumeAttempt);

    let verdict = provider
        .resume_create_step(&resource, token, resume)
        .await
        .expect_err("a transient unissued resume error must be undetermined");
    assert!(matches!(verdict, TestResumeVerdict::Undetermined(_)));
    assert!(!verdict.provider_error().to_wire().message.is_empty());
}

#[tokio::test]
async fn nat_gateway_create_steps_issue_once_poll_once_and_read_back() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();

    let mut token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 0);
    assert_eq!(service.describe_nat_gateways_calls(), 0);

    token = expect_create_pending(
        provider
            .poll_create_step(&resource, token, poll_attempt(&resource))
            .await
            .expect("first NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 1);

    token = expect_create_pending(
        provider
            .poll_create_step(&resource, token, poll_attempt(&resource))
            .await
            .expect("second NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 2);

    let state = match provider
        .poll_create_step(&resource, token, poll_attempt(&resource))
        .await
        .expect("available NAT gateway create poll")
    {
        TestCreateStep::Success(state) => state,
        other => panic!("expected completed NAT gateway create, got {other:?}"),
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(
        service.describe_nat_gateways_calls(),
        4,
        "the available poll must perform one describe and one read-back describe"
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn nat_gateway_create_read_back_not_found_is_partial_success_with_created_identifier() {
    let (provider, service) = provider_with_ec2(&["available", "not_found"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let outcome = provider
        .poll_create_step(&resource, token, poll_attempt(&resource))
        .await
        .expect("the create API succeeded, so a missing read-back must stay successful");
    let TestCreateStep::Partial {
        state,
        reason,
        missing_attributes,
    } = outcome
    else {
        panic!("expected partial create success, got {outcome:?}");
    };
    assert_nat_gateway_state_identifier(&state);
    assert!(reason.contains("post-create read-back"));
    assert!(!missing_attributes.is_empty());
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 2);
}

#[tokio::test]
async fn nat_gateway_create_read_back_error_is_partial_success_with_created_identifier() {
    let (provider, service) = provider_with_ec2(&["available", "read_error"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let outcome = provider
        .poll_create_step(&resource, token, poll_attempt(&resource))
        .await
        .expect("the create API succeeded, so a failed read-back must stay successful");
    let TestCreateStep::Partial {
        state,
        reason,
        missing_attributes,
    } = outcome
    else {
        panic!("expected partial create success, got {outcome:?}");
    };
    assert_nat_gateway_state_identifier(&state);
    assert!(reason.contains("post-create read-back"));
    assert!(reason.contains("Failed to describe NAT gateway"));
    assert!(!missing_attributes.is_empty());
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 2);
}

#[tokio::test]
async fn nat_gateway_failed_create_poll_returns_api_error() {
    let (provider, service) = provider_with_ec2(&["failed"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &resource,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let error = provider
        .poll_create_step(&resource, token, poll_attempt(&resource))
        .await
        .expect_err("failed NAT gateway should fail create polling");
    assert_eq!(error.provider_error().variant_name(), "api_error");
    assert_eq!(
        error.provider_error().to_wire().message,
        "NAT gateway creation failed"
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 1);
}

#[tokio::test]
async fn nat_gateway_delete_steps_issue_once_and_poll_to_gone() {
    let (provider, service) = provider_with_ec2(&["deleting", "deleted"]).await;
    let id = ResourceId::with_provider_identity("aws", RESOURCE_TYPE, "pending", None);
    let request = DeleteRequest::default();

    let mut token = expect_delete_pending(
        provider
            .delete_step(&id, &applied_identifier(NAT_GATEWAY_ID), &request)
            .await
            .expect("start NAT gateway delete"),
    );
    assert_eq!(service.delete_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 0);

    token = expect_delete_pending(
        provider
            .poll_delete_step(&id, token)
            .await
            .expect("deleting NAT gateway poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 1);

    let deleted = provider
        .poll_delete_step(&id, token)
        .await
        .expect("deleted NAT gateway poll");
    assert!(matches!(deleted, OperationStep::Done(())));
    assert_eq!(service.describe_nat_gateways_calls(), 2);
    assert_eq!(service.delete_nat_gateway_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn native_provider_create_drives_nat_gateway_to_read_back_state() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let state = carina_core::executor::dispatch_create(&provider, &id, request)
        .await
        .expect("drive native NAT gateway create")
        .into_state_for_writeback();

    assert_eq!(state.identifier.as_deref(), Some(NAT_GATEWAY_ID));
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 4);
}
