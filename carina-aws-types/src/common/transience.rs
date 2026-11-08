use std::error::Error;

use aws_runtime::retries::classifiers::{THROTTLING_ERRORS, TRANSIENT_ERRORS};
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};
use carina_core::provider::ProviderError;
use carina_core::provider_text::Retryability;

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

    /// Read transience captured before provider diagnostic text is made opaque.
    /// Unclassified validation, conversion, and other local errors are permanent.
    pub fn of_provider_error(error: &ProviderError) -> Self {
        match error.retryability() {
            Retryability::Transient => Self::Transient,
            Retryability::Unclassified => Self::Permanent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;
    use std::fmt;

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
            let cause = AwsCause(sdk);
            assert_eq!(cause.to_string(), "AWS read failed");
            assert!(
                cause
                    .source()
                    .unwrap()
                    .is::<SdkError<ErrorMetadata, HttpResponse>>()
            );
            let provider = ProviderError::api_error_caused_by("read failed", cause)
                .with_retryability(match transience {
                    Transience::Transient => Retryability::Transient,
                    Transience::Permanent => Retryability::Unclassified,
                });
            let nested = ProviderError::api_error_caused_by("read-back failed", provider)
                .with_retryability(match transience {
                    Transience::Transient => Retryability::Transient,
                    Transience::Permanent => Retryability::Unclassified,
                });
            assert_eq!(Transience::of_provider_error(&nested), transience);
            let nested =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(
                    nested,
                );
            assert!(nested.to_string().contains("original SDK timeout"));
        }
        let unclassified = ProviderError::api_error("throttling timeout connection reset");
        assert_eq!(
            Transience::of_provider_error(&unclassified),
            Transience::Permanent
        );
    }

    #[test]
    fn transient_aws_error_is_classified_from_structured_detail() {
        let sdk = SdkError::service_error(
            ErrorMetadata::builder().code("RequestTimeout").build(),
            response(503),
        );
        let transience = Transience::of(&sdk);
        let error = ProviderError::api_error_caused_by("AWS read failed", AwsCause(sdk))
            .with_status(503)
            .with_retryability(match transience {
                Transience::Transient => Retryability::Transient,
                Transience::Permanent => Retryability::Unclassified,
            });

        assert!(error.is_transient());
        assert_eq!(error.detail().status, Some(503));
        assert!(error.detail().cause().unwrap().is::<AwsCause>());
        assert_eq!(Transience::of_provider_error(&error), Transience::Transient);
    }
}
