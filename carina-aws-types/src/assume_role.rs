//! AssumeRole credentials loaded exclusively through a guarded STS client.

use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_credential_types::Credentials;
use aws_credential_types::credential_feature::AwsCredentialFeature;
use aws_credential_types::provider::{self, ProvideCredentials, error::CredentialsError, future};
use aws_smithy_runtime::client::identity::IdentityCache;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::common::ServiceErrorCode;
use crate::sdk::sts::StsClient;
use crate::sdk::sts::error::SdkError;
use crate::sdk::sts::operation::assume_role::{
    AssumeRoleError, AssumeRoleOutput, builders::AssumeRoleFluentBuilder,
};
use crate::{Idempotency, SendClassification};
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};

type PendingAssumeRole = Shared<BoxFuture<'static, Result<AssumeRoleOutput, AssumeRoleFailure>>>;

#[derive(Clone, Copy, Debug)]
enum SdkFailureKind {
    ConstructionFailure,
    TimeoutError,
    DispatchFailure,
    ResponseError,
    ServiceError,
    Unknown,
}

/// An AssumeRole failure containing only its resend verdict and safe diagnostics.
///
/// The SDK error, raw response, service message and source chain are discarded:
/// even an unparseable success response can contain temporary credentials.
#[derive(Clone)]
pub struct AssumeRoleFailure {
    classification: SendClassification,
    kind: SdkFailureKind,
    http_status: Option<u16>,
    service_error_code: Option<String>,
    invalid_configuration: bool,
}

impl AssumeRoleFailure {
    fn from_sdk_error(error: SdkError<AssumeRoleError>) -> Self {
        let kind = if matches!(error, SdkError::ConstructionFailure(..)) {
            SdkFailureKind::ConstructionFailure
        } else if matches!(error, SdkError::TimeoutError(..)) {
            SdkFailureKind::TimeoutError
        } else if matches!(error, SdkError::DispatchFailure(..)) {
            SdkFailureKind::DispatchFailure
        } else if matches!(error, SdkError::ResponseError(..)) {
            SdkFailureKind::ResponseError
        } else if matches!(error, SdkError::ServiceError(..)) {
            SdkFailureKind::ServiceError
        } else {
            // SdkError is non-exhaustive; future variants get no free-form text.
            SdkFailureKind::Unknown
        };
        let service = error.as_service_error();
        Self {
            classification: SendClassification::of(&error),
            kind,
            http_status: error
                .raw_response()
                .map(|response| response.status().as_u16()),
            service_error_code: service
                .and_then(ProvideErrorMetadata::code)
                .and_then(ServiceErrorCode::parse)
                .map(|code| code.as_str().to_owned()),
            invalid_configuration: service.is_some_and(|service| {
                service.is_region_disabled_exception()
                    || service.is_malformed_policy_document_exception()
            }),
        }
    }

    fn into_credentials_error(self) -> CredentialsError {
        if self.invalid_configuration {
            CredentialsError::invalid_configuration(self)
        } else {
            CredentialsError::provider_error(self)
        }
    }

    /// Whether this STS request may already have issued credentials.
    pub fn classification(&self) -> SendClassification {
        self.classification
    }
}

impl fmt::Debug for AssumeRoleFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssumeRoleFailure")
            .field("classification", &self.classification)
            .field("kind", &self.kind)
            .field("http_status", &self.http_status)
            .field("service_error_code", &self.service_error_code)
            .finish()
    }
}

impl fmt::Display for AssumeRoleFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssumeRole {:?}", self.kind)?;
        if let Some(status) = self.http_status {
            write!(f, " HTTP {status}")?;
        }
        if let Some(code) = &self.service_error_code {
            write!(f, " {code}")?;
        }
        write!(f, " with {:?} send classification", self.classification)
    }
}

impl std::error::Error for AssumeRoleFailure {}

/// Obtains temporary role credentials using the configuration's base credentials.
///
/// Like `aws_config::sts::AssumeRoleProvider`, this provider does not cache its
/// completed results. Concurrent loads share one in-flight STS request, retained
/// even if a caller's attempt or identity-load timeout drops its future. The
/// consuming SDK clients share their identity cache and refresh
/// these credentials using the expiration returned by STS. Base credentials are
/// resolved on each STS call, so host credential refreshes remain visible.
#[derive(Debug)]
pub struct GuardedAssumeRoleProvider {
    request: AssumeRoleFluentBuilder,
    duration_seconds: Result<Option<i32>, std::num::TryFromIntError>,
    in_flight: Mutex<Option<PendingAssumeRole>>,
}

impl GuardedAssumeRoleProvider {
    /// Retain the base credentials, region, transport, retry and timeout policy.
    /// An omitted session name uses the SDK's timestamp-based naming convention.
    pub fn new(
        config: &aws_types::SdkConfig,
        role_arn: String,
        session_name: Option<String>,
        external_id: Option<String>,
        session_length: Option<Duration>,
    ) -> Self {
        let session_name = session_name.unwrap_or_else(|| {
            let now = config
                .time_source()
                .map(|source| source.now())
                .unwrap_or_else(SystemTime::now);
            format!(
                "assume-role-provider-{}",
                now.duration_since(UNIX_EPOCH)
                    .expect("post epoch")
                    .as_millis()
            )
        });
        // Match aws-config's AssumeRole provider: do not cache base identities
        // inside STS. The consuming service caches the resulting role identity.
        let config = config
            .to_builder()
            .identity_cache(IdentityCache::no_cache())
            .build();
        let client = StsClient::new(&config, sts_idempotency);
        let duration_seconds = session_length
            .map(|duration| i32::try_from(duration.as_secs()))
            .transpose();
        Self {
            request: client
                .inner()
                .assume_role()
                .role_arn(role_arn)
                .role_session_name(session_name)
                .set_external_id(external_id)
                .set_duration_seconds(duration_seconds.as_ref().ok().copied().flatten()),
            duration_seconds,
            in_flight: Mutex::new(None),
        }
    }

    async fn credentials(&self) -> provider::Result {
        self.duration_seconds
            .as_ref()
            .map_err(|error| CredentialsError::invalid_configuration(*error))?;
        let pending = {
            let mut slot = self.in_flight.lock().expect("AssumeRole in-flight lock");
            // A completed request is never a credentials cache. Another waiter
            // may have completed it before reaching the cleanup below.
            if slot
                .as_ref()
                .is_some_and(|pending| pending.peek().is_some())
            {
                *slot = None;
            }
            slot.get_or_insert_with(|| {
                let request = self.request.clone();
                // Sanitize before Shared retains the result for other waiters.
                async move {
                    request
                        .send()
                        .await
                        .map_err(AssumeRoleFailure::from_sdk_error)
                }
                .boxed()
                .shared()
            })
            .clone()
        };
        // The slot owns a clone, so dropping this caller cannot cancel a request
        // which may already have reached STS. No executor or background task is
        // needed: the next waiter polls that same future.
        let result = pending.clone().await;
        {
            let mut slot = self.in_flight.lock().expect("AssumeRole in-flight lock");
            // A late waiter must not clear a newer request's slot.
            if slot
                .as_ref()
                .is_some_and(|current| current.ptr_eq(&pending))
            {
                *slot = None;
            }
        }
        let output = result.map_err(AssumeRoleFailure::into_credentials_error)?;
        let credentials = output
            .credentials
            .ok_or_else(|| CredentialsError::unhandled("STS credentials must be defined"))?;
        let expiry = SystemTime::try_from(credentials.expiration).map_err(|error| {
            CredentialsError::unhandled(format!(
                "credential expiration time cannot be represented by a SystemTime: {error}"
            ))
        })?;
        let mut builder = Credentials::builder()
            .access_key_id(credentials.access_key_id)
            .secret_access_key(credentials.secret_access_key)
            .session_token(credentials.session_token)
            .expiry(expiry)
            .provider_name("AssumeRoleProvider");
        if let Some(user) = output.assumed_role_user {
            let account = user.arn.split(':').nth(4).ok_or_else(|| {
                CredentialsError::unhandled("ARN must have 6 components delimited by `:`")
            })?;
            builder.set_account_id(Some(account.into()));
        }
        let mut credentials = builder.build();
        credentials
            .get_property_mut_or_default::<Vec<AwsCredentialFeature>>()
            .push(AwsCredentialFeature::CredentialsStsAssumeRole);
        Ok(credentials)
    }
}

// This client only issues AssumeRole, which has no idempotency trait or token.
fn sts_idempotency(_: &str) -> Idempotency {
    Idempotency::NotIdempotent
}

impl ProvideCredentials for GuardedAssumeRoleProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }
}

#[cfg(test)]
mod tests;
