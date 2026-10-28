//! Builds AWS SDK configurations with provider-wide timeout and retry bounds.

use std::time::Duration;

use aws_config::Region;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;

use crate::AwsProvider;
use crate::resend_guard::ResendGuard;
use crate::resend_safety_generated::{
    acm_idempotency, cloudwatchlogs_idempotency, ec2_idempotency, iam_idempotency,
    identitystore_idempotency, organizations_idempotency, route53_idempotency, s3_idempotency,
    sqs_idempotency, sts_idempotency,
};
use crate::services::sts::assume_role::AssumeRoleConfig;

macro_rules! guarded_client {
    ($sdk:ident, $config:expr, $idempotency:path) => {
        $sdk::Client::from_conf(
            $sdk::config::Builder::from(&$config.0)
                .interceptor(ResendGuard::new($idempotency))
                .build(),
        )
    };
}

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
/// only between attempts and one call can overrun 60 seconds by up to one attempt.
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
            s3_client: guarded_client!(aws_sdk_s3, config, s3_idempotency),
            ec2_client: guarded_client!(aws_sdk_ec2, config, ec2_idempotency),
            iam_client: guarded_client!(aws_sdk_iam, config, iam_idempotency),
            logs_client: guarded_client!(
                aws_sdk_cloudwatchlogs,
                config,
                cloudwatchlogs_idempotency
            ),
            sts_client: guarded_client!(aws_sdk_sts, config, sts_idempotency),
            organizations_client: guarded_client!(
                aws_sdk_organizations,
                config,
                organizations_idempotency
            ),
            identitystore_client: guarded_client!(
                aws_sdk_identitystore,
                config,
                identitystore_idempotency
            ),
            route53_client: guarded_client!(aws_sdk_route53, config, route53_idempotency),
            acm_client: guarded_client!(aws_sdk_acm, config, acm_idempotency),
            sqs_client: guarded_client!(aws_sdk_sqs, config, sqs_idempotency),
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
