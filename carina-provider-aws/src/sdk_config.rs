//! Builds AWS SDK configurations with provider-wide timeout and retry bounds.

use std::time::Duration;

use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_types::region::Region;

use crate::AwsProvider;
use crate::resend_safety_generated::{
    AccountClient, AcmClient, CloudWatchLogsClient, Ec2Client, IamClient, IdentityStoreClient,
    OrganizationsClient, Route53Client, S3Client, SqsClient, StsClient,
};
use crate::services::sts::assume_role::AssumeRoleConfig;

/// An AWS SDK configuration with the provider's timeout and retry policy applied.
#[derive(Clone)]
pub struct BoundedSdkConfig(carina_aws_types::AwsSdkConfig);

impl BoundedSdkConfig {
    /// Apply the provider's timeout and retry policy to an SDK configuration.
    fn new(config: carina_aws_types::AwsSdkConfig) -> Self {
        Self(config.with_policy(timeout_config(), retry_config()))
    }

    /// Build a bounded configuration from an SDK configuration for tests.
    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
    pub fn for_test(config: aws_types::SdkConfig) -> Self {
        Self::new(carina_aws_types::AwsSdkConfig::for_test(config))
    }

    /// Borrow the shared configuration for guarded-client constructors.
    ///
    /// Only carina-aws-types can name and construct raw service clients; its
    /// factories always install the resend guard.
    pub(crate) fn sdk_config(&self) -> &carina_aws_types::AwsSdkConfig {
        &self.0
    }

    /// Keep effective credentials and bounds when routing a qualified reference.
    pub(crate) fn in_region(&self, region: &str) -> Self {
        Self(self.0.clone().in_region(Region::new(region.to_string())))
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
            aws_types::SdkConfig::builder()
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
    pub(crate) async fn load(
        region: &str,
        assume_role: Option<AssumeRoleConfig>,
        diagnostics: carina_plugin_sdk::ProviderDiagnostics,
    ) -> Self {
        let base = Self::new(
            carina_aws_types::load_sdk_config(
                Region::new(region.to_string()),
                timeout_config(),
                retry_config(),
                diagnostics,
                None,
            )
            .await,
        );

        match assume_role {
            None => base,
            Some(assume_role) => base.with_assume_role(assume_role),
        }
    }

    fn with_assume_role(self, assume_role: AssumeRoleConfig) -> Self {
        let provider = carina_aws_types::GuardedAssumeRoleProvider::new(
            &self.0,
            assume_role.role_arn,
            assume_role.session_name,
            assume_role.external_id,
            assume_role.duration,
        );
        Self::new(self.0.with_source(provider))
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
            account_client: AccountClient::from_bounded_config(config),
            organizations_client: OrganizationsClient::from_bounded_config(config),
            identitystore_client: IdentityStoreClient::from_bounded_config(config),
            route53_client: Route53Client::from_bounded_config(config),
            acm_client: AcmClient::from_bounded_config(config),
            sqs_client: SqsClient::from_bounded_config(config),
            kms_client: crate::references::KmsClient::from_bounded_config(config),
            diagnostics: config.0.diagnostics().clone(),
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
        let input = aws_types::SdkConfig::builder()
            .timeout_config(TimeoutConfig::disabled())
            .retry_config(RetryConfig::standard().with_max_attempts(10))
            .build();

        let bounded = BoundedSdkConfig::for_test(input);
        assert_bounded_policy(
            bounded.0.sdk_config().timeout_config(),
            bounded.0.sdk_config().retry_config(),
        );
    }

    #[test]
    fn provider_dependencies_cannot_name_raw_service_clients() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        // Check the complete manifest, including dev and target dependencies.
        assert!(!manifest.contains("aws-sdk-"));
        assert!(!manifest.contains("aws-config"));
    }

    #[tokio::test]
    async fn kms_reference_routing_keeps_assumed_credentials_and_qualified_arn_scope() {
        use carina_aws_types::AwsType;
        use carina_plugin_sdk::types::ReferenceOutcome;
        use carina_provider_protocol::types::{ReferenceRequest, TypeIdentity};
        use std::sync::{Arc, Mutex};
        use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

        struct ScopedKms(Arc<Mutex<Vec<MockRequest>>>);
        impl MockService for ScopedKms {
            fn service_name(&self) -> &str {
                "kms-scope-test"
            }
            fn url_patterns(&self) -> Vec<&str> {
                vec![r"^https://(kms|sts)\."]
            }
            fn handle(
                &self,
                request: MockRequest,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = MockResponse> + Send + '_>>
            {
                let response = if request.uri.contains("sts.") {
                    MockResponse::xml(
                        200,
                        "<AssumeRoleResponse xmlns=\"https://sts.amazonaws.com/doc/2011-06-15/\"><AssumeRoleResult><Credentials><AccessKeyId>ASSUMEDKMSKEY</AccessKeyId><SecretAccessKey>fake-assumed-secret</SecretAccessKey><SessionToken>fake-assumed-token</SessionToken><Expiration>2099-01-01T00:00:00Z</Expiration></Credentials><AssumedRoleUser><AssumedRoleId>ROLE:session</AssumedRoleId><Arn>arn:aws:sts::222222222222:assumed-role/test/session</Arn></AssumedRoleUser></AssumeRoleResult><ResponseMetadata><RequestId>sts-test</RequestId></ResponseMetadata></AssumeRoleResponse>",
                    )
                } else {
                    assert_eq!(
                        request.headers.get("x-amz-target").unwrap(),
                        "TrentService.DescribeKey"
                    );
                    assert!(
                        request
                            .headers
                            .get("authorization")
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .contains("Credential=ASSUMEDKMSKEY/")
                    );
                    assert_eq!(
                        request.headers.get("x-amz-security-token").unwrap(),
                        "fake-assumed-token"
                    );
                    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    let raw = body["KeyId"].as_str().unwrap();
                    let arn = if raw == "alias/instance" {
                        assert_eq!(request.uri, "https://kms.us-east-1.amazonaws.com/");
                        "arn:aws:kms:us-east-1:222222222222:key/11111111-1111-1111-1111-111111111111"
                    } else {
                        assert_eq!(raw, "arn:aws:kms:eu-west-1:999999999999:alias/customer");
                        assert_eq!(request.uri, "https://kms.eu-west-1.amazonaws.com/");
                        "arn:aws:kms:eu-west-1:999999999999:key/22222222-2222-2222-2222-222222222222"
                    };
                    MockResponse::json(
                        200,
                        format!(r#"{{"KeyMetadata":{{"KeyId":"test","Arn":"{arn}"}}}}"#),
                    )
                };
                self.0.lock().unwrap().push(request);
                Box::pin(async move { response })
            }
        }
        let calls = Arc::new(Mutex::new(vec![]));
        let mock = MockAws::builder()
            .with_service(ScopedKms(calls.clone()))
            .build();
        let config = BoundedSdkConfig::for_test(mock.sdk_config("us-east-1").await)
            .with_assume_role(crate::services::sts::assume_role::AssumeRoleConfig {
                role_arn: "arn:aws:iam::222222222222:role/test".into(),
                session_name: Some("kms-test".into()),
                external_id: None,
                duration: None,
            });
        let provider = AwsProvider::from_sdk_config("us-east-1".into(), vec![], vec![], &config);
        let request = |value: &str| ReferenceRequest {
            ty: TypeIdentity {
                provider: "aws".into(),
                segments: vec!["kms".into(), "Key".into()],
                kind: "Reference".into(),
            },
            value: value.into(),
        };
        let results = provider
            .resolve_guest_references(vec![
                request("alias/instance"),
                request("arn:aws:kms:eu-west-1:999999999999:alias/customer"),
            ])
            .await;
        assert!(
            matches!(&results[0], Ok(ReferenceOutcome::Resolved(arn)) if arn.contains(":us-east-1:222222222222:"))
        );
        assert!(
            matches!(&results[1], Ok(ReferenceOutcome::Resolved(arn)) if arn.contains(":eu-west-1:999999999999:"))
        );
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.uri.contains("kms."))
                .count(),
            2
        );

        let native = carina_core::provider::Provider::resolve_references(&provider, &[
            carina_core::references::ReferenceRequest { ty: AwsType::KmsKeyReference.identity(), value: "arn:aws:kms:eu-west-1:999999999999:key/22222222-2222-2222-2222-222222222222".into() },
            carina_core::references::ReferenceRequest { ty: AwsType::KmsKeyArn.identity(), value: "alias/unsupported".into() },
        ]).await.unwrap();
        assert!(matches!(
            &native[0],
            Ok(carina_core::references::ReferenceOutcome::Resolved(_))
        ));
        assert!(matches!(
            &native[1],
            Err(carina_core::references::ReferenceError::InvalidReference(_))
        ));
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|request| request.uri.contains("kms."))
                .count(),
            2,
            "key ARNs and unsupported identities must not issue KMS calls"
        );
        assert_eq!(
            calls
                .iter()
                .filter(|request| request.uri.contains("sts."))
                .count(),
            1,
            "routing must reuse one STS AssumeRole session"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn load_applies_bounded_policy() {
        use aws_credential_types::provider::ProvideCredentials;

        let mock = winterbaume_core::MockAws::builder().build();
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let expected = mock
            .credentials_provider()
            .provide_credentials()
            .await
            .unwrap();
        let bounded = BoundedSdkConfig::new(
            carina_aws_types::load_sdk_config(
                aws_types::region::Region::new("us-east-1"),
                super::timeout_config(),
                super::retry_config(),
                carina_plugin_sdk::ProviderDiagnostics::from_session(&session),
                Some(expected.clone().into()),
            )
            .await,
        );
        assert_bounded_policy(
            bounded.0.sdk_config().timeout_config(),
            bounded.0.sdk_config().retry_config(),
        );
        assert_eq!(
            bounded.0.sdk_config().region().unwrap().as_ref(),
            "us-east-1"
        );
        let actual = bounded
            .0
            .sdk_config()
            .credentials_provider()
            .unwrap()
            .provide_credentials()
            .await
            .unwrap();
        assert_eq!(actual.access_key_id(), expected.access_key_id());
        assert_eq!(actual.session_token(), expected.session_token());
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
            kms_client,
            schema_registry: _,
            region: _,
            allowed_account_ids: _,
            forbidden_account_ids: _,
            diagnostics: _,
        } = provider;

        macro_rules! assert_guard {
            ($client:expr) => {{
                let guarded = $client.guarded_client();
                let config = guarded.inner().config();
                assert_bounded_policy(config.timeout_config(), config.retry_config());
                assert!(
                    config
                        .interceptors()
                        .any(|interceptor| interceptor.name() == "carina.ResendGuard"),
                    "{} has no resend guard",
                    stringify!($client)
                );
            }};
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
        assert_guard!(kms_client);
    }
}
