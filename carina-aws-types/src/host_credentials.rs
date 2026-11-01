//! AWS SDK credentials supplied by the Carina host.

use std::time::SystemTime;

use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, error::CredentialsError, future},
};
use carina_plugin_sdk::aws_credentials::AwsCredentialError;

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
pub struct HostCredentialsProvider {
    #[cfg(test)]
    source: Option<fn() -> Result<HostCredentials, AwsCredentialError>>,
}

impl ProvideCredentials for HostCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        #[cfg(test)]
        let source = self.source.unwrap_or(current_host_credentials);
        #[cfg(not(test))]
        let source = current_host_credentials;

        future::ProvideCredentials::ready(
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
                    // The host contract guarantees secret-free error messages.
                    AwsCredentialError::NotFound(message) => CredentialsError::not_loaded(message),
                    AwsCredentialError::ProviderFailed(message) => {
                        CredentialsError::provider_error(message)
                    }
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

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
        let provider = HostCredentialsProvider {
            source: Some(expiring_credentials),
        };
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
        let provider = HostCredentialsProvider {
            source: Some(|| {
                Ok(HostCredentials {
                    session_token: None,
                    expires_at: None,
                    ..expiring_credentials()?
                })
            }),
        };
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id(), ACCESS_KEY);
        assert_eq!(credentials.secret_access_key(), SECRET);
        assert_eq!(credentials.session_token(), None);
        assert_eq!(credentials.expiry(), None);
    }

    #[tokio::test]
    async fn not_found_is_credentials_not_loaded_with_host_message() {
        const MESSAGE: &str = "no host credential source produced credentials";
        let provider = HostCredentialsProvider {
            source: Some(|| Err(AwsCredentialError::NotFound(MESSAGE.into()))),
        };
        let error = provider.provide_credentials().await.unwrap_err();

        assert!(matches!(error, CredentialsError::CredentialsNotLoaded(_)));
        assert_eq!(error.source().unwrap().to_string(), MESSAGE);
        assert!(!format!("{error:?}").contains(SECRET));
        assert!(!format!("{error:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn provider_failed_keeps_host_message() {
        const MESSAGE: &str = "SSO session expired; run aws sso login --profile example";
        let provider = HostCredentialsProvider {
            source: Some(|| Err(AwsCredentialError::ProviderFailed(MESSAGE.into()))),
        };
        let error = provider.provide_credentials().await.unwrap_err();

        assert!(matches!(error, CredentialsError::ProviderError(_)));
        assert_eq!(error.source().unwrap().to_string(), MESSAGE);
        assert!(!format!("{error:?}").contains(SECRET));
        assert!(!format!("{error:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn asks_the_host_again_when_credentials_are_requested_again() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let provider = HostCredentialsProvider {
            source: Some(|| {
                let call = CALLS.fetch_add(1, Ordering::Relaxed);
                Ok(HostCredentials {
                    access_key_id: format!("host-access-key-{call}"),
                    ..expiring_credentials()?
                })
            }),
        };

        let first = provider.provide_credentials().await.unwrap();
        let second = provider.provide_credentials().await.unwrap();
        assert_eq!(CALLS.load(Ordering::Relaxed), 2);
        assert_ne!(first.access_key_id(), second.access_key_id());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_default_reports_the_sdk_stub_error() {
        let error = HostCredentialsProvider::default()
            .provide_credentials()
            .await
            .unwrap_err();

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
