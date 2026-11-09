//! Carina AWS Provider
//!
//! AWS Provider implementation

#![cfg_attr(not(test), deny(clippy::cast_possible_truncation))]

mod aws_operation;
mod create_resource;
mod data_source_lookups;
mod ec2_security_group_rules;
mod ec2_tags;
pub(crate) mod error_helpers;
mod factory;
pub(crate) mod helpers;
mod normalizer;
mod provider;
pub mod provider_generated;
mod read_back;
mod read_model;
mod read_models_generated;
mod references;
#[cfg(test)]
mod resend_guard;
mod resend_safety_generated;
mod s3_delete;
pub mod schemas;
mod sdk_config;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod send_contract_test;
mod send_model;
mod send_models_generated;
pub mod services;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod test_http;
#[cfg(test)]
mod tests;

#[doc(hidden)]
pub use aws_operation::ClientToken;
pub use create_resource::AwsCreateResource;
pub use factory::AwsProviderFactory;
#[doc(hidden)]
pub use helpers::UpdateResource;
pub use normalizer::AwsNormalizer;
#[doc(hidden)]
pub use provider::{
    AwsCreateAttempt, AwsCreatePoll, AwsCreateResume, AwsFailureSummary, AwsIssuedCreate,
    AwsPollAttempt, AwsResumeAttempt, AwsResumedCreate,
};
pub use provider::{
    AwsCreateOperation, AwsCreateToken, AwsDeleteToken, AwsUpdateToken, Ec2TaggedResourceKind,
};
pub use read_back::{NonEmptyAttributeNames, ReadBackContinuation, ReadBackRequirements};
#[doc(hidden)]
#[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
pub use read_model::out_of_range_duration_applied_state_for_protocol_test;
#[doc(hidden)]
#[cfg(debug_assertions)]
pub use read_model::test_existing_state;
pub use sdk_config::BoundedSdkConfig;

#[cfg(test)]
#[path = "../tests/support/comparison.rs"]
mod comparison_tests;
#[cfg(test)]
pub(crate) use comparison_tests::{test_comparison_router, test_diff};

#[cfg(test)]
pub(crate) fn test_rootless_type(
    attr: carina_core::schema::AttributeType,
) -> carina_core::schema::RootlessType {
    const NAME: &str = "test_value";
    carina_core::schema::ValidatedConfigSchema::validate(std::collections::BTreeMap::from([(
        NAME.to_string(),
        attr,
    )]))
    .expect("test attribute type must pass config-schema admission")
    .rootless_type(NAME)
    .expect("test attribute type must be present")
}

use carina_core::schema::{CompiledResourceSchema, SchemaKind, SchemaRegistry};

#[cfg(test)]
pub(crate) fn test_resource_schema(resource_type: &str) -> &'static CompiledResourceSchema {
    static REGISTRY: std::sync::OnceLock<SchemaRegistry> = std::sync::OnceLock::new();
    REGISTRY
        .get_or_init(schemas::schema_registry)
        .get("aws", resource_type, SchemaKind::Resource)
        .expect("test resource schema must be registered")
        .schema()
        .as_schema()
}

use resend_safety_generated::{
    AccountClient, AcmClient, CloudWatchLogsClient, Ec2Client, IamClient, IdentityStoreClient,
    OrganizationsClient, Route53Client, S3Client, SqsClient, StsClient,
};

/// AWS Provider
///
/// Service SDKs are available only through `carina_aws_types::sdk`.
/// Provider code cannot use the SDK's raw client constructors:
///
/// ```compile_fail
/// use aws_sdk_s3::Client;
/// ```
///
/// The same import works through the guarded facade:
///
/// ```
/// use carina_aws_types::sdk::s3::S3Client;
/// ```
pub struct AwsProvider {
    diagnostics: carina_plugin_sdk::ProviderDiagnostics,
    s3_client: S3Client,
    ec2_client: Ec2Client,
    iam_client: IamClient,
    logs_client: CloudWatchLogsClient,
    sts_client: StsClient,
    account_client: AccountClient,
    organizations_client: OrganizationsClient,
    identitystore_client: IdentityStoreClient,
    route53_client: Route53Client,
    acm_client: AcmClient,
    sqs_client: SqsClient,
    kms_client: references::KmsClient,
    schema_registry: SchemaRegistry,
    region: String,
    /// Provider-level allow-list of AWS account IDs. Empty means "no
    /// allow-list configured" (the check is skipped). Enforced once
    /// during initialization via [`AwsProvider::verify_account_id`].
    pub(crate) allowed_account_ids: Vec<String>,
    /// Provider-level deny-list of AWS account IDs. Empty means "no
    /// deny-list configured". Enforced once during initialization via
    /// [`AwsProvider::verify_account_id`].
    pub(crate) forbidden_account_ids: Vec<String>,
}

impl AwsProvider {
    /// Create a new AWS Provider
    pub async fn new(region: &str, diagnostics: carina_plugin_sdk::ProviderDiagnostics) -> Self {
        Self::new_with_account_guard(region, Vec::new(), Vec::new(), None, diagnostics).await
    }

    /// Create a new AWS Provider with provider-level account guard
    /// configuration. `allowed_account_ids` and `forbidden_account_ids`
    /// are stored verbatim; the guard itself is only run when
    /// [`AwsProvider::verify_account_id`] is called.
    ///
    /// `assume_role`, when present, layers an
    /// [`carina_aws_types::GuardedAssumeRoleProvider`] over the base credentials
    /// (host-supplied on wasm32, the default chain natively) so all SDK
    /// calls go out under the assumed-role session.
    pub async fn new_with_account_guard(
        region: &str,
        allowed_account_ids: Vec<String>,
        forbidden_account_ids: Vec<String>,
        assume_role: Option<crate::services::sts::assume_role::AssumeRoleConfig>,
        diagnostics: carina_plugin_sdk::ProviderDiagnostics,
    ) -> Self {
        let config = BoundedSdkConfig::load(region, assume_role, diagnostics).await;

        Self::from_sdk_config(
            region.to_string(),
            allowed_account_ids,
            forbidden_account_ids,
            &config,
        )
    }

    fn schema_registry() -> SchemaRegistry {
        crate::schemas::schema_registry()
    }

    pub(crate) fn resource_schema(&self, resource_type: &str) -> Option<&CompiledResourceSchema> {
        self.schema_registry
            .get("aws", resource_type, SchemaKind::Resource)
            .map(|schema| schema.schema().as_schema())
    }
}
