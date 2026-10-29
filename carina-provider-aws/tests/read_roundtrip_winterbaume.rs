//! Round-trip regressions against scripted AWS protocol responses.
//! No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::str;

use carina_core::differ::{Diff, diff};
use carina_core::provider::{
    AppliedIdentifier, CreateRequest, Provider, ProviderNormalizer, ReadRequest, UpdateRequest,
};
use carina_core::resource::{ConcreteValue, ResolvedResource, Resource, State, Value};
use carina_core::schema::{SchemaKind, SchemaRegistry};
use carina_provider_aws::{AwsNormalizer, AwsProvider, BoundedSdkConfig};
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

const REGION: &str = "ap-northeast-1";
const LOG_GROUP_NAME: &str = "/carina/read-roundtrip";
const CERTIFICATE_ARN: &str =
    "arn:aws:acm:ap-northeast-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef";
const QUEUE_NAME: &str = "carina-read-roundtrip";
const QUEUE_URL: &str =
    "https://sqs.ap-northeast-1.amazonaws.com/123456789012/carina-read-roundtrip";

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

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
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
    let from = State::existing(
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
