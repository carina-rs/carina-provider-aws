//! Carina AWS Provider
//!
//! AWS Provider implementation

mod aws_operation;
mod data_source_lookups;
mod ec2_security_group_rules;
mod ec2_tags;
pub(crate) mod error_helpers;
mod factory;
pub(crate) mod helpers;
mod normalizer;
mod provider;
pub mod provider_generated;
#[cfg(test)]
mod resend_guard;
mod resend_safety_generated;
pub mod schemas;
mod sdk_config;
pub mod services;
#[cfg(test)]
mod tests;

#[doc(hidden)]
pub use aws_operation::ClientToken;
pub use factory::AwsProviderFactory;
pub use normalizer::AwsNormalizer;
#[doc(hidden)]
pub use provider::{
    AwsCreateAttempt, AwsCreatePoll, AwsCreateResume, AwsFailureSummary, AwsIssuedCreate,
    AwsPollAttempt, AwsResumeAttempt, AwsResumedCreate,
};
pub use provider::{AwsCreateToken, AwsDeleteToken, Ec2TaggedResourceKind};
pub use sdk_config::BoundedSdkConfig;

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

use carina_core::schema::{ResourceSchema, SchemaKind, SchemaRegistry};
use resend_safety_generated::{
    AcmClient, CloudWatchLogsClient, Ec2Client, IamClient, IdentityStoreClient,
    OrganizationsClient, Route53Client, S3Client, SqsClient, StsClient,
};

/// AWS Provider
pub struct AwsProvider {
    s3_client: S3Client,
    ec2_client: Ec2Client,
    iam_client: IamClient,
    logs_client: CloudWatchLogsClient,
    sts_client: StsClient,
    organizations_client: OrganizationsClient,
    identitystore_client: IdentityStoreClient,
    route53_client: Route53Client,
    acm_client: AcmClient,
    sqs_client: SqsClient,
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
    pub async fn new(region: &str) -> Self {
        Self::new_with_account_guard(region, Vec::new(), Vec::new(), None).await
    }

    /// Create a new AWS Provider with provider-level account guard
    /// configuration. `allowed_account_ids` and `forbidden_account_ids`
    /// are stored verbatim; the guard itself is only run when
    /// [`AwsProvider::verify_account_id`] is called.
    ///
    /// `assume_role`, when present, layers an
    /// [`aws_config::sts::AssumeRoleProvider`] on top of the ambient
    /// credential chain so all SDK calls go out under the assumed-role
    /// session.
    pub async fn new_with_account_guard(
        region: &str,
        allowed_account_ids: Vec<String>,
        forbidden_account_ids: Vec<String>,
        assume_role: Option<crate::services::sts::assume_role::AssumeRoleConfig>,
    ) -> Self {
        let config = BoundedSdkConfig::load(region, assume_role).await;

        Self::from_sdk_config(
            region.to_string(),
            allowed_account_ids,
            forbidden_account_ids,
            &config,
        )
    }

    fn schema_registry() -> SchemaRegistry {
        let mut registry = SchemaRegistry::new();
        for schema in crate::schemas::all_schemas() {
            registry.insert("aws", schema);
        }
        registry
    }

    pub(crate) fn resource_schema(&self, resource_type: &str) -> Option<&ResourceSchema> {
        self.schema_registry
            .get("aws", resource_type, SchemaKind::Resource)
            .map(|schema| schema.schema().as_schema())
    }
}
