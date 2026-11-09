//! AWS SDK credentials supplied by the Carina host.

use std::time::SystemTime;

use aws_credential_types::{Credentials, provider::error::CredentialsError};
use carina_plugin_sdk::aws_credentials::AwsCredentialError;
use carina_plugin_sdk::protected_material::{
    AwsSessionFuture, AwsSessionMaterial, AwsSessionSource,
};

struct HostCredentials {
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    expires_at: Option<SystemTime>,
}

fn current_host_credentials() -> Result<HostCredentials, AwsCredentialError> {
    let credentials = carina_plugin_sdk::aws_credentials::current()?;
    Ok(HostCredentials {
        access_key_id: credentials.access_key_id().to_string(),
        secret_access_key: credentials.secret_access_key().to_string(),
        session_token: credentials.session_token().map(str::to_string),
        expires_at: credentials.expires_at(),
    })
}

/// Loads base credentials from the AWS-kind guest's typed host import.
///
/// The SDK caches these credentials until near their host-supplied expiry.
/// Native SDK configurations should keep using their default credential chain.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostCredentialsSource {
    #[cfg(test)]
    source: Option<fn() -> Result<HostCredentials, AwsCredentialError>>,
}

impl HostCredentialsSource {
    fn credentials(&self) -> Result<Credentials, CredentialsError> {
        #[cfg(test)]
        let source = self.source.unwrap_or(current_host_credentials);
        #[cfg(not(test))]
        let source = current_host_credentials;
        source()
            .map(|credentials| {
                Credentials::new(
                    credentials.access_key_id,
                    credentials.secret_access_key,
                    credentials.session_token,
                    credentials.expires_at,
                    "CarinaHost",
                )
            })
            .map_err(|error| match error {
                error @ AwsCredentialError::NotFound(_) => CredentialsError::not_loaded(error),
                error @ AwsCredentialError::ProviderFailed(_) => {
                    CredentialsError::provider_error(error)
                }
            })
    }
}

impl AwsSessionSource for HostCredentialsSource {
    fn current_session(&self) -> AwsSessionFuture<'_> {
        Box::pin(std::future::ready(
            self.credentials().map(AwsSessionMaterial::new),
        ))
    }
}

impl From<HostCredentialsSource> for crate::AwsCredentialsSource {
    fn from(source: HostCredentialsSource) -> Self {
        Self::new(move || std::future::ready(source.credentials()))
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
    use carina_core::provider::ProviderPlan;
    use carina_plugin_sdk::{ProviderDiagnostics, wasi_http::AwsSdkParts};

    fn registered(
        source: HostCredentialsSource,
    ) -> (
        carina_core::provider::ProviderSession,
        SharedCredentialsProvider,
    ) {
        let session = ProviderPlan::state_only().session("aws", None, None);
        let config = AwsSdkParts::new(source, ProviderDiagnostics::from_session(&session))
            .apply(aws_types::SdkConfig::builder())
            .build();
        (session, config.credentials_provider().unwrap())
    }

    const ACCESS_KEY: &str = "host-access-key-sentinel";
    const SECRET: &str = "host-secret-sentinel";
    const TOKEN: &str = "host-session-sentinel";

    fn expiring_credentials() -> Result<HostCredentials, AwsCredentialError> {
        Ok(HostCredentials {
            access_key_id: ACCESS_KEY.into(),
            secret_access_key: SECRET.into(),
            session_token: Some(TOKEN.into()),
            expires_at: Some(UNIX_EPOCH + Duration::from_secs(2_000_000_000)),
        })
    }

    #[tokio::test]
    async fn maps_credentials_and_expiry() {
        let provider = HostCredentialsSource {
            source: Some(expiring_credentials),
        };
        let (_session, provider) = registered(provider);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id(), ACCESS_KEY);
        assert_eq!(credentials.secret_access_key(), SECRET);
        assert_eq!(credentials.session_token(), Some(TOKEN));
        assert_eq!(
            credentials.expiry(),
            Some(UNIX_EPOCH + Duration::from_secs(2_000_000_000))
        );
        let debug = format!("{provider:?} {credentials:?}");
        assert!(!debug.contains(SECRET));
        assert!(!debug.contains(TOKEN));
    }

    #[tokio::test]
    async fn maps_credentials_without_token_or_expiry() {
        let provider = HostCredentialsSource {
            source: Some(|| {
                Ok(HostCredentials {
                    session_token: None,
                    expires_at: None,
                    ..expiring_credentials()?
                })
            }),
        };
        let (_session, provider) = registered(provider);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id(), ACCESS_KEY);
        assert_eq!(credentials.secret_access_key(), SECRET);
        assert_eq!(credentials.session_token(), None);
        assert_eq!(credentials.expiry(), None);
    }

    #[tokio::test]
    async fn not_found_is_credentials_not_loaded_with_host_message() {
        const MESSAGE: &str = "no host credential source produced credentials";
        let provider = HostCredentialsSource {
            source: Some(|| Err(AwsCredentialError::NotFound(MESSAGE.into()))),
        };
        let (_session, provider) = registered(provider);
        let error = provider.provide_credentials().await.unwrap_err();

        assert!(matches!(error, CredentialsError::CredentialsNotLoaded(_)));
        assert!(error.source().unwrap().is::<AwsCredentialError>());
        assert_eq!(
            error.source().unwrap().to_string(),
            AwsCredentialError::NotFound(MESSAGE.into()).to_string()
        );
        assert!(matches!(
            error.source().unwrap().downcast_ref::<AwsCredentialError>(),
            Some(AwsCredentialError::NotFound(message)) if message == MESSAGE
        ));
        assert!(!format!("{error:?}").contains(SECRET));
        assert!(!format!("{error:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn provider_failed_keeps_host_message() {
        const MESSAGE: &str = "SSO session expired; run aws sso login --profile example";
        let provider = HostCredentialsSource {
            source: Some(|| Err(AwsCredentialError::ProviderFailed(MESSAGE.into()))),
        };
        let (_session, provider) = registered(provider);
        let error = provider.provide_credentials().await.unwrap_err();

        assert!(matches!(error, CredentialsError::ProviderError(_)));
        assert!(error.source().unwrap().is::<AwsCredentialError>());
        assert_eq!(
            error.source().unwrap().to_string(),
            AwsCredentialError::ProviderFailed(MESSAGE.into()).to_string()
        );
        assert!(matches!(
            error.source().unwrap().downcast_ref::<AwsCredentialError>(),
            Some(AwsCredentialError::ProviderFailed(message)) if message == MESSAGE
        ));
        assert!(!format!("{error:?}").contains(SECRET));
        assert!(!format!("{error:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn asks_the_host_again_when_credentials_are_requested_again() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let provider = HostCredentialsSource {
            source: Some(|| {
                let call = CALLS.fetch_add(1, Ordering::Relaxed);
                Ok(HostCredentials {
                    access_key_id: format!("host-access-key-{call}"),
                    ..expiring_credentials()?
                })
            }),
        };

        let (_session, provider) = registered(provider);
        let first = provider.provide_credentials().await.unwrap();
        let second = provider.provide_credentials().await.unwrap();
        assert_eq!(CALLS.load(Ordering::Relaxed), 2);
        assert_ne!(first.access_key_id(), second.access_key_id());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_default_reports_the_sdk_stub_error() {
        let (_session, provider) = registered(HostCredentialsSource::default());
        let error = provider.provide_credentials().await.unwrap_err();

        assert!(matches!(error, CredentialsError::ProviderError(_)));
        assert!(
            error
                .source()
                .unwrap()
                .to_string()
                .contains("wasm32 AWS-kind provider guests")
        );
    }
}
