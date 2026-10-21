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
use carina_core::provider::{CreateOutcome, CreateRequest, DeleteRequest, Provider};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};
use carina_provider_aws::{AwsCreateToken, AwsDeleteToken, AwsNormalizer, AwsProvider};
use carina_provider_protocol::OperationStep;
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

const REGION: &str = "us-east-1";
const NAT_GATEWAY_ID: &str = "nat-0123456789abcdef0";
const SUBNET_ID: &str = "subnet-0123456789abcdef0";
const RESOURCE_TYPE: &str = "ec2.NatGateway";

struct ScriptedEc2Service {
    describe_states: Mutex<VecDeque<String>>,
    create_nat_gateway_calls: AtomicUsize,
    create_tags_calls: AtomicUsize,
    describe_nat_gateways_calls: AtomicUsize,
    delete_nat_gateway_calls: AtomicUsize,
}

impl ScriptedEc2Service {
    fn new(describe_states: &[&str]) -> Self {
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
                MockResponse::xml(200, create_nat_gateway_response())
            }
            "CreateTags" => {
                self.create_tags_calls.fetch_add(1, Ordering::Relaxed);
                MockResponse::xml(
                    200,
                    r#"<CreateTagsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"/>"#,
                )
            }
            "DescribeNatGateways" => {
                self.describe_nat_gateways_calls
                    .fetch_add(1, Ordering::Relaxed);
                MockResponse::xml(
                    200,
                    describe_nat_gateways_response(&self.next_describe_state()),
                )
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

fn expect_create_pending(step: OperationStep<CreateOutcome, AwsCreateToken>) -> AwsCreateToken {
    match step {
        OperationStep::Pending {
            pending,
            poll_after_secs,
        } => {
            assert_eq!(poll_after_secs, 5);
            assert_eq!(
                pending,
                AwsCreateToken::NatGateway {
                    nat_gateway_id: NAT_GATEWAY_ID.to_string(),
                }
            );
            pending
        }
        OperationStep::Done(outcome) => panic!("expected pending create, got {outcome:?}"),
    }
}

fn expect_delete_pending(step: OperationStep<(), AwsDeleteToken>) -> AwsDeleteToken {
    match step {
        OperationStep::Pending {
            pending,
            poll_after_secs,
        } => {
            assert_eq!(poll_after_secs, 5);
            assert_eq!(
                pending,
                AwsDeleteToken::NatGateway {
                    nat_gateway_id: NAT_GATEWAY_ID.to_string(),
                }
            );
            pending
        }
        OperationStep::Done(()) => panic!("expected pending delete"),
    }
}

fn assert_nat_gateway_state_identifier(state: &carina_core::resource::State) {
    assert!(state.exists);
    assert_eq!(state.identifier.as_deref(), Some(NAT_GATEWAY_ID));
}

#[tokio::test]
async fn nat_gateway_create_steps_issue_once_poll_once_and_read_back() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();

    let mut token = expect_create_pending(
        provider
            .create_step(&resource)
            .await
            .expect("start NAT gateway create"),
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 0);

    token = expect_create_pending(
        provider
            .poll_create_step(&resource, token)
            .await
            .expect("first NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 1);

    token = expect_create_pending(
        provider
            .poll_create_step(&resource, token)
            .await
            .expect("second NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 2);

    let state = match provider
        .poll_create_step(&resource, token)
        .await
        .expect("available NAT gateway create poll")
    {
        OperationStep::Done(CreateOutcome::Success { state }) => state,
        other => panic!("expected completed NAT gateway create, got {other:?}"),
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(
        service.describe_nat_gateways_calls(),
        4,
        "the available poll must perform one describe and one read-back describe"
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
}

#[tokio::test]
async fn nat_gateway_failed_create_poll_returns_api_error() {
    let (provider, service) = provider_with_ec2(&["failed"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(&resource)
            .await
            .expect("start NAT gateway create"),
    );

    let error = provider
        .poll_create_step(&resource, token)
        .await
        .expect_err("failed NAT gateway should fail create polling");
    assert_eq!(error.variant_name(), "api_error");
    assert_eq!(error.to_wire().message, "NAT gateway creation failed");
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
            .delete_step(&id, NAT_GATEWAY_ID, &request)
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

    assert_eq!(
        provider
            .poll_delete_step(&id, token)
            .await
            .expect("deleted NAT gateway poll"),
        OperationStep::Done(())
    );
    assert_eq!(service.describe_nat_gateways_calls(), 2);
    assert_eq!(service.delete_nat_gateway_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn native_provider_create_drives_nat_gateway_to_read_back_state() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let state = provider
        .create(&id, request)
        .await
        .expect("drive native NAT gateway create")
        .into_state_for_writeback();

    assert_nat_gateway_state_identifier(&state);
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 4);
}
