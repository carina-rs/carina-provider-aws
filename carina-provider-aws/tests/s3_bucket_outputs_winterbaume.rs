//! Provider-level coverage for managed S3 bucket outputs against the
//! in-process winterbaume S3 service. No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use aws_sdk_s3::Client as S3Client;
use carina_core::differ::{Diff, diff};
use carina_core::provider::RawSavedAttrs;
use carina_core::provider::{
    AppliedIdentifier, CreateOutcomeKind, CreateRequest, NoopNormalizer, Provider,
    ProviderReadyDataSource, ReadRequest, UpdateOutcome, UpdateRequest,
};
use carina_core::resource::{ConcreteValue, DataSource, ResolvedResource, Resource, State, Value};
use carina_core::schema::SchemaKind;
use carina_provider_aws::{AwsNormalizer, AwsProvider, BoundedSdkConfig, test_existing_state};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use winterbaume_core::{
    MockAws, MockRequest, MockResponse, MockService, StatefulService, default_account_id,
};
use winterbaume_s3::views::BucketStateView;
use winterbaume_s3::{S3Service, S3StateView};

const PROVIDER_REGION: &str = "us-east-1";
const BUCKET_REGION: &str = "ap-northeast-1";
const RESOURCE_TYPE: &str = "s3.Bucket";

struct RecordingS3Service {
    inner: Arc<S3Service>,
    get_bucket_location_calls: AtomicUsize,
}

struct NotFoundHeadS3Service {
    inner: Arc<S3Service>,
}

struct ErrorHeadS3Service {
    inner: Arc<S3Service>,
}

struct NamespaceRecordingS3Service {
    inner: Arc<S3Service>,
    namespaces: Mutex<Vec<String>>,
}

impl MockService for NamespaceRecordingS3Service {
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
        if request.method == "PUT"
            && let Some(namespace) = request
                .headers
                .get("x-amz-bucket-namespace")
                .and_then(|value| value.to_str().ok())
        {
            self.namespaces
                .lock()
                .expect("namespace recording lock")
                .push(namespace.to_string());
        }
        self.inner.handle(request)
    }
}

impl MockService for NotFoundHeadS3Service {
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
        if request.method == "HEAD" {
            let response = MockResponse::error(404, "NoSuchBucket", "bucket not found");
            return Box::pin(async move { response });
        }
        self.inner.handle(request)
    }
}

impl MockService for ErrorHeadS3Service {
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
        if request.method == "HEAD" {
            let response = MockResponse::error(400, "InvalidRequest", "scripted read-back failure");
            return Box::pin(async move { response });
        }
        self.inner.handle(request)
    }
}

impl RecordingS3Service {
    fn new(inner: Arc<S3Service>) -> Self {
        Self {
            inner,
            get_bucket_location_calls: AtomicUsize::new(0),
        }
    }

    fn get_bucket_location_calls(&self) -> usize {
        self.get_bucket_location_calls.load(Ordering::Relaxed)
    }
}

impl MockService for RecordingS3Service {
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
        let is_get_bucket_location = request.method == "GET"
            && request.uri.split_once('?').is_some_and(|(_, query)| {
                query
                    .split('&')
                    .any(|parameter| parameter.split('=').next() == Some("location"))
            });
        if is_get_bucket_location {
            self.get_bucket_location_calls
                .fetch_add(1, Ordering::Relaxed);
        }
        self.inner.handle(request)
    }
}

fn provider_from_config(sdk_config: aws_types::SdkConfig) -> AwsProvider {
    AwsProvider::from_sdk_config(
        PROVIDER_REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    )
}

// This test-only raw client stages and inspects winterbaume state outside the
// provider; provider operations still use the generated guarded wrapper.
#[allow(clippy::disallowed_methods)]
async fn provider_with_s3() -> (AwsProvider, S3Client) {
    let mock = MockAws::builder().with_service(S3Service::new()).build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    let s3_client = S3Client::new(&sdk_config);
    (provider_from_config(sdk_config), s3_client)
}

async fn provider_with_namespace_recording_s3() -> (AwsProvider, Arc<NamespaceRecordingS3Service>) {
    let service = Arc::new(NamespaceRecordingS3Service {
        inner: Arc::new(S3Service::new()),
        namespaces: Mutex::new(Vec::new()),
    });
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    (provider_from_config(sdk_config), service)
}

async fn provider_with_staged_bucket(bucket: &str) -> (AwsProvider, Arc<RecordingS3Service>) {
    let s3_service = Arc::new(S3Service::new());

    // Winterbaume normally derives a created bucket's stored region from the
    // signed request. Its public restore seam lets this test place a bucket with
    // a distinct region in the provider/signing region's request scope.
    s3_service
        .restore(
            default_account_id(),
            PROVIDER_REGION,
            S3StateView {
                buckets: HashMap::from([(
                    bucket.to_string(),
                    BucketStateView {
                        name: bucket.to_string(),
                        region: BUCKET_REGION.to_string(),
                        ..Default::default()
                    },
                )]),
            },
        )
        .await
        .expect("stage S3 bucket with a distinct region");

    let recording_service = Arc::new(RecordingS3Service::new(s3_service));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&recording_service))
        .build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    (provider_from_config(sdk_config), recording_service)
}

async fn provider_with_not_found_read_back(staged_bucket: Option<&str>) -> AwsProvider {
    let s3_service = Arc::new(S3Service::new());
    if let Some(bucket) = staged_bucket {
        s3_service
            .restore(
                default_account_id(),
                PROVIDER_REGION,
                S3StateView {
                    buckets: HashMap::from([(
                        bucket.to_string(),
                        BucketStateView {
                            name: bucket.to_string(),
                            region: PROVIDER_REGION.to_string(),
                            ..Default::default()
                        },
                    )]),
                },
            )
            .await
            .expect("stage S3 bucket for update");
    }

    let mock = MockAws::builder()
        .with_service(Arc::new(NotFoundHeadS3Service { inner: s3_service }))
        .build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    provider_from_config(sdk_config)
}

async fn provider_with_error_read_back(staged_bucket: Option<&str>) -> AwsProvider {
    let s3_service = Arc::new(S3Service::new());
    if let Some(bucket) = staged_bucket {
        s3_service
            .restore(
                default_account_id(),
                PROVIDER_REGION,
                S3StateView {
                    buckets: HashMap::from([(
                        bucket.to_string(),
                        BucketStateView {
                            name: bucket.to_string(),
                            region: PROVIDER_REGION.to_string(),
                            ..Default::default()
                        },
                    )]),
                },
            )
            .await
            .expect("stage S3 bucket for update");
    }

    let mock = MockAws::builder()
        .with_service(Arc::new(ErrorHeadS3Service { inner: s3_service }))
        .build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    provider_from_config(sdk_config)
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn schema_registry() -> carina_core::schema::SchemaRegistry {
    let mut schemas = carina_core::schema::SchemaRegistry::new();
    for schema in carina_provider_aws::schemas::all_schemas() {
        schemas.insert("aws", schema);
    }
    schemas
}

fn assert_no_diff(desired: Resource, current: State) {
    let registry = schema_registry();
    let schema = registry
        .get("aws", RESOURCE_TYPE, SchemaKind::Resource)
        .expect("S3 bucket schema");
    let saved = RawSavedAttrs::default().lift(&registry);
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
        .expect("test resource should become a provider-ready create request")
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

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
}

fn ready_data_source_for_test(resource: DataSource) -> ProviderReadyDataSource {
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
    carina_core::executor::prepare_provider_ready_data_source(resource, &preparation)
        .expect("test data source should pass checked provider preparation")
}

fn assert_bucket_outputs(state: &State, bucket: &str, region: &str, hosted_zone_id: &str) {
    assert!(state.exists);
    assert_eq!(state.attributes.get("bucket"), Some(&string(bucket)));
    assert_eq!(
        state.attributes.get("arn"),
        Some(&string(&format!("arn:aws:s3:::{bucket}")))
    );
    assert_eq!(state.attributes.get("region"), Some(&string(region)));
    assert_eq!(
        state.attributes.get("bucket_domain_name"),
        Some(&string(&format!("{bucket}.s3.amazonaws.com")))
    );
    assert_eq!(
        state.attributes.get("bucket_regional_domain_name"),
        Some(&string(&format!("{bucket}.s3.{region}.amazonaws.com")))
    );
    assert_eq!(
        state.attributes.get("hosted_zone_id"),
        Some(&string(hosted_zone_id))
    );
}

#[tokio::test]
async fn create_bucket_ignores_supplied_read_only_region_for_placement() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-bucket-outputs";
    let supplied_read_only_region = BUCKET_REGION;
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "outputs", None);
    resource.set_attr("bucket", string(bucket));
    // The provider region controls placement. Supplying this read-only value pins
    // the regression where create previously treated it as a placement override.
    resource.set_attr("region", string(supplied_read_only_region));
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let state = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create S3 bucket"),
    )
    .0;

    let raw_location = client
        .get_bucket_location()
        .bucket(bucket)
        .send()
        .await
        .expect("read raw bucket location");
    assert_eq!(
        raw_location
            .location_constraint()
            .map(|constraint| constraint.as_str())
            .unwrap_or_default(),
        "",
        "winterbaume should reproduce S3's absent/empty us-east-1 location constraint"
    );

    assert_bucket_outputs(&state, bucket, PROVIDER_REGION, "Z3AQBSTGFYJSTF");
    assert_ne!(
        state.attributes.get("region"),
        Some(&string(supplied_read_only_region))
    );
}

#[tokio::test]
async fn bucket_namespace_is_sent_and_import_style_read_has_no_diff() {
    let (provider, service) = provider_with_namespace_recording_s3().await;
    let bucket = "carina-bucket-namespace";
    let mut desired = Resource::with_provider("aws", RESOURCE_TYPE, "namespace", None);
    desired.set_attr("bucket", string(bucket));
    desired.set_attr("bucket_namespace", string("global"));
    desired.set_attr("tags", Value::Concrete(ConcreteValue::Map(IndexMap::new())));
    let id = desired.id.clone();
    let request = create_request_without_schema_for_test(desired.clone()).await;

    let current = carina_core::executor::complete_create_for_writeback(
        carina_core::executor::dispatch_create(&provider, &id, request)
            .await
            .expect("create namespaced S3 bucket"),
    )
    .0;

    assert_eq!(
        service
            .namespaces
            .lock()
            .expect("namespace recording lock")
            .as_slice(),
        ["global"]
    );
    assert!(
        !current.attributes.contains_key("bucket_namespace"),
        "HeadBucket cannot return the create-time namespace selector"
    );
    assert_no_diff(desired, current);
}

#[tokio::test]
async fn create_bucket_read_back_not_found_is_partial_success_with_created_identifier() {
    let provider = provider_with_not_found_read_back(None).await;
    let bucket = "carina-bucket-create-read-back-not-found";
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "partial-create", None);
    resource.set_attr("bucket", string(bucket));
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let outcome = carina_core::executor::dispatch_create(&provider, &id, request)
        .await
        .expect("the create API succeeded, so a missing read-back must stay successful");

    let CreateOutcomeKind::PartialSuccess { state, diagnostic } = outcome.into_kind() else {
        panic!("expected partial create success");
    };
    assert_eq!(state.identifier().as_str(), bucket);
    assert!(diagnostic.reason().contains("post-create read-back"));
    assert!(!diagnostic.missing_attributes().is_empty());
}

#[tokio::test]
async fn update_bucket_read_back_not_found_is_partial_success_with_updated_identifier() {
    let bucket = "carina-bucket-update-read-back-not-found";
    let provider = provider_with_not_found_read_back(Some(bucket)).await;
    let id = Resource::with_provider("aws", RESOURCE_TYPE, "partial-update", None).id;
    let from = test_existing_state(
        id.clone(),
        HashMap::from([("bucket".to_string(), string(bucket))]),
    )
    .with_identifier(bucket);
    let mut desired = Resource::from_id(id.clone());
    desired.set_attr("bucket", string(bucket));
    desired.set_attr(
        "tags",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "purpose".to_string(),
            string("partial-update-test"),
        )]))),
    );
    let request = update_request_for_test(desired, from, &["tags".to_string()]).await;

    let outcome = provider
        .update(&id, &applied_identifier(bucket), request)
        .await
        .expect("the update API succeeded, so a missing read-back must stay successful");

    let UpdateOutcome::PartialSuccess { state, diagnostic } = outcome else {
        panic!("expected partial update success, got {outcome:?}");
    };
    assert_eq!(state.identifier().as_str(), bucket);
    assert!(diagnostic.reason().contains("post-update read-back"));
    assert!(!diagnostic.missing_attributes().is_empty());
}

#[tokio::test]
async fn create_bucket_read_back_error_is_partial_success_with_created_identifier() {
    let provider = provider_with_error_read_back(None).await;
    let bucket = "carina-bucket-create-read-back-error";
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "partial-create-error", None);
    resource.set_attr("bucket", string(bucket));
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let outcome = carina_core::executor::dispatch_create(&provider, &id, request)
        .await
        .expect("the create API succeeded, so a failed read-back must stay successful");

    let CreateOutcomeKind::PartialSuccess { state, diagnostic } = outcome.into_kind() else {
        panic!("expected partial create success");
    };
    assert_eq!(state.identifier().as_str(), bucket);
    assert!(diagnostic.reason().contains("post-create read-back"));
    assert!(diagnostic.reason().contains("Failed to read bucket"));
    assert!(!diagnostic.missing_attributes().is_empty());
}

#[tokio::test]
async fn update_bucket_read_back_error_is_partial_success_with_updated_identifier() {
    let bucket = "carina-bucket-update-read-back-error";
    let provider = provider_with_error_read_back(Some(bucket)).await;
    let id = Resource::with_provider("aws", RESOURCE_TYPE, "partial-update-error", None).id;
    let from = test_existing_state(
        id.clone(),
        HashMap::from([("bucket".to_string(), string(bucket))]),
    )
    .with_identifier(bucket);
    let mut desired = Resource::from_id(id.clone());
    desired.set_attr("bucket", string(bucket));
    desired.set_attr(
        "tags",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "purpose".to_string(),
            string("partial-update-error-test"),
        )]))),
    );
    let request = update_request_for_test(desired, from, &["tags".to_string()]).await;

    let outcome = provider
        .update(&id, &applied_identifier(bucket), request)
        .await
        .expect("the update API succeeded, so a failed read-back must stay successful");

    let UpdateOutcome::PartialSuccess { state, diagnostic } = outcome else {
        panic!("expected partial update success, got {outcome:?}");
    };
    assert_eq!(state.identifier().as_str(), bucket);
    assert!(diagnostic.reason().contains("post-update read-back"));
    assert!(diagnostic.reason().contains("Failed to read bucket"));
    assert!(!diagnostic.missing_attributes().is_empty());
}

#[tokio::test]
async fn managed_and_data_source_reads_use_head_bucket_region_without_get_bucket_location() {
    assert_ne!(
        PROVIDER_REGION, BUCKET_REGION,
        "the fixture must distinguish HeadBucket's region from the provider region"
    );

    let bucket = "carina-bucket-head-region";
    let (provider, service) = provider_with_staged_bucket(bucket).await;

    let managed_id = Resource::with_provider("aws", RESOURCE_TYPE, "managed", None).id;
    let managed_state = provider
        .read(&managed_id, &applied_identifier(bucket), ReadRequest)
        .await
        .expect("read managed S3 bucket")
        .into_state();
    assert_bucket_outputs(&managed_state, bucket, BUCKET_REGION, "Z2M4EHUR26P7ZW");

    let mut data_source = DataSource::with_provider("aws", RESOURCE_TYPE, "lookup", None);
    data_source.set_attr("bucket", string(bucket));
    let data_source = ready_data_source_for_test(data_source);
    let data_source_state = provider
        .read_data_source(&data_source)
        .await
        .expect("read S3 bucket data source");
    assert_bucket_outputs(&data_source_state, bucket, BUCKET_REGION, "Z2M4EHUR26P7ZW");
    assert_eq!(
        data_source_state.attributes.get("bucket"),
        Some(&string(bucket)),
        "the data source exposes its lookup input"
    );

    assert_eq!(
        service.get_bucket_location_calls(),
        0,
        "both read paths must use HeadBucket's x-amz-bucket-region"
    );
}
