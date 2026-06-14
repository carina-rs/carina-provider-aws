//! PoC integration test: exercise `AwsProvider`'s STS data-source read
//! against an in-process AWS mock (`winterbaume`, library mode).
//!
//! This is the first foothold for winterbaume coverage in
//! `carina-provider-aws` (#355). It proves:
//!
//! - The `AwsProvider::from_clients(...)` injection seam works end to end.
//! - The full production read path runs:
//!   `Provider::read_data_source` → codegen-generated
//!   `dispatch_read_data_source` → `DataSourceLookups`
//!   → `do_read_sts_caller_identity_data_source`. This is the exact
//!   chain `carina apply`/`plan` invokes for a data source.
//! - `winterbaume-sts`'s `GetCallerIdentity` handler is wire-compatible
//!   with `aws-sdk-sts` and our provider's response unwrapping.
//!
//! Follow-ups will reuse the same `from_clients` seam to test the CRUD
//! surfaces of managed resources (s3 bucket, ec2 vpc, etc.).

// winterbaume is a host-process AWS emulator and does not build under
// the WASM target. The cfg-gated dev-dependencies block in `Cargo.toml`
// excludes winterbaume from WASM compiles; this attribute keeps the
// test source consistent with that gating so `cargo test --target
// wasm32-*` does not try to compile imports it cannot resolve.
#![cfg(not(target_arch = "wasm32"))]

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

use carina_core::effect::PlanOp;
use carina_core::provider::Provider;
use carina_core::resource::{ConcreteValue, DataSource, ResourceId, Value};
use carina_provider_aws::AwsProvider;
use winterbaume_core::MockAws;
use winterbaume_sts::StsService;

const TEST_REGION: &str = "us-east-1";

/// Canonical resource_type for this data source as emitted by codegen
/// (PascalCase tail). The production `dispatch_read_data_source` matches
/// on this string verbatim; using `sts.caller_identity` (snake_case)
/// would fall into the default "not implemented" arm.
const STS_CALLER_IDENTITY_TYPE: &str = "sts.CallerIdentity";

#[tokio::test]
async fn required_permissions_returns_empty_vec() {
    let mock = MockAws::builder().with_service(StsService::new()).build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;

    let provider = AwsProvider::from_clients(
        TEST_REGION.to_string(),
        Vec::new(),
        Vec::new(),
        S3Client::new(&sdk_config),
        Ec2Client::new(&sdk_config),
        IamClient::new(&sdk_config),
        CloudWatchLogsClient::new(&sdk_config),
        StsClient::new(&sdk_config),
        OrganizationsClient::new(&sdk_config),
        IdentityStoreClient::new(&sdk_config),
        Route53Client::new(&sdk_config),
        AcmClient::new(&sdk_config),
        SqsClient::new(&sdk_config),
    );

    let id = ResourceId::with_provider("aws", "s3.Bucket", "example", None);

    assert_eq!(
        provider.required_permissions(&id, PlanOp::Create),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn sts_caller_identity_data_source_returns_account_arn_user_id_from_winterbaume() {
    // `from_clients` requires all 10 SDK clients (see its doc-comment in
    // `lib.rs` for the type-safety rationale). This test only calls STS,
    // so we register `StsService` in the mock and wire the other 9
    // clients to the same `sdk_config`. They are never invoked here; a
    // future fan-out to an unregistered service would surface as a
    // loud winterbaume error rather than passing silently.
    let mock = MockAws::builder().with_service(StsService::new()).build();
    let sdk_config = mock.sdk_config(TEST_REGION).await;

    let provider = AwsProvider::from_clients(
        TEST_REGION.to_string(),
        Vec::new(),
        Vec::new(),
        S3Client::new(&sdk_config),
        Ec2Client::new(&sdk_config),
        IamClient::new(&sdk_config),
        CloudWatchLogsClient::new(&sdk_config),
        StsClient::new(&sdk_config),
        OrganizationsClient::new(&sdk_config),
        IdentityStoreClient::new(&sdk_config),
        Route53Client::new(&sdk_config),
        AcmClient::new(&sdk_config),
        SqsClient::new(&sdk_config),
    );

    let ds = DataSource::with_provider("aws", STS_CALLER_IDENTITY_TYPE, "me", None);

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
                    !s.is_empty(),
                    "attribute `{attr}` came back as an empty string",
                );
            }
            other => panic!("attribute `{attr}` expected to be a String, got {other:?}"),
        }
    }
}
