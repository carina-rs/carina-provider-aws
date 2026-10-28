//! Builds AWS SDK configurations with provider-wide timeout and retry bounds.

use std::time::Duration;

use aws_config::Region;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;

use crate::AwsProvider;
use crate::resend_safety_generated::{
    AcmClient, CloudWatchLogsClient, Ec2Client, IamClient, IdentityStoreClient,
    OrganizationsClient, Route53Client, S3Client, SqsClient, StsClient,
};
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

    /// Borrow the shared configuration for generated guarded-client constructors.
    ///
    /// Raw SDK client constructors are denied workspace-wide by the generated
    /// Clippy policy, so handwritten provider code cannot use this accessor to
    /// bypass the generated wrappers.
    pub(crate) fn sdk_config(&self) -> &aws_config::SdkConfig {
        &self.0
    }

    /// Build a bounded configuration around a test HTTP client while retaining
    /// the production timeout and retry policy.
    #[cfg(test)]
    pub(crate) fn for_test_http_client(
        http_client: aws_smithy_runtime_api::client::http::SharedHttpClient,
        endpoint_url: &'static str,
    ) -> Self {
        let credentials = aws_credential_types::Credentials::new(
            "test",
            "test",
            None,
            None,
            "bounded-sdk-config-test",
        );
        Self::new(
            aws_config::SdkConfig::builder()
                .region(Region::new("us-east-1"))
                .endpoint_url(endpoint_url)
                .credentials_provider(
                    aws_credential_types::provider::SharedCredentialsProvider::new(credentials),
                )
                .http_client(http_client)
                .sleep_impl(aws_smithy_async::rt::sleep::TokioSleep::new())
                .build(),
        )
    }

    /// Load a bounded SDK configuration for the region and optional assumed role.
    pub(crate) async fn load(region: &str, assume_role: Option<AssumeRoleConfig>) -> Self {
        // Give the loader the policy too so invalid AWS retry settings cannot panic first.
        let loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .timeout_config(timeout_config())
            .retry_config(retry_config());
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

/// Sets read and per-attempt timeouts to the host's 30-second `wasi:http` per-request
/// cap (`HTTP_API_REQUEST_TIMEOUT` in `carina-plugin-host`), so WASM requests get no
/// shorter deadline than the host already enforces. The operation timeout caps one
/// SDK call at 60 seconds natively. In WASM the HTTP client sends synchronously, so
/// connect/read timeouts reach `wasi:http`, but attempt/operation timeouts are checked
/// only between attempts and one call can overrun 60 seconds by one 30-second attempt,
/// taking about 90 seconds in the worst case.
fn timeout_config() -> TimeoutConfig {
    TimeoutConfig::builder()
        .connect_timeout(Duration::from_millis(3_100))
        .read_timeout(Duration::from_secs(30))
        .operation_attempt_timeout(Duration::from_secs(30))
        .operation_timeout(Duration::from_secs(60))
        .build()
}

/// Uses standard retry mode with three total attempts, overriding `AWS_MAX_ATTEMPTS`,
/// `AWS_RETRY_MODE`, and profile settings.
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
            s3_client: S3Client::from_bounded_config(config),
            ec2_client: Ec2Client::from_bounded_config(config),
            iam_client: IamClient::from_bounded_config(config),
            logs_client: CloudWatchLogsClient::from_bounded_config(config),
            sts_client: StsClient::from_bounded_config(config),
            organizations_client: OrganizationsClient::from_bounded_config(config),
            identitystore_client: IdentityStoreClient::from_bounded_config(config),
            route53_client: Route53Client::from_bounded_config(config),
            acm_client: AcmClient::from_bounded_config(config),
            sqs_client: SqsClient::from_bounded_config(config),
            schema_registry: Self::schema_registry(),
            region,
            allowed_account_ids,
            forbidden_account_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_runtime_api::client::interceptors::Intercept;
    use aws_smithy_types::retry::RetryConfig;
    use aws_smithy_types::timeout::TimeoutConfig;

    use crate::AwsProvider;

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

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn load_applies_bounded_policy() {
        let bounded = BoundedSdkConfig::load("us-east-1", None).await;
        assert_bounded_policy(bounded.0.timeout_config(), bounded.0.retry_config());
    }

    #[tokio::test]
    async fn every_provider_client_has_the_resend_guard() {
        let mock = winterbaume_core::MockAws::builder().build();
        let config = BoundedSdkConfig::new(mock.sdk_config("us-east-1").await);
        let provider =
            AwsProvider::from_sdk_config("us-east-1".to_string(), Vec::new(), Vec::new(), &config);
        let AwsProvider {
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
            schema_registry: _,
            region: _,
            allowed_account_ids: _,
            forbidden_account_ids: _,
        } = provider;

        macro_rules! assert_guard {
            ($client:expr) => {
                assert!(
                    $client
                        .config()
                        .interceptors()
                        .any(|interceptor| interceptor.name() == "carina.ResendGuard"),
                    "{} has no resend guard",
                    stringify!($client)
                );
            };
        }

        assert_guard!(s3_client);
        assert_guard!(ec2_client);
        assert_guard!(iam_client);
        assert_guard!(logs_client);
        assert_guard!(sts_client);
        assert_guard!(organizations_client);
        assert_guard!(identitystore_client);
        assert_guard!(route53_client);
        assert_guard!(acm_client);
        assert_guard!(sqs_client);
    }
}
