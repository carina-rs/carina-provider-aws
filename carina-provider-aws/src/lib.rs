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
        Self::new_with_account_guard(region, Vec::new(), Vec::new()).await
    }

    /// Create a new AWS Provider with provider-level account guard
    /// configuration. `allowed_account_ids` and `forbidden_account_ids`
    /// are stored verbatim; the guard itself is only run when
    /// [`AwsProvider::verify_account_id`] is called.
    pub async fn new_with_account_guard(
        region: &str,
        allowed_account_ids: Vec<String>,
        forbidden_account_ids: Vec<String>,
    ) -> Self {
        let config = Self::build_config(region).await;

        Self {
            s3_client: S3Client::new(&config),
            ec2_client: Ec2Client::new(&config),
            iam_client: IamClient::new(&config),
            logs_client: CloudWatchLogsClient::new(&config),
            sts_client: StsClient::new(&config),
            organizations_client: OrganizationsClient::new(&config),
            identitystore_client: IdentityStoreClient::new(&config),
            route53_client: Route53Client::new(&config),
            acm_client: AcmClient::new(&config),
            sqs_client: SqsClient::new(&config),
            region: region.to_string(),
            allowed_account_ids,
            forbidden_account_ids,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn build_config(region: &str) -> aws_config::SdkConfig {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .load()
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn build_config(region: &str) -> aws_config::SdkConfig {
        use carina_plugin_sdk::wasi_http::WasiHttpClient;
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .http_client(WasiHttpClient::new())
            .load()
            .await
    }

    /// Create with specific clients (for testing)
    #[allow(clippy::too_many_arguments)]
    pub fn with_clients(
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
            allowed_account_ids: Vec::new(),
            forbidden_account_ids: Vec::new(),
        }
    }
}
