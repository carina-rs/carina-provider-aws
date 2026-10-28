use aws_runtime::retries::classifiers::THROTTLING_ERRORS;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};

/// Whether an AWS request could have reached the service before it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// The failure occurred before any request byte could reach AWS.
    NotSent,
    /// The request may have reached AWS and may have been applied.
    MaybeSent,
    /// AWS answered in a way that proves the request was not applied.
    Rejected,
}

/// An operation's resend safety as declared by its Smithy model traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idempotency {
    /// The operation has the Smithy `@readonly` trait.
    ReadOnly,
    /// The operation has the Smithy `@idempotent` trait.
    Idempotent,
    /// The input has an `@idempotencyToken` member.
    ///
    /// Resending is safe only when every resend carries the same token as the
    /// original request.
    IdempotencyToken,
    /// The operation has none of the resend-safety traits above.
    NotIdempotent,
}

impl SendOutcome {
    /// Classifies whether an SDK error proves that the request was not applied.
    ///
    /// Dispatch failures are always classified conservatively. The production
    /// transport is the `wasm32-wasip2` build, whose `carina-plugin-sdk`
    /// `wasi:http` connector maps every `wasi:http` failure—including DNS,
    /// connection refused, read timeout, and reset after send—to
    /// `ConnectorError::other(String, None)`. A connect-phase failure therefore
    /// cannot be distinguished from a post-send failure, so both are
    /// [`SendOutcome::MaybeSent`].
    pub fn classify<E>(error: &SdkError<E, HttpResponse>) -> SendOutcome
    where
        E: ProvideErrorKind + ProvideErrorMetadata,
    {
        match error {
            SdkError::ConstructionFailure(_) => Self::NotSent,
            SdkError::TimeoutError(_) | SdkError::ResponseError(_) => Self::MaybeSent,
            SdkError::DispatchFailure(_) => Self::MaybeSent,
            SdkError::ServiceError(context) => {
                let service_error = context.err();
                let is_throttling = service_error.retryable_error_kind()
                    == Some(ErrorKind::ThrottlingError)
                    || ProvideErrorMetadata::code(service_error)
                        .is_some_and(|code| THROTTLING_ERRORS.contains(&code));

                if is_throttling {
                    Self::Rejected
                } else if context.raw().status().is_server_error() {
                    Self::MaybeSent
                } else {
                    Self::Rejected
                }
            }
            // `SdkError` is non-exhaustive. Unknown future failures must fail
            // closed because they may represent a request that reached AWS.
            _ => Self::MaybeSent,
        }
    }

    /// Returns whether resending cannot duplicate an already-applied effect.
    ///
    /// This says only whether a resend is safe; deciding whether the error is
    /// worth retrying remains the provider's responsibility.
    pub fn resend_is_safe(self, idempotency: Idempotency) -> bool {
        match self {
            Self::NotSent | Self::Rejected => true,
            Self::MaybeSent => match idempotency {
                Idempotency::ReadOnly | Idempotency::Idempotent | Idempotency::IdempotencyToken => {
                    true
                }
                Idempotency::NotIdempotent => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_runtime_api::client::result::{ConnectorError, SdkError};
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};

    use super::{Idempotency, SendOutcome};

    #[derive(Debug)]
    struct MockServiceError {
        metadata: ErrorMetadata,
        retryable: Option<ErrorKind>,
    }

    impl MockServiceError {
        fn new(code: &str, retryable: Option<ErrorKind>) -> Self {
            Self {
                metadata: ErrorMetadata::builder().code(code).build(),
                retryable,
            }
        }
    }

    impl std::fmt::Display for MockServiceError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{:?}", self.metadata)
        }
    }

    impl std::error::Error for MockServiceError {}

    impl ProvideErrorKind for MockServiceError {
        fn retryable_error_kind(&self) -> Option<ErrorKind> {
            self.retryable
        }

        fn code(&self) -> Option<&str> {
            self.metadata.code()
        }
    }

    impl ProvideErrorMetadata for MockServiceError {
        fn meta(&self) -> &ErrorMetadata {
            &self.metadata
        }
    }

    fn http_response(status: u16) -> HttpResponse {
        HttpResponse::new(
            status.try_into().expect("valid HTTP status"),
            SdkBody::empty(),
        )
    }

    fn service_error(
        code: &str,
        retryable: Option<ErrorKind>,
        status: u16,
    ) -> SdkError<MockServiceError, HttpResponse> {
        SdkError::service_error(
            MockServiceError::new(code, retryable),
            http_response(status),
        )
    }

    #[test]
    fn construction_failure_was_not_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::construction_failure("request construction failed");

        assert_eq!(SendOutcome::classify(&error), SendOutcome::NotSent);
    }

    #[test]
    fn timeout_error_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::timeout_error("request timed out");

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn unclassified_dispatch_failure_may_have_been_sent() {
        let connector_error = ConnectorError::other(
            io::Error::other("unclassified connector failure").into(),
            None,
        );
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(connector_error);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn io_dispatch_failure_may_have_been_sent() {
        let connector_error = ConnectorError::io(io::Error::other("socket failure").into());
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(connector_error);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn response_error_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::response_error("invalid response", http_response(502));

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn throttling_kind_with_client_error_is_rejected() {
        let error = service_error("SomeThrottlingError", Some(ErrorKind::ThrottlingError), 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn slow_down_code_with_server_error_is_rejected() {
        let error = service_error("SlowDown", None, 503);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn request_limit_exceeded_code_with_server_error_is_rejected() {
        let error = service_error("RequestLimitExceeded", None, 503);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn internal_error_with_server_error_may_have_been_sent() {
        let error = service_error("InternalError", None, 500);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn validation_exception_is_rejected() {
        let error = service_error("ValidationException", None, 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn s3_operation_aborted_is_rejected() {
        let error = service_error("OperationAborted", None, 409);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn s3_request_timeout_is_rejected() {
        let error = service_error("RequestTimeout", None, 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn resend_safety_truth_table() {
        let cases = [
            (SendOutcome::NotSent, Idempotency::ReadOnly, true),
            (SendOutcome::NotSent, Idempotency::Idempotent, true),
            (SendOutcome::NotSent, Idempotency::IdempotencyToken, true),
            (SendOutcome::NotSent, Idempotency::NotIdempotent, true),
            (SendOutcome::MaybeSent, Idempotency::ReadOnly, true),
            (SendOutcome::MaybeSent, Idempotency::Idempotent, true),
            (SendOutcome::MaybeSent, Idempotency::IdempotencyToken, true),
            (SendOutcome::MaybeSent, Idempotency::NotIdempotent, false),
            (SendOutcome::Rejected, Idempotency::ReadOnly, true),
            (SendOutcome::Rejected, Idempotency::Idempotent, true),
            (SendOutcome::Rejected, Idempotency::IdempotencyToken, true),
            (SendOutcome::Rejected, Idempotency::NotIdempotent, true),
        ];

        for (outcome, idempotency, expected) in cases {
            assert_eq!(
                outcome.resend_is_safe(idempotency),
                expected,
                "unexpected resend safety for {outcome:?} and {idempotency:?}"
            );
        }
    }
}
