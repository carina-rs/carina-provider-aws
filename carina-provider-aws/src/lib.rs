//! Carina AWS Provider
//!
//! AWS Provider implementation

mod data_source_lookups;
mod ec2_security_group_rules;
mod ec2_tags;
mod factory;
pub(crate) mod helpers;
mod normalizer;
mod provider;
pub mod provider_generated;
pub mod schemas;
pub mod services;
#[cfg(test)]
mod tests;

pub use factory::AwsProviderFactory;
pub use normalizer::AwsNormalizer;

use aws_config::Region;
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
        let config = Self::build_config(region, assume_role).await;

        Self::from_clients(
            region.to_string(),
            allowed_account_ids,
            forbidden_account_ids,
            S3Client::new(&config),
            Ec2Client::new(&config),
            IamClient::new(&config),
            CloudWatchLogsClient::new(&config),
            StsClient::new(&config),
            OrganizationsClient::new(&config),
            IdentityStoreClient::new(&config),
            Route53Client::new(&config),
            AcmClient::new(&config),
            SqsClient::new(&config),
        )
    }

    /// Construct an `AwsProvider` from caller-supplied SDK clients.
    ///
    /// `new_with_account_guard` is the production entry point — it
    /// resolves the region, optionally layers an `AssumeRoleProvider`
    /// onto the credential chain, builds an `SdkConfig`, and constructs
    /// all 10 clients itself. This constructor exists so integration
    /// tests can inject clients wired to an in-process AWS mock
    /// (`winterbaume`, library mode) and exercise the real SDK I/O
    /// path with no real AWS and no external process (#355).
    ///
    /// All 10 clients are required arguments by design: when an 11th
    /// SDK is added in the future every caller — production and tests
    /// alike — is forced by the compiler to provide it. An
    /// `Option<Client>`-shape would silently let tests skip newly-added
    /// clients; a `from_sdk_config`-shape would block the eventual
    /// "swap one client for a mock, keep the rest on real AWS"-style
    /// finer-grained injection. The 10-arg boilerplate is the deliberate
    /// trade for the type-level guarantee.
    #[allow(clippy::too_many_arguments)]
    pub fn from_clients(
        region: String,
        allowed_account_ids: Vec<String>,
        forbidden_account_ids: Vec<String>,
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
    ) -> Self {
        Self {
            s3_client,
            ec2_client,
            iam_client,
            logs_client,
            sts_client,
            organizations_client,
            identitystore_client,
            route53_client,
            acm_client,
            sqs_client,
            region,
            allowed_account_ids,
            forbidden_account_ids,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn build_config(
        region: &str,
        assume_role: Option<crate::services::sts::assume_role::AssumeRoleConfig>,
    ) -> aws_config::SdkConfig {
        let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .load()
            .await;
        match assume_role {
            None => base,
            Some(ar) => Self::wrap_with_assume_role(base, ar).await,
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn build_config(
        region: &str,
        assume_role: Option<crate::services::sts::assume_role::AssumeRoleConfig>,
    ) -> aws_config::SdkConfig {
        use carina_plugin_sdk::wasi_http::WasiHttpClient;
        let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .http_client(WasiHttpClient::new())
            .load()
            .await;
        match assume_role {
            None => base,
            Some(ar) => Self::wrap_with_assume_role(base, ar).await,
        }
    }

    /// Layer an STS `AssumeRoleProvider` on top of the ambient base
    /// credential chain. The returned [`aws_config::SdkConfig`] uses the
    /// assumed-role credentials for every SDK call thereafter.
    async fn wrap_with_assume_role(
        base: aws_config::SdkConfig,
        ar: crate::services::sts::assume_role::AssumeRoleConfig,
    ) -> aws_config::SdkConfig {
        let mut builder =
            aws_config::sts::AssumeRoleProvider::builder(ar.role_arn.clone()).configure(&base);
        if let Some(name) = ar.session_name.as_deref() {
            builder = builder.session_name(name);
        }
        if let Some(eid) = ar.external_id.as_deref() {
            builder = builder.external_id(eid);
        }
        if let Some(dur) = ar.duration {
            builder = builder.session_length(dur);
        }
        let provider = builder.build().await;
        let shared = aws_credential_types::provider::SharedCredentialsProvider::new(provider);
        base.into_builder().credentials_provider(shared).build()
    }
}
