//! PoC integration test: exercise `AwsProvider`'s STS data-source read
//! against an in-process AWS mock (`winterbaume`, library mode).
//!
//! This is the first foothold for winterbaume coverage in
//! `carina-provider-aws` (#355). It proves:
//!
//! - The `AwsProvider::from_sdk_config(...)` injection seam works end to end.
//! - The full production read path runs:
//!   `Provider::read_data_source` → codegen-generated
//!   `dispatch_read_data_source` → `DataSourceLookups`
//!   → `do_read_sts_caller_identity_data_source`. This is the exact
//!   chain `carina apply`/`plan` invokes for a data source.
//! - `winterbaume-sts`'s `GetCallerIdentity` handler is wire-compatible
//!   with `aws-sdk-sts` and our provider's response unwrapping.
//!
//! Follow-ups will reuse the same `from_sdk_config` seam to test the CRUD
//! surfaces of managed resources (s3 bucket, ec2 vpc, etc.).

// winterbaume is a host-process AWS emulator and does not build under
// the WASM target. The cfg-gated dev-dependencies block in `Cargo.toml`
// excludes winterbaume from WASM compiles; this attribute keeps the
// test source consistent with that gating so `cargo test --target
// wasm32-*` does not try to compile imports it cannot resolve.
#![cfg(not(target_arch = "wasm32"))]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aws_smithy_types::retry::RetryConfig;
use carina_core::effect::PlanOp;
use carina_core::provider::{Provider, ProviderReadyDataSource};
use carina_core::resource::{ConcreteValue, DataSource, ResourceId, Value};
use carina_provider_aws::{AwsNormalizer, AwsProvider, BoundedSdkConfig};
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};
use winterbaume_sts::StsService;

const TEST_REGION: &str = "us-east-1";

/// Canonical resource_type for this data source as emitted by codegen
/// (PascalCase tail). The production `dispatch_read_data_source` matches
/// on this string verbatim; using `sts.caller_identity` (snake_case)
/// would fall into the default "not implemented" arm.
const STS_CALLER_IDENTITY_TYPE: &str = "sts.CallerIdentity";

struct AlwaysUnavailableStsService {
    inner: Arc<StsService>,
    calls: AtomicUsize,
}

impl AlwaysUnavailableStsService {
    fn new(inner: Arc<StsService>) -> Self {
        Self {
            inner,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl MockService for AlwaysUnavailableStsService {
    fn service_name(&self) -> &str {
        self.inner.service_name()
    }

    fn url_patterns(&self) -> Vec<&str> {
        self.inner.url_patterns()
    }

    fn handle(
        &self,
        _request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {
            MockResponse::error(503, "ServiceUnavailable", "scripted retryable failure")
        })
    }
}

fn ready_data_source_for_test(resource: DataSource) -> ProviderReadyDataSource {
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let module_gate = carina_core::executor::ModuleConstraintGate::new(&[]);
    let mut schemas = carina_core::schema::SchemaRegistry::new();
    for schema in carina_provider_aws::schemas::all_schemas() {
        schemas.insert("aws", schema);
    }
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

#[tokio::test]
async fn required_permissions_returns_empty_vec() {
    let mock = MockAws::builder().with_service(StsService::new()).build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;

    let provider = AwsProvider::from_sdk_config(
        TEST_REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    );

    let id = ResourceId::with_provider_identity("aws", "s3.Bucket", "example", None);

    assert_eq!(
        provider
            .required_permissions(&id, PlanOp::Create)
            .expect("required permissions should be available"),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn sts_caller_identity_data_source_returns_account_arn_user_id_from_winterbaume() {
    // This test only calls STS, so only `StsService` is registered. All
    // provider clients still share the same bounded mock configuration;
    // a future fan-out to an unregistered service would surface as a loud
    // winterbaume error rather than passing silently.
    let mock = MockAws::builder().with_service(StsService::new()).build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;

    let provider = AwsProvider::from_sdk_config(
        TEST_REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    );

    let ds = ready_data_source_for_test(DataSource::with_provider(
        "aws",
        STS_CALLER_IDENTITY_TYPE,
        "me",
        None,
    ));

    // Go through `Provider::read_data_source`, the same entry point
    // `carina apply`/`plan` uses for data sources. That runs the
    // codegen-generated dispatcher (which matches on the PascalCase
    // resource_type above), the `DataSourceLookups` trait method, and
    // the `do_read_*` worker — the full chain, not just the worker.
    let state = provider
        .read_data_source(&ds)
        .await
        .expect("Provider::read_data_source must succeed");

    // All three attributes must be present as non-empty strings — proves
    // the SDK ↔ winterbaume wire round-trip and our response unwrap.
    for attr in ["account_id", "arn", "user_id"] {
        let value = state
            .attributes
            .get(attr)
            .unwrap_or_else(|| panic!("returned state missing `{attr}` attribute"));
        match value {
            Value::Concrete(ConcreteValue::String(s)) => {
                assert!(
                    !s.expose_plaintext().is_empty(),
                    "attribute `{attr}` came back as an empty string",
                );
            }
            other => panic!("attribute `{attr}` expected to be a String, got {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn bounded_config_limits_sdk_retries_to_three_attempts() {
    let service = Arc::new(AlwaysUnavailableStsService::new(
        Arc::new(StsService::new()),
    ));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock
        .sdk_config(TEST_REGION)
        .await
        .into_builder()
        .retry_config(RetryConfig::standard().with_max_attempts(10))
        .build();
    let provider = AwsProvider::from_sdk_config(
        TEST_REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(sdk_config),
    );
    let data_source = ready_data_source_for_test(DataSource::with_provider(
        "aws",
        STS_CALLER_IDENTITY_TYPE,
        "retry-bound",
        None,
    ));

    provider
        .read_data_source(&data_source)
        .await
        .expect_err("an always-503 STS service should fail");

    assert_eq!(service.calls(), 3);
}
