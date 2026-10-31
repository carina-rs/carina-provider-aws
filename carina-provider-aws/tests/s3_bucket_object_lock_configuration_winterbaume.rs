//! Provider-level coverage for S3 bucket Object Lock against the in-process
//! winterbaume S3 service. No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::types::{
    BucketVersioningStatus, DefaultRetention, ObjectLockConfiguration, ObjectLockRetentionMode,
    ObjectLockRule, VersioningConfiguration,
};
use carina_core::provider::{
    AppliedIdentifier, CreateFailure, CreateOutcome, CreateRequest, DeleteRequest, NoopNormalizer,
    Provider, ProviderDirectiveSchema, ProviderError, ReadRequest, UpdateRequest,
};
use carina_core::resource::{
    ConcreteValue, Directives, Resource, ResourceIdentity, ResourceIdentityError, State, Value,
};
use carina_provider_aws::{AwsProvider, BoundedSdkConfig};
use indexmap::IndexMap;
use winterbaume_core::MockAws;
use winterbaume_s3::S3Service;

const TEST_REGION: &str = "us-east-1";
const RESOURCE_TYPE: &str = "s3.BucketObjectLockConfiguration";

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
}

fn delete_request() -> DeleteRequest {
    DeleteRequest::resolve(&Directives::default(), ProviderDirectiveSchema::Unavailable)
}

fn provider_from_config(sdk_config: aws_types::SdkConfig) -> AwsProvider {
    AwsProvider::from_sdk_config(
        TEST_REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    )
}

// This test-only raw client stages winterbaume prerequisites outside the
// provider; provider operations still use the generated guarded wrapper.
#[allow(clippy::disallowed_methods)]
async fn provider_with_s3() -> (AwsProvider, S3Client) {
    let mock = MockAws::builder().with_service(S3Service::new()).build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;
    let s3_client = S3Client::new(&sdk_config);
    (provider_from_config(sdk_config), s3_client)
}

async fn provider_without_services() -> AwsProvider {
    let mock = MockAws::builder().build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;
    provider_from_config(sdk_config)
}

async fn create_bucket(client: &S3Client, bucket: &str) {
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create test bucket");
}

async fn enable_versioning(client: &S3Client, bucket: &str) {
    client
        .put_bucket_versioning()
        .bucket(bucket)
        .versioning_configuration(
            VersioningConfiguration::builder()
                .status(BucketVersioningStatus::Enabled)
                .build(),
        )
        .send()
        .await
        .expect("enable test bucket versioning");
}

fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Concrete(ConcreteValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<IndexMap<_, _>>(),
    ))
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

fn assert_canonical_enum(value: Option<&Value>, expected: &str) {
    let Some(Value::Concrete(ConcreteValue::CanonicalEnum(value))) = value else {
        panic!("expected canonical enum {expected}, got {value:?}");
    };
    assert_eq!(value.api_value(), expected);
}

fn schema_registry() -> carina_core::schema::SchemaRegistry {
    carina_provider_aws::schemas::schema_registry()
}

async fn create_request_for_test(resource: Resource) -> CreateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
    let schemas = carina_core::schema::SchemaRegistry::new();
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
        .expect("test resource should pass checked create preparation")
}

async fn create_outcome_for_test(
    provider: &AwsProvider,
    id: &carina_core::resource::ResourceId,
    request: CreateRequest,
) -> Result<CreateOutcome, CreateFailure> {
    let (create, _cancellation) = carina_core::executor::dispatch_create(provider, id, request);
    create.await
}

async fn create_state_for_test(
    provider: &AwsProvider,
    id: &carina_core::resource::ResourceId,
    request: CreateRequest,
) -> Result<State, CreateFailure> {
    let accepted = carina_core::executor::AcceptedRequest::from_create_request(&request);
    create_outcome_for_test(provider, id, request)
        .await
        .map(|outcome| carina_core::executor::complete_create_for_writeback(outcome, accepted).0)
}

async fn update_request_for_test(
    mut resource: Resource,
    from: State,
    changed_attributes: &[String],
) -> UpdateRequest {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
    let schemas = schema_registry();
    let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
        vec![Box::new(carina_provider_aws::AwsProviderFactory)];
    carina_core::value::canonicalize_normalized_resources_with_schemas(
        std::slice::from_mut(&mut resource),
        &schemas,
    )
    .expect("test provider/state values should canonicalize for update preparation");
    let preparation = carina_core::executor::ProviderPreparationContext::new(
        &bindings,
        &module_gate,
        &[],
        &NoopNormalizer,
        &factories,
        &schemas,
    );
    carina_core::executor::prepare_update_request(resource, from, changed_attributes, &preparation)
        .await
        .expect("test resource should pass checked update preparation")
}

fn resource_from_state(state: &State) -> Resource {
    let mut resource = Resource::from_id(state.id.clone());
    resource.attributes = state.attributes.clone().into_iter().collect();
    resource
}

fn retention_rule(mode: &str, period_name: &'static str, period: i64) -> Value {
    map([(
        "default_retention",
        map([
            ("mode", string(mode)),
            ("retention_period", map([(period_name, int(period))])),
        ]),
    )])
}

fn object_lock_resource(bucket: &str, rule: Option<Value>) -> Resource {
    let identity = match ResourceIdentity::try_from(format!("lock-{bucket}")) {
        Ok(identity) => identity,
        Err(ResourceIdentityError::Empty) => {
            panic!("object-lock test identity derived from a non-empty bucket cannot be empty")
        }
    };
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, identity, None);
    resource.set_attr("bucket", string(bucket));
    resource.set_attr("object_lock_enabled", string("Enabled"));
    if let Some(rule) = rule {
        resource.set_attr("rule", rule);
    }
    resource
}

async fn create_object_lock(provider: &AwsProvider, bucket: &str, rule: Option<Value>) -> State {
    let resource = object_lock_resource(bucket, rule);
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;
    create_state_for_test(provider, &id, request)
        .await
        .expect("create Object Lock configuration")
}

fn assert_enabled_without_rule(state: &State) {
    assert!(state.exists);
    assert_canonical_enum(state.attributes.get("object_lock_enabled"), "Enabled");
    assert!(!state.attributes.contains_key("rule"));
}

fn assert_retention(state: &State, mode: &str, period_name: &str, period: i64) {
    let Some(Value::Concrete(ConcreteValue::Map(rule))) = state.attributes.get("rule") else {
        panic!("state should contain an object-lock rule: {state:?}");
    };
    let Some(Value::Concrete(ConcreteValue::Map(default_retention))) =
        rule.get("default_retention")
    else {
        panic!("rule should contain default_retention: {rule:?}");
    };
    assert_canonical_enum(default_retention.get("mode"), mode);
    let Some(Value::Concrete(ConcreteValue::Map(retention_period))) =
        default_retention.get("retention_period")
    else {
        panic!("default_retention should contain retention_period: {default_retention:?}");
    };
    assert_eq!(retention_period.get(period_name), Some(&int(period)));
}

#[tokio::test]
async fn rule_omitted_enables_object_lock_without_default_retention() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-no-rule";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;

    let state = create_object_lock(&provider, bucket, None).await;

    assert_enabled_without_rule(&state);
}

#[tokio::test]
async fn rule_with_days_round_trips() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-days";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;

    let state = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("GOVERNANCE", "days", 30)),
    )
    .await;

    assert_retention(&state, "GOVERNANCE", "days", 30);
}

#[tokio::test]
async fn rule_with_years_round_trips() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-years";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;

    let state = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("COMPLIANCE", "years", 1)),
    )
    .await;

    assert_retention(&state, "COMPLIANCE", "years", 1);
}

#[tokio::test]
async fn host_canonicalizes_dsl_enum_identifiers_before_create() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-dsl-enums";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;

    let rule = map([(
        "default_retention",
        map([
            (
                "mode",
                Value::Concrete(ConcreteValue::enum_identifier(
                    "aws.s3.BucketObjectLockConfiguration.ObjectLockRule.DefaultRetention.ObjectLockRetentionMode.governance",
                )),
            ),
            ("retention_period", map([("days", int(30))])),
        ]),
    )]);
    let mut resource = object_lock_resource(bucket, Some(rule));
    resource.set_attr(
        "object_lock_enabled",
        Value::Concrete(ConcreteValue::enum_identifier(
            "aws.s3.BucketObjectLockConfiguration.ObjectLockEnabled.enabled",
        )),
    );
    let registry = carina_provider_aws::schemas::schema_registry();
    carina_core::value::canonicalize_resources_with_schemas(
        std::slice::from_mut(&mut resource),
        &registry,
    )
    .expect("host should resolve authored enum identifiers");
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let _state = create_state_for_test(&provider, &id, request)
        .await
        .expect("create Object Lock configuration from DSL enum identifiers");

    let stored = client
        .get_object_lock_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("read raw Object Lock configuration");
    let configuration = stored
        .object_lock_configuration()
        .expect("stored configuration should exist");
    assert_eq!(
        configuration
            .object_lock_enabled()
            .map(|enabled| enabled.as_str()),
        Some("Enabled")
    );
    let mode = configuration
        .rule()
        .and_then(|rule| rule.default_retention())
        .and_then(|default_retention| default_retention.mode())
        .expect("stored configuration should contain a default retention mode");

    assert_eq!(mode.as_str(), "GOVERNANCE");
}

#[tokio::test]
async fn read_maps_object_lock_configuration_not_found_to_not_found() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-not-configured";
    create_bucket(&client, bucket).await;
    let id = object_lock_resource(bucket, None).id;

    let state = provider
        .read(&id, &applied_identifier(bucket), ReadRequest)
        .await
        .expect("not-configured read should succeed")
        .into_state();

    assert!(!state.exists);
}

#[tokio::test]
async fn read_maps_no_such_bucket_to_not_found() {
    let (provider, _client) = provider_with_s3().await;
    let bucket = "carina-object-lock-missing-bucket";
    let id = object_lock_resource(bucket, None).id;

    let state = provider
        .read(&id, &applied_identifier(bucket), ReadRequest)
        .await
        .expect("missing-bucket read should succeed")
        .into_state();

    assert!(!state.exists);
}

#[tokio::test]
async fn read_treats_rule_without_required_enabled_as_not_found() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-rule-only";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;
    client
        .put_object_lock_configuration()
        .bucket(bucket)
        .object_lock_configuration(
            ObjectLockConfiguration::builder()
                .rule(
                    ObjectLockRule::builder()
                        .default_retention(
                            DefaultRetention::builder()
                                .mode(ObjectLockRetentionMode::Compliance)
                                .years(5)
                                .build(),
                        )
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("store a rule-only Object Lock configuration");
    let id = object_lock_resource(bucket, None).id;

    let state = provider
        .read(&id, &applied_identifier(bucket), ReadRequest)
        .await
        .expect("rule-only read should succeed")
        .into_state();

    assert!(!state.exists);
    assert!(state.attributes.is_empty(), "{state:?}");
}

#[tokio::test]
async fn create_requires_versioning_with_actionable_bucket_error() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-versioning-required";
    create_bucket(&client, bucket).await;
    let resource = object_lock_resource(bucket, None);
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let error = create_outcome_for_test(&provider, &id, request)
        .await
        .expect_err("Object Lock create must reject a bucket without versioning");
    let message = error.provider_error().to_string();

    assert!(message.contains(bucket), "{message}");
    assert!(
        message.contains("versioning must be enabled first"),
        "{message}"
    );
}

#[tokio::test]
async fn create_rejects_all_reachable_invalid_rule_shapes() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-invalid-rules";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;

    let cases: [(&str, Value, &str); 8] = [
        ("rule not a map", string("invalid"), "rule must be a map"),
        (
            "default_retention not a map",
            map([("default_retention", string("invalid"))]),
            "default_retention must be a map",
        ),
        (
            "retention_period missing",
            map([("default_retention", map([("mode", string("GOVERNANCE"))]))]),
            "default_retention.retention_period is required",
        ),
        (
            "retention_period not a map",
            map([(
                "default_retention",
                map([
                    ("mode", string("GOVERNANCE")),
                    ("retention_period", string("invalid")),
                ]),
            )]),
            "default_retention.retention_period must be a map",
        ),
        (
            "mode missing",
            map([(
                "default_retention",
                map([("retention_period", map([("days", int(30))]))]),
            )]),
            "default_retention.mode is required",
        ),
        (
            "days not an integer",
            map([(
                "default_retention",
                map([
                    ("mode", string("GOVERNANCE")),
                    ("retention_period", map([("days", string("thirty"))])),
                ]),
            )]),
            "default_retention.retention_period.days must be an integer",
        ),
        (
            "days and years",
            map([(
                "default_retention",
                map([
                    ("mode", string("GOVERNANCE")),
                    (
                        "retention_period",
                        map([("days", int(30)), ("years", int(1))]),
                    ),
                ]),
            )]),
            "default_retention.retention_period must contain exactly one of days or years",
        ),
        (
            "neither days nor years",
            map([(
                "default_retention",
                map([
                    ("mode", string("GOVERNANCE")),
                    ("retention_period", map([])),
                ]),
            )]),
            "default_retention.retention_period must contain days or years",
        ),
    ];

    for (case, rule, expected_message) in cases {
        let resource = object_lock_resource(bucket, Some(rule));
        let id = resource.id.clone();
        let request = create_request_for_test(resource).await;
        let result = create_outcome_for_test(&provider, &id, request).await;
        let error = match result {
            Ok(_) => panic!("{case}: provider.create must reject the value"),
            Err(error) => error,
        };

        assert!(
            matches!(error.provider_error(), ProviderError::InvalidInput(_)),
            "{case}: expected InvalidInput, got {error:?}"
        );
        assert_eq!(
            error.provider_error().detail().context(),
            expected_message,
            "{case}"
        );
    }
}

#[tokio::test]
async fn destroy_succeeds_when_no_s3_service_is_registered() {
    let provider = provider_without_services().await;
    let bucket = "carina-object-lock-no-api-delete";
    let id = object_lock_resource(bucket, None).id;

    provider
        .delete(&id, &applied_identifier(bucket), delete_request())
        .await
        .expect("destroy must be a local state-only operation");
}

#[tokio::test]
async fn destroy_preserves_remote_configuration_for_later_read() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-destroy-convergence";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;
    let created = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("GOVERNANCE", "days", 30)),
    )
    .await;

    provider
        .delete(&created.id, &applied_identifier(bucket), delete_request())
        .await
        .expect("Object Lock destroy should be a no-op");
    let reread = provider
        .read(&created.id, &applied_identifier(bucket), ReadRequest)
        .await
        .expect("read after no-op destroy")
        .into_state();

    assert_retention(&reread, "GOVERNANCE", "days", 30);
}

#[tokio::test]
async fn recreate_without_rule_refuses_to_clobber_retained_compliance_rule() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-recreate-preserves-rule";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;
    let created = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("COMPLIANCE", "years", 5)),
    )
    .await;

    provider
        .delete(&created.id, &applied_identifier(bucket), delete_request())
        .await
        .expect("Object Lock destroy should be a no-op");
    let resource = object_lock_resource(bucket, None);
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;
    let recreate = create_outcome_for_test(&provider, &id, request).await;

    let stored = client
        .get_object_lock_configuration()
        .bucket(bucket)
        .send()
        .await
        .expect("read raw Object Lock configuration after refused re-create");
    let retention = stored
        .object_lock_configuration()
        .and_then(|configuration| configuration.rule())
        .and_then(|rule| rule.default_retention())
        .expect("re-create without rule must not clobber the retained COMPLIANCE rule");
    assert_eq!(
        retention.mode().map(|mode| mode.as_str()),
        Some("COMPLIANCE")
    );
    assert_eq!(retention.years(), Some(5));

    let error = recreate.expect_err("re-create without rule must be rejected");
    let message = error.provider_error().to_string();
    assert!(message.contains(bucket), "{message}");
    assert!(message.contains("already has"), "{message}");
    assert!(message.contains("Import/adopt"), "{message}");
    assert!(message.contains("declare 'rule' explicitly"), "{message}");
}

#[test]
fn served_schema_keeps_rule_updatable() {
    let schema = carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .find(|schema| schema.parts().resource_type == RESOURCE_TYPE)
        .expect("served Object Lock schema");

    assert!(
        !schema
            .effective_attribute("rule")
            .expect("rule attribute")
            .is_create_only(),
        "served rule attribute must not force replacement"
    );
}

#[tokio::test]
async fn dropping_rule_is_an_in_place_update() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-drop-rule";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;
    let created = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("GOVERNANCE", "days", 30)),
    )
    .await;
    let id = created.id.clone();
    let mut desired = resource_from_state(&created);
    desired.attributes.shift_remove("rule");
    let changed_attributes = ["rule".to_string()];
    let request = update_request_for_test(desired, created, &changed_attributes).await;

    let updated = provider
        .update(&id, &applied_identifier(bucket), request)
        .await
        .expect("dropping rule should update in place")
        .into_state_for_writeback();

    assert_enabled_without_rule(&updated);
}

#[tokio::test]
async fn retention_mode_and_period_update_in_place() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-object-lock-change-rule";
    create_bucket(&client, bucket).await;
    enable_versioning(&client, bucket).await;
    let created = create_object_lock(
        &provider,
        bucket,
        Some(retention_rule("GOVERNANCE", "days", 30)),
    )
    .await;
    let id = created.id.clone();
    let replacement_rule = retention_rule("COMPLIANCE", "years", 1);
    let mut desired = resource_from_state(&created);
    desired.set_attr("rule", replacement_rule);
    let changed_attributes = ["rule".to_string()];
    let request = update_request_for_test(desired, created, &changed_attributes).await;

    let updated = provider
        .update(&id, &applied_identifier(bucket), request)
        .await
        .expect("retention change should update in place")
        .into_state_for_writeback();

    assert_retention(&updated, "COMPLIANCE", "years", 1);
}
