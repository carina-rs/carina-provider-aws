//! Builds AWS SDK configurations with provider-wide timeout and retry bounds.

use std::time::Duration;

use aws_config::Region;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;

use crate::AwsProvider;
use crate::resend_safety_generated::{
    AccountClient, AcmClient, CloudWatchLogsClient, Ec2Client, IamClient, IdentityStoreClient,
    OrganizationsClient, Route53Client, S3Client, SqsClient, StsClient,
};
use crate::services::sts::assume_role::AssumeRoleConfig;

/// An AWS SDK configuration with the provider's timeout and retry policy applied.
pub struct BoundedSdkConfig(aws_config::SdkConfig);

impl BoundedSdkConfig {
    /// Apply the provider's timeout and retry policy to an SDK configuration.
    fn new(config: aws_config::SdkConfig) -> Self {
        Self(
            config
                .into_builder()
                .timeout_config(timeout_config())
                .retry_config(retry_config())
                .build(),
        )
    }

    /// Build a bounded configuration from an SDK configuration for tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(config: aws_config::SdkConfig) -> Self {
        Self::new(config)
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
        Self::for_test(
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
        Self::load_with_overrides(region, assume_role, None, None).await
    }

    async fn load_with_overrides(
        region: &str,
        assume_role: Option<AssumeRoleConfig>,
        http_client: Option<aws_smithy_runtime_api::client::http::SharedHttpClient>,
        credentials_provider: Option<aws_credential_types::provider::SharedCredentialsProvider>,
    ) -> Self {
        // Give the loader the policy too so invalid AWS retry settings cannot panic first.
        // This is the sole SDK loader; wasm32 always installs host credentials below.
        #[allow(clippy::disallowed_methods)]
        let loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .timeout_config(timeout_config())
            .retry_config(retry_config());
        let loader = match http_client {
            Some(http_client) => loader.http_client(http_client),
            None => loader,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let loader = match base_credentials_provider(credentials_provider) {
            Some(credentials_provider) => loader.credentials_provider(credentials_provider),
            None => loader,
        };
        #[cfg(target_arch = "wasm32")]
        let loader = loader.credentials_provider(base_credentials_provider(credentials_provider));
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

#[cfg(target_arch = "wasm32")]
fn base_credentials_provider(
    override_provider: Option<SharedCredentialsProvider>,
) -> SharedCredentialsProvider {
    override_provider.unwrap_or_else(|| {
        SharedCredentialsProvider::new(carina_aws_types::HostCredentialsProvider::default())
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn base_credentials_provider(
    override_provider: Option<SharedCredentialsProvider>,
) -> Option<SharedCredentialsProvider> {
    override_provider
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
            account_client: AccountClient::from_bounded_config(config),
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

    use super::{BoundedSdkConfig, assert_bounded_policy, base_credentials_provider};

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_base_keeps_default_credentials_chain() {
        assert!(base_credentials_provider(None).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_base_keeps_credentials_override() {
        use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};

        let credentials = aws_credential_types::Credentials::new(
            "override-access-key",
            "override-secret",
            Some("override-token".into()),
            None,
            "sdk-config-test",
        );
        let provider = base_credentials_provider(Some(SharedCredentialsProvider::new(credentials)))
            .expect("the explicit credentials override must be retained");
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id(), "override-access-key");
        assert_eq!(credentials.secret_access_key(), "override-secret");
        assert_eq!(credentials.session_token(), Some("override-token"));
    }

    #[test]
    fn new_overrides_input_timeout_and_retry_config() {
        let input = aws_config::SdkConfig::builder()
            .timeout_config(TimeoutConfig::disabled())
            .retry_config(RetryConfig::standard().with_max_attempts(10))
            .build();

        let bounded = BoundedSdkConfig::for_test(input);
        assert_bounded_policy(bounded.0.timeout_config(), bounded.0.retry_config());
    }

    #[test]
    fn every_aws_sdk_dependency_has_a_wrapper_and_constructor_bans() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        let generated = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/resend_safety_generated.rs"
        ));
        let clippy = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../clippy.toml"));
        let mut services = std::collections::BTreeSet::new();
        for line in manifest.lines() {
            let mut declaration = line.split('#').next().unwrap_or_default();
            while let Some(prefix) = declaration.find("aws-sdk-") {
                let candidate = &declaration[prefix + "aws-sdk-".len()..];
                let length = candidate
                    .find(|character: char| {
                        !character.is_ascii_lowercase()
                            && !character.is_ascii_digit()
                            && character != '-'
                    })
                    .unwrap_or(candidate.len());
                if length != 0 {
                    services.insert(&candidate[..length]);
                }
                declaration = &candidate[length..];
            }
        }

        assert!(!services.is_empty(), "expected AWS SDK dependencies");
        for service in services {
            let rust_service = service.replace('-', "_");
            assert!(
                generated.contains(&format!("inner: aws_sdk_{rust_service}::Client,")),
                "aws-sdk-{service} has no generated wrapper; add resend decisions before adding the dependency"
            );
            for constructor in ["new", "from_conf"] {
                assert!(
                    clippy.contains(&format!("aws_sdk_{rust_service}::Client::{constructor}")),
                    "aws-sdk-{service} is missing the {constructor} constructor ban"
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn load_applies_bounded_policy() {
        let mock = winterbaume_core::MockAws::builder().build();
        let bounded = BoundedSdkConfig::load_with_overrides(
            "us-east-1",
            None,
            Some(mock.http_client()),
            Some(mock.credentials_provider()),
        )
        .await;
        assert_bounded_policy(bounded.0.timeout_config(), bounded.0.retry_config());
    }

    #[tokio::test]
    async fn every_provider_client_has_the_resend_guard() {
        let mock = winterbaume_core::MockAws::builder().build();
        let config = BoundedSdkConfig::for_test(mock.sdk_config("us-east-1").await);
        let provider =
            AwsProvider::from_sdk_config("us-east-1".to_string(), Vec::new(), Vec::new(), &config);
        let AwsProvider {
            s3_client,
            ec2_client,
            iam_client,
            logs_client,
            sts_client,
            account_client,
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
        assert_guard!(account_client);
        assert_guard!(organizations_client);
        assert_guard!(identitystore_client);
        assert_guard!(route53_client);
        assert_guard!(acm_client);
        assert_guard!(sqs_client);
    }
}
