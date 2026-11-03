//! Load shared SDK configuration without exposing aws-config's client builders.

use aws_credential_types::provider::SharedCredentialsProvider;
use aws_smithy_runtime_api::client::behavior_version::BehaviorVersion;
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_types::{SdkConfig, region::Region};

/// Load the default SDK configuration with the provider's explicit policy.
///
/// Callers supply their region and bounds, plus any native transport and
/// credential overrides. WASM defaults to host credentials and always uses
/// the host's HTTP transport. Only the shared configuration leaves this crate;
/// aws-config's loader and credential-provider constructors are not re-exported.
pub async fn load_sdk_config(
    region: Region,
    timeout_config: TimeoutConfig,
    retry_config: RetryConfig,
    http_client: Option<SharedHttpClient>,
    credentials_provider: Option<SharedCredentialsProvider>,
) -> SdkConfig {
    #[cfg(target_arch = "wasm32")]
    let credentials_provider = Some(wasm_credentials_provider(credentials_provider));
    // Give the loader the policy too so invalid AWS retry settings cannot panic first.
    // This is the sole SDK loader; providers cannot name aws-config's client builders.
    let loader = aws_config::defaults(BehaviorVersion::latest())
        .region(region)
        .timeout_config(timeout_config)
        .retry_config(retry_config);
    let loader = match http_client {
        Some(http_client) => loader.http_client(http_client),
        None => loader,
    };
    let loader = match credentials_provider {
        Some(credentials_provider) => loader.credentials_provider(credentials_provider),
        None => loader,
    };
    #[cfg(target_arch = "wasm32")]
    let loader = loader.http_client(carina_plugin_sdk::wasi_http::WasiHttpClient::new());
    loader.load().await
}

#[cfg(any(target_arch = "wasm32", test))]
fn wasm_credentials_provider(
    override_provider: Option<SharedCredentialsProvider>,
) -> SharedCredentialsProvider {
    override_provider.unwrap_or_else(|| {
        SharedCredentialsProvider::new(crate::HostCredentialsProvider::default())
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use aws_credential_types::provider::ProvideCredentials;
    use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};

    use super::wasm_credentials_provider;

    #[tokio::test]
    async fn review_round1_wasm_loader_defaults_to_host_credentials() {
        let selected = wasm_credentials_provider(None);
        // Native tests use the typed host import's stub. This identifies the
        // host provider without consulting an environment/profile/IMDS chain.
        let error = selected.provide_credentials().await.unwrap_err();
        assert!(
            error
                .source()
                .unwrap()
                .to_string()
                .contains("wasm32 AWS-kind provider guests")
        );
    }

    #[tokio::test]
    async fn review_round1_wasm_loader_keeps_credentials_override() {
        let credentials = Credentials::new(
            "override-access-key",
            "override-secret",
            Some("override-token".into()),
            None,
            "loader-test",
        );
        let selected = wasm_credentials_provider(Some(SharedCredentialsProvider::new(credentials)));
        let credentials = selected.provide_credentials().await.unwrap();
        assert_eq!(credentials.access_key_id(), "override-access-key");
        assert_eq!(credentials.secret_access_key(), "override-secret");
        assert_eq!(credentials.session_token(), Some("override-token"));
    }
}
