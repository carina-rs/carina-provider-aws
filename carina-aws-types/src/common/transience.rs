use std::error::Error;
use std::fmt;

use aws_runtime::retries::classifiers::{THROTTLING_ERRORS, TRANSIENT_ERRORS};
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};
use carina_core::provider::ProviderError;

use super::send_outcome::connector_transience;
use super::{FinalAttempt, SendClassification, Transience};

// S3's Smithy model does not mark these documented transient errors retryable.
// Keep the provider's existing carve-out alongside the SDK classifier.
const S3_RETRYABLE_ERROR_CODES: &[&str] = &["RequestTimeout", "SlowDown", "OperationAborted"];

impl Transience {
    /// Classify an SDK failure independently of mutation resend safety.
    ///
    /// Guard-proven final-attempt transience takes precedence. Otherwise use
    /// modeled retry kinds, the SDK's AWS error-code sets, HTTP status, and the
    /// shared transport classifier. Response decoding failures are permanent.
    pub fn of<E>(error: &SdkError<E, HttpResponse>) -> Self
    where
        E: Error + ProvideErrorMetadata + ProvideErrorKind + 'static,
    {
        match SendClassification::of(error) {
            SendClassification::NotSent(transience)
            | SendClassification::MaybeSent {
                final_attempt: FinalAttempt::NotSent(transience),
            } => return transience,
            SendClassification::Rejected
            | SendClassification::MaybeSent {
                final_attempt: FinalAttempt::Sent,
            } => {}
        }
        match error {
            SdkError::TimeoutError(_) => Self::Transient,
            SdkError::DispatchFailure(context) => context
                .as_connector_error()
                .map(connector_transience)
                .unwrap_or(Self::Permanent),
            SdkError::ServiceError(context) => {
                let kind = context.err().retryable_error_kind();
                let code = ProvideErrorMetadata::code(context.err());
                let transient_code = code.is_some_and(|code| {
                    THROTTLING_ERRORS.contains(&code)
                        || TRANSIENT_ERRORS.contains(&code)
                        || S3_RETRYABLE_ERROR_CODES.contains(&code)
                });
                if matches!(
                    kind,
                    Some(
                        ErrorKind::TransientError
                            | ErrorKind::ThrottlingError
                            | ErrorKind::ServerError
                    )
                ) || transient_code
                    || context.raw().status().is_server_error()
                    || context.raw().status().as_u16() == 429
                {
                    Self::Transient
                } else {
                    Self::Permanent
                }
            }
            _ => Self::Permanent,
        }
    }

    /// Retain the classification when a typed diagnostic wraps the SDK error.
    /// Its display and source chain stay the same as the supplied cause.
    pub fn caused_by(self, cause: impl Error + Send + Sync + 'static) -> impl Error + Send + Sync {
        ClassifiedCause {
            transience: self,
            cause: Box::new(cause),
        }
    }

    /// Recover typed transience from a provider diagnostic, including nesting.
    /// Unclassified validation, conversion, and other local errors are permanent.
    pub fn of_provider_error(error: &ProviderError) -> Self {
        let mut cause: Option<&(dyn Error + 'static)> = Some(error);
        while let Some(error) = cause {
            if let Some(classified) = error.downcast_ref::<ClassifiedCause>() {
                return classified.transience;
            }
            // ProviderError owns its diagnostic cause without exposing Error::source.
            cause = match error.downcast_ref::<ProviderError>() {
                Some(provider) => provider.detail().cause().map(|cause| cause as &dyn Error),
                None => error.source(),
            };
        }
        Self::Permanent
    }
}

#[derive(Debug)]
struct ClassifiedCause {
    transience: Transience,
    cause: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for ClassifiedCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}

impl Error for ClassifiedCause {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    fn response(status: u16) -> HttpResponse {
        HttpResponse::new(status.try_into().unwrap(), SdkBody::empty())
    }

    #[test]
    fn sdk_transience_uses_aws_codes_and_http_status() {
        for (status, code, expected) in [
            (403, "AccessDenied", Transience::Permanent),
            (400, "ValidationException", Transience::Permanent),
            (400, "ThrottlingException", Transience::Transient),
            (400, "RequestTimeout", Transience::Transient),
            (409, "OperationAborted", Transience::Transient),
            (429, "Unmodeled", Transience::Transient),
            (500, "Unmodeled", Transience::Transient),
            (503, "Unmodeled", Transience::Transient),
        ] {
            let error = SdkError::service_error(
                ErrorMetadata::builder().code(code).build(),
                response(status),
            );
            assert_eq!(Transience::of(&error), expected, "{status} {code}");
        }
    }

    #[test]
    fn sdk_transience_uses_shared_connector_classification() {
        for (connector, expected) in [
            (
                ConnectorError::io("connection reset".into()),
                Transience::Transient,
            ),
            (
                ConnectorError::timeout("read timed out".into()),
                Transience::Transient,
            ),
            (
                ConnectorError::other("temporary".into(), Some(ErrorKind::TransientError)),
                Transience::Transient,
            ),
            (
                ConnectorError::other("timeout text alone".into(), None),
                Transience::Permanent,
            ),
            (
                ConnectorError::user("invalid endpoint".into()),
                Transience::Permanent,
            ),
        ] {
            let error: SdkError<ErrorMetadata, HttpResponse> =
                SdkError::dispatch_failure(connector);
            assert_eq!(Transience::of(&error), expected, "{error:?}");
        }
        let timeout: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::timeout_error("operation timed out");
        assert_eq!(Transience::of(&timeout), Transience::Transient);
    }

    #[test]
    fn sdk_transience_rejects_construction_and_response_decode_errors() {
        let construction: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::construction_failure("invalid request");
        let decode: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::response_error("invalid response", response(503));
        assert_eq!(Transience::of(&construction), Transience::Permanent);
        assert_eq!(Transience::of(&decode), Transience::Permanent);
    }

    #[derive(Debug)]
    struct AwsCause(SdkError<ErrorMetadata, HttpResponse>);

    impl fmt::Display for AwsCause {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("AWS read failed")
        }
    }

    impl Error for AwsCause {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn provider_transience_survives_nested_diagnostics_without_changing_sources() {
        for transience in [Transience::Transient, Transience::Permanent] {
            let sdk = SdkError::timeout_error("original SDK timeout");
            let cause = transience.caused_by(AwsCause(sdk));
            assert_eq!(cause.to_string(), "AWS read failed");
            assert!(
                cause
                    .source()
                    .unwrap()
                    .is::<SdkError<ErrorMetadata, HttpResponse>>()
            );
            let provider = ProviderError::api_error_caused_by("read failed", cause);
            let nested = ProviderError::api_error_caused_by("read-back failed", provider);
            assert_eq!(Transience::of_provider_error(&nested), transience);
            assert!(nested.to_string().contains("original SDK timeout"));
        }
        let unclassified = ProviderError::api_error("throttling timeout connection reset");
        assert_eq!(
            Transience::of_provider_error(&unclassified),
            Transience::Permanent
        );
    }
}
