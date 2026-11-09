//! Session-bound SDK configuration and refreshing credential sources.

use std::{future::Future, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_credential_types::{Credentials, provider::error::CredentialsError};
use aws_smithy_runtime_api::client::behavior_version::BehaviorVersion;
use aws_smithy_types::{retry::RetryConfig, timeout::TimeoutConfig};
use aws_types::{SdkConfig, region::Region};
#[cfg(any(target_arch = "wasm32", test, feature = "test-utils"))]
use carina_plugin_sdk::wasi_http::AwsSdkParts;
use carina_plugin_sdk::{
    ProviderDiagnostics,
    protected_material::{AwsSessionFuture, AwsSessionMaterial, AwsSessionSource},
};
use futures::future::BoxFuture;

type CredentialsLoad =
    dyn Fn() -> BoxFuture<'static, Result<Credentials, CredentialsError>> + Send + Sync;

/// A refreshing source of complete SDK credentials, never a signing provider.
///
/// Host credentials, guarded AssumeRole and explicit credentials convert with
/// `Into`. Use `new` for another refreshing source. WASM always registers each
/// generation through `AwsSdkParts` before it can reach a signer.
///
/// ```
/// use carina_aws_types::AwsCredentialsSource;
/// use carina_plugin_sdk::protected_material::AwsSessionSource;
/// fn requires_source<T: AwsSessionSource>() {}
/// requires_source::<AwsCredentialsSource>();
/// ```
///
/// A source cannot be installed directly as a signing credentials provider:
///
/// ```compile_fail
/// use carina_aws_types::AwsCredentialsSource;
/// use aws_credential_types::provider::ProvideCredentials;
/// fn requires_provider<T: ProvideCredentials>() {}
/// requires_provider::<AwsCredentialsSource>();
/// ```
#[derive(Clone)]
pub struct AwsCredentialsSource(Arc<CredentialsLoad>);

impl std::fmt::Debug for AwsCredentialsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<AWS credentials source>")
    }
}

impl AwsCredentialsSource {
    pub fn new<F, Fut>(load: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Credentials, CredentialsError>> + Send + 'static,
    {
        Self(Arc::new(move || Box::pin(load())))
    }

    fn credentials(&self) -> BoxFuture<'static, Result<Credentials, CredentialsError>> {
        (self.0)()
    }
}

impl From<Credentials> for AwsCredentialsSource {
    fn from(credentials: Credentials) -> Self {
        Self::new(move || std::future::ready(Ok(credentials.clone())))
    }
}

impl AwsSessionSource for AwsCredentialsSource {
    fn current_session(&self) -> AwsSessionFuture<'_> {
        let credentials = self.credentials();
        Box::pin(async move { credentials.await.map(AwsSessionMaterial::new) })
    }
}

// Native providers are trusted host code. This adapter cannot exist in guests.
#[cfg(all(not(target_arch = "wasm32"), not(any(test, feature = "test-utils"))))]
#[derive(Debug)]
struct NativeSourceCredentials(AwsCredentialsSource);
#[cfg(all(not(target_arch = "wasm32"), not(any(test, feature = "test-utils"))))]
impl ProvideCredentials for NativeSourceCredentials {
    fn provide_credentials<'a>(
        &'a self,
    ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        aws_credential_types::provider::future::ProvideCredentials::new(self.0.credentials())
    }
}

#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
fn sdk_credentials_source(provider: Option<SharedCredentialsProvider>) -> AwsCredentialsSource {
    AwsCredentialsSource::new(move || {
        let provider = provider.clone();
        async move {
            let provider = provider
                .ok_or_else(|| CredentialsError::not_loaded("no credentials source configured"))?;
            provider.provide_credentials().await
        }
    })
}

/// Shared configuration paired with the provider session's diagnostics.
/// Guarded service factories require this type so every retry uses that channel.
#[derive(Clone)]
pub struct AwsSdkConfig {
    config: SdkConfig,
    diagnostics: ProviderDiagnostics,
    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
    test_session: Option<carina_core::provider::ProviderSession>,
}

impl std::fmt::Debug for AwsSdkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsSdkConfig")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl AwsSdkConfig {
    /// Replace the input configuration's credentials with this source. WASM also
    /// replaces its transport with the paired WASI transport. Native tests retain
    /// their explicit emulator transport after installing the registration pair.
    pub fn from_source(
        config: SdkConfig,
        source: impl Into<AwsCredentialsSource>,
        diagnostics: ProviderDiagnostics,
    ) -> Self {
        let source = source.into();
        #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
        let http_client = config.http_client();
        #[cfg(any(target_arch = "wasm32", test, feature = "test-utils"))]
        let builder = AwsSdkParts::new(source, diagnostics.clone()).apply(config.into_builder());
        #[cfg(all(not(target_arch = "wasm32"), not(any(test, feature = "test-utils"))))]
        let builder = config
            .into_builder()
            .credentials_provider(SharedCredentialsProvider::new(NativeSourceCredentials(
                source,
            )));
        #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
        let builder = match http_client {
            Some(client) => builder.http_client(client),
            None => builder,
        };
        Self {
            config: builder.build(),
            diagnostics,
            #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
            test_session: None,
        }
    }

    pub fn sdk_config(&self) -> &SdkConfig {
        &self.config
    }
    pub fn diagnostics(&self) -> &ProviderDiagnostics {
        &self.diagnostics
    }

    pub fn with_source(self, source: impl Into<AwsCredentialsSource>) -> Self {
        Self {
            #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
            test_session: self.test_session,
            ..Self::from_source(self.config, source, self.diagnostics)
        }
    }

    pub fn with_policy(mut self, timeout: TimeoutConfig, retry: RetryConfig) -> Self {
        self.config = self
            .config
            .into_builder()
            .timeout_config(timeout)
            .retry_config(retry)
            .build();
        self
    }

    pub fn in_region(mut self, region: Region) -> Self {
        self.config = self.config.into_builder().region(region).build();
        self
    }

    #[cfg(feature = "sts")]
    pub(crate) fn without_identity_cache(mut self) -> Self {
        self.config = self
            .config
            .into_builder()
            .identity_cache(aws_smithy_runtime::client::identity::IdentityCache::no_cache())
            .build();
        self
    }

    /// Native offline injection still registers credentials and retains its session.
    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
    pub fn for_test(config: SdkConfig) -> Self {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let source = sdk_credentials_source(config.credentials_provider());
        let mut config =
            Self::from_source(config, source, ProviderDiagnostics::from_session(&session));
        config.test_session = Some(session);
        config
    }
}

/// Load bounded SDK settings with one API on every target. WASM defaults to the
/// host import and installs only the `AwsSdkParts` registration/transport pair.
/// Native production keeps aws-config's default chain; source overrides use a
/// private native adapter. Native tests exercise the real registration pair too.
pub async fn load_sdk_config(
    region: Region,
    timeout_config: TimeoutConfig,
    retry_config: RetryConfig,
    diagnostics: ProviderDiagnostics,
    credentials_source: Option<AwsCredentialsSource>,
) -> AwsSdkConfig {
    let loader = aws_config::defaults(BehaviorVersion::latest())
        .region(region.clone())
        .timeout_config(timeout_config)
        .retry_config(retry_config);
    #[cfg(any(target_arch = "wasm32", test, feature = "test-utils"))]
    {
        #[cfg(target_arch = "wasm32")]
        let source = wasm_source(credentials_source);
        #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-utils")))]
        let source = match credentials_source {
            Some(source) => source,
            None => sdk_credentials_source(Some(SharedCredentialsProvider::new(
                aws_config::default_provider::credentials::DefaultCredentialsChain::builder()
                    .region(region)
                    .build()
                    .await,
            ))),
        };
        let mut paired =
            AwsSdkConfig::from_source(SdkConfig::builder().build(), source, diagnostics);
        // Supply the pair through the loader's existing overrides. Loading must
        // use the same transport and registering credentials provider as signing.
        let http_client = paired
            .config
            .http_client()
            .expect("AwsSdkParts installs transport");
        let credentials = paired
            .config
            .credentials_provider()
            .expect("AwsSdkParts installs credentials");
        paired.config = loader
            .http_client(http_client)
            .credentials_provider(credentials)
            .load()
            .await;
        paired
    }
    #[cfg(all(not(target_arch = "wasm32"), not(any(test, feature = "test-utils"))))]
    {
        let loader = match credentials_source {
            Some(source) => loader.credentials_provider(NativeSourceCredentials(source)),
            None => loader,
        };
        AwsSdkConfig {
            config: loader.load().await,
            diagnostics,
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn wasm_source(override_source: Option<AwsCredentialsSource>) -> AwsCredentialsSource {
    override_source.unwrap_or_else(|| crate::HostCredentialsSource::default().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::Credentials;
    use carina_core::provider::ProviderPlan;
    use carina_core::provider_text::{ProviderDiagnostic, present_event};
    use carina_plugin_sdk::{EventLevel, ProviderEvent};
    use std::{
        error::Error,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn review_round1_wasm_loader_defaults_to_host_credentials() {
        let session = ProviderPlan::state_only().session("aws", None, None);
        let selected = AwsSdkConfig::from_source(
            SdkConfig::builder().build(),
            wasm_source(None),
            ProviderDiagnostics::from_session(&session),
        );
        let error = selected
            .sdk_config()
            .credentials_provider()
            .unwrap()
            .provide_credentials()
            .await
            .unwrap_err();
        assert!(
            error
                .source()
                .unwrap()
                .is::<carina_plugin_sdk::aws_credentials::AwsCredentialError>()
        );
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
        let session = ProviderPlan::state_only().session("aws", None, None);
        let credentials = Credentials::new(
            "override-access-key",
            "override-secret",
            Some("override-token".into()),
            None,
            "loader-test",
        );
        let source = AwsCredentialsSource::from(credentials);
        let selected = AwsSdkConfig::from_source(
            SdkConfig::builder().build(),
            wasm_source(Some(source)),
            ProviderDiagnostics::from_session(&session),
        );
        let credentials = selected
            .sdk_config()
            .credentials_provider()
            .unwrap()
            .provide_credentials()
            .await
            .unwrap();
        assert_eq!(credentials.access_key_id(), "override-access-key");
        assert_eq!(credentials.secret_access_key(), "override-secret");
        assert_eq!(credentials.session_token(), Some("override-token"));
    }

    fn generations(calls: Arc<AtomicUsize>) -> AwsCredentialsSource {
        AwsCredentialsSource::new(move || {
            let generation = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                Ok(Credentials::new(
                    format!("rotating-access-{generation}"),
                    format!("rotating-secret-{generation}"),
                    Some(format!("rotating-token-{generation}")),
                    Some(UNIX_EPOCH + Duration::from_secs(2_000_000_000 + generation as u64)),
                    "rotating-test",
                ))
            }
        })
    }

    #[tokio::test]
    async fn registers_each_generation_and_expiry_before_returning_signing_credentials() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let session = ProviderPlan::state_only()
            .session_builder("aws", None, None)
            .with_diagnostic_sink(move |event| {
                captured
                    .lock()
                    .unwrap()
                    .push(present_event(&event).to_string())
            })
            .build();
        let diagnostics = ProviderDiagnostics::from_session(&session);
        let calls = Arc::new(AtomicUsize::new(0));
        let config = AwsSdkConfig::from_source(
            SdkConfig::builder().build(),
            generations(calls.clone()),
            diagnostics.clone(),
        );
        let provider = config.sdk_config().credentials_provider().unwrap();
        for generation in 0..2 {
            let credentials = provider.provide_credentials().await.unwrap();
            assert_eq!(
                credentials.access_key_id(),
                format!("rotating-access-{generation}")
            );
            assert_eq!(
                credentials.expiry(),
                Some(UNIX_EPOCH + Duration::from_secs(2_000_000_000 + generation))
            );
            let diagnostic = ProviderDiagnostic::new(std::io::Error::other(format!(
                "rotating-access-{generation} rotating-secret-{generation} rotating-token-{generation}"
            )));
            diagnostics.emit(ProviderEvent::diagnostic(
                EventLevel::Warn,
                "generation".into(),
                &diagnostic,
            ));
        }
        // A later event must still redact the first generation too.
        let diagnostic =
            ProviderDiagnostic::new(std::io::Error::other("rotating-secret-0 rotating-token-0"));
        diagnostics.emit(ProviderEvent::diagnostic(
            EventLevel::Warn,
            "older generation".into(),
            &diagnostic,
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|event| !event.contains("rotating-") && event.contains("(secret)"))
        );
    }

    #[tokio::test]
    async fn registration_failure_never_returns_signing_credentials() {
        let session = ProviderPlan::state_only().session("aws", None, None);
        let config = AwsSdkConfig::from_source(
            SdkConfig::builder().build(),
            generations(Arc::new(AtomicUsize::new(0))),
            ProviderDiagnostics::from_session(&session),
        );
        drop(session);
        let error = config
            .sdk_config()
            .credentials_provider()
            .unwrap()
            .provide_credentials()
            .await
            .unwrap_err();
        assert!(matches!(error, CredentialsError::ProviderError(_)));
        assert!(
            error
                .source()
                .unwrap()
                .is::<carina_plugin_sdk::protected_material::RegistrationError>()
        );
    }
}
