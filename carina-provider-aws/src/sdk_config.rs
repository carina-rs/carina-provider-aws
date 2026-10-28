//! Builds AWS SDK configurations with provider-wide timeout and retry bounds.

use std::time::Duration;

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
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;

use crate::AwsProvider;
use crate::services::sts::assume_role::AssumeRoleConfig;

/// An AWS SDK configuration with the provider's timeout and retry policy applied.
pub struct BoundedSdkConfig(aws_config::SdkConfig);

impl BoundedSdkConfig {
    /// Apply the provider's timeout and retry policy to an SDK configuration.
    pub fn new(config: aws_config::SdkConfig) -> Self {
        Self(
            config
                .into_builder()
                .timeout_config(timeout_config())
                .retry_config(retry_config())
                .build(),
        )
    }

    /// Load a bounded SDK configuration for the region and optional assumed role.
    pub(crate) async fn load(region: &str, assume_role: Option<AssumeRoleConfig>) -> Self {
        let loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()));
        #[cfg(target_arch = "wasm32")]
        let loader = loader.http_client(carina_plugin_sdk::wasi_http::WasiHttpClient::new());
        let base = Self::new(loader.load().await);

        match assume_role {
            None => base,
            Some(assume_role) => base.with_assume_role(assume_role).await,
        }
    }

    async fn with_assume_role(self, assume_role: AssumeRoleConfig) -> Self {
        let mut builder =
            aws_config::sts::AssumeRoleProvider::builder(assume_role.role_arn.clone())
                .configure(&self.0);
        if let Some(name) = assume_role.session_name.as_deref() {
            builder = builder.session_name(name);
        }
        if let Some(external_id) = assume_role.external_id.as_deref() {
            builder = builder.external_id(external_id);
        }
        if let Some(duration) = assume_role.duration {
            builder = builder.session_length(duration);
        }

        let provider = builder.build().await;
        let shared = aws_credential_types::provider::SharedCredentialsProvider::new(provider);
        Self::new(self.0.into_builder().credentials_provider(shared).build())
    }
}

/// Sets read and per-attempt timeouts to the host's 30-second wasi:http per-request
/// cap and a 60-second ceiling for one SDK call (all attempts and backoff). In WASM
/// the HTTP client sends synchronously, so the attempt timeout is enforced through
/// connector read/connect timeouts passed by the SDK to wasi:http; the operation
/// timeout fires between attempts.
fn timeout_config() -> TimeoutConfig {
    TimeoutConfig::builder()
        .connect_timeout(Duration::from_millis(3_100))
        .read_timeout(Duration::from_secs(30))
        .operation_attempt_timeout(Duration::from_secs(30))
        .operation_timeout(Duration::from_secs(60))
        .build()
}

/// Uses standard retry mode with three total attempts. With `retry_aws_operation`,
/// 8 attempts plus 395 seconds of backoff take at most 8 * 60 + 395 = 875 seconds,
/// below the host's 20-minute per-guest-call backstop.
fn retry_config() -> RetryConfig {
    RetryConfig::standard().with_max_attempts(3)
}

#[cfg(test)]
pub(crate) fn assert_bounded_policy(timeout: Option<&TimeoutConfig>, retry: Option<&RetryConfig>) {
    use aws_smithy_types::retry::RetryMode;

    let timeout = timeout.expect("timeout config should be set");
    assert_eq!(
        timeout.connect_timeout(),
        Some(Duration::from_millis(3_100))
    );
    assert_eq!(timeout.read_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(
        timeout.operation_attempt_timeout(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(timeout.operation_timeout(), Some(Duration::from_secs(60)));

    let retry = retry.expect("retry config should be set");
    assert_eq!(retry.mode(), RetryMode::Standard);
    assert_eq!(retry.max_attempts(), 3);
}

impl AwsProvider {
    /// Construct an `AwsProvider` from a bounded SDK configuration.
    ///
    /// Production and integration-test clients are all constructed here. Adding
    /// a client field makes this one constructor fail to compile until the new
    /// client is wired to the same bounded configuration.
    pub fn from_sdk_config(
        region: String,
        allowed_account_ids: Vec<String>,
        forbidden_account_ids: Vec<String>,
        config: &BoundedSdkConfig,
    ) -> Self {
        Self {
            s3_client: S3Client::new(&config.0),
            ec2_client: Ec2Client::new(&config.0),
            iam_client: IamClient::new(&config.0),
            logs_client: CloudWatchLogsClient::new(&config.0),
            sts_client: StsClient::new(&config.0),
            organizations_client: OrganizationsClient::new(&config.0),
            identitystore_client: IdentityStoreClient::new(&config.0),
            route53_client: Route53Client::new(&config.0),
            acm_client: AcmClient::new(&config.0),
            sqs_client: SqsClient::new(&config.0),
            schema_registry: Self::schema_registry(),
            region,
            allowed_account_ids,
            forbidden_account_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_types::retry::RetryConfig;
    use aws_smithy_types::timeout::TimeoutConfig;

    use super::{BoundedSdkConfig, assert_bounded_policy};

    #[test]
    fn new_overrides_input_timeout_and_retry_config() {
        let input = aws_config::SdkConfig::builder()
            .timeout_config(TimeoutConfig::disabled())
            .retry_config(RetryConfig::standard().with_max_attempts(10))
            .build();

        let bounded = BoundedSdkConfig::new(input);
        assert_bounded_policy(bounded.0.timeout_config(), bounded.0.retry_config());
    }
}
