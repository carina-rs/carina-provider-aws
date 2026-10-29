use std::fmt;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry, RetryAction, RetryReason,
};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
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

/// The resend guard's verdict over every attempt of one SDK operation.
///
/// Only the resend guard can construct this type. Consumers may inspect the
/// verdict, but cannot promote a single-attempt result to an operation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationSendOutcome(SendOutcome);

impl OperationSendOutcome {
    pub(crate) fn new(outcome: SendOutcome) -> Self {
        Self(outcome)
    }

    /// Returns the all-attempts verdict.
    pub fn outcome(self) -> SendOutcome {
        self.0
    }
}

/// A dispatch failure annotated with the resend guard's all-attempts verdict.
#[derive(Debug)]
pub struct OperationDispatchFailure {
    outcome: OperationSendOutcome,
    source: BoxError,
}

impl OperationDispatchFailure {
    pub(crate) fn new(outcome: OperationSendOutcome, source: BoxError) -> Self {
        Self { outcome, source }
    }

    /// Returns the all-attempts verdict attached by the resend guard.
    pub fn outcome(&self) -> OperationSendOutcome {
        self.outcome
    }
}

impl fmt::Display for OperationDispatchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AWS operation dispatch failed with {:?} send outcome",
            self.outcome.outcome()
        )
    }
}

impl std::error::Error for OperationDispatchFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
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
    /// original request. Within one SDK `send()`, the SDK's idempotency-token
    /// interceptor fills the token once before its retry loop, so SDK-internal
    /// resends carry the same token. A provider-level resend that rebuilds the
    /// request must set the token explicitly; the AWS provider's typed retry
    /// wrapper does this.
    IdempotencyToken,
    /// The operation has none of the resend-safety traits above.
    NotIdempotent,
}

impl SendOutcome {
    /// Classifies whether an SDK error proves that the request was not applied.
    ///
    /// Apart from construction failures, a conclusive result is accepted only
    /// when [`ResendGuard`](super::ResendGuard) observed every SDK attempt and
    /// attached an [`OperationSendOutcome`]. A connector's per-attempt send
    /// phase is deliberately ignored here.
    pub fn classify<E>(error: &SdkError<E, HttpResponse>) -> SendOutcome
    where
        E: ProvideErrorKind + ProvideErrorMetadata,
    {
        match error {
            SdkError::ConstructionFailure(_) => Self::NotSent,
            SdkError::TimeoutError(_) | SdkError::ResponseError(_) => Self::MaybeSent,
            SdkError::DispatchFailure(context) => context
                .as_connector_error()
                .and_then(std::error::Error::source)
                .and_then(|source| source.downcast_ref::<OperationDispatchFailure>())
                .map(|failure| failure.outcome().outcome())
                .unwrap_or(Self::MaybeSent),
            SdkError::ServiceError(context) => context
                .raw()
                .extension::<OperationSendOutcome>()
                .map(|outcome| outcome.outcome())
                .unwrap_or(Self::MaybeSent),
            // `SdkError` is non-exhaustive. Unknown future failures must fail
            // closed because they may represent a request that reached AWS.
            _ => Self::MaybeSent,
        }
    }

    /// Classifies a type-erased failed SDK attempt from an interceptor hook.
    ///
    /// The effective configured retry classification preserves modeled
    /// throttling and timeout semantics even though the operation error is
    /// type-erased at this layer. Unknown future classifier results fail
    /// closed as [`SendOutcome::MaybeSent`].
    pub(crate) fn classify_orchestrator(
        context: &InterceptorContext,
        runtime_components: &RuntimeComponents,
    ) -> SendOutcome {
        #[cfg(any(target_arch = "wasm32", feature = "test-utils"))]
        if let Some(send_phase) = context
            .output_or_error()
            .and_then(Result::err)
            .and_then(
                aws_smithy_runtime_api::client::orchestrator::OrchestratorError::as_connector_error,
            )
            .and_then(carina_plugin_sdk::wasi_http::send_phase_of)
        {
            return match send_phase {
                carina_plugin_sdk::wasi_http::SendPhase::NotSent => Self::NotSent,
                carina_plugin_sdk::wasi_http::SendPhase::MaybeSent => Self::MaybeSent,
            };
        }

        let mut effective_kind = None;
        for classifier in runtime_components.retry_classifiers() {
            match classifier.classify_retry(context) {
                RetryAction::NoActionIndicated => {}
                RetryAction::RetryForbidden => {
                    effective_kind = None;
                    break;
                }
                RetryAction::RetryIndicated(RetryReason::RetryableError { kind, .. }) => {
                    effective_kind = Some(kind);
                }
                // `RetryReason` is non-exhaustive. A future reason cannot
                // prove that AWS rejected the request.
                RetryAction::RetryIndicated(_) => return Self::MaybeSent,
                // `RetryAction` is non-exhaustive. A future action cannot
                // prove that AWS rejected the request.
                _ => return Self::MaybeSent,
            }
        }

        match effective_kind {
            Some(ErrorKind::ThrottlingError) | Some(ErrorKind::ClientError) => Self::Rejected,
            // This covers the known transient/server kinds and fails closed for
            // future variants because `ErrorKind` is non-exhaustive.
            Some(_) => Self::MaybeSent,
            None if context
                .response()
                .is_some_and(|response| response.status().is_client_error()) =>
            {
                Self::Rejected
            }
            None => Self::MaybeSent,
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

    #[cfg(feature = "test-utils")]
    use carina_plugin_sdk::wasi_http::{SendPhase, transport_connector_error_for_test};

    use super::{Idempotency, OperationDispatchFailure, OperationSendOutcome, SendOutcome};

    #[cfg(feature = "test-utils")]
    #[derive(Debug)]
    struct WrappedConnectorError(ConnectorError);

    #[cfg(feature = "test-utils")]
    impl std::fmt::Display for WrappedConnectorError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("wrapped connector error")
        }
    }

    #[cfg(feature = "test-utils")]
    impl std::error::Error for WrappedConnectorError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

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

    fn service_error_with_outcome(
        code: &str,
        retryable: Option<ErrorKind>,
        status: u16,
        outcome: SendOutcome,
    ) -> SdkError<MockServiceError, HttpResponse> {
        let mut response = http_response(status);
        response.add_extension(OperationSendOutcome::new(outcome));
        SdkError::service_error(MockServiceError::new(code, retryable), response)
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
    fn guard_annotated_dispatch_failure_uses_operation_verdict() {
        let failure = OperationDispatchFailure::new(
            OperationSendOutcome::new(SendOutcome::NotSent),
            Box::new(io::Error::other("original connector failure")),
        );
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::io(Box::new(failure)));

        assert_eq!(SendOutcome::classify(&error), SendOutcome::NotSent);
    }

    #[cfg(feature = "test-utils")]
    #[test]
    fn raw_transport_not_sent_without_operation_verdict_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(transport_connector_error_for_test(SendPhase::NotSent));

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[cfg(feature = "test-utils")]
    #[test]
    fn wrapped_transport_not_sent_without_operation_verdict_may_have_been_sent() {
        let connector_error = ConnectorError::other(
            Box::new(WrappedConnectorError(transport_connector_error_for_test(
                SendPhase::NotSent,
            ))),
            None,
        );
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
        let error = service_error_with_outcome(
            "SomeThrottlingError",
            Some(ErrorKind::ThrottlingError),
            400,
            SendOutcome::Rejected,
        );

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn slow_down_code_with_server_error_is_rejected() {
        let error = service_error_with_outcome("SlowDown", None, 503, SendOutcome::Rejected);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn request_limit_exceeded_code_with_server_error_is_rejected() {
        let error =
            service_error_with_outcome("RequestLimitExceeded", None, 503, SendOutcome::Rejected);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn internal_error_with_server_error_may_have_been_sent() {
        let error = service_error("InternalError", None, 500);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn internal_error_with_success_status_may_have_been_sent() {
        let error = service_error("InternalError", None, 200);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn service_error_without_operation_verdict_may_have_been_sent() {
        let error = service_error("ValidationException", None, 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn s3_operation_aborted_is_rejected() {
        let error =
            service_error_with_outcome("OperationAborted", None, 409, SendOutcome::Rejected);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
    }

    #[test]
    fn s3_request_timeout_may_have_been_sent() {
        let error = service_error("RequestTimeout", None, 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn generic_request_timeout_exception_may_have_been_sent() {
        let error = service_error("RequestTimeoutException", None, 400);

        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[test]
    fn direct_and_orchestrator_classifiers_agree_on_service_failures() {
        use aws_runtime::retries::classifiers::AwsErrorCodeClassifier;
        use aws_smithy_runtime_api::client::interceptors::context::{
            Error, Input, InterceptorContext,
        };
        use aws_smithy_runtime_api::client::orchestrator::OrchestratorError;
        use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;

        let runtime_components = RuntimeComponentsBuilder::for_tests()
            .with_retry_classifier(AwsErrorCodeClassifier::<MockServiceError>::new())
            .build()
            .expect("test runtime components");
        let cases = [
            ("Throttling", Some(ErrorKind::ThrottlingError), 400),
            ("SlowDown", None, 503),
            ("ValidationException", None, 400),
            ("InternalError", Some(ErrorKind::ServerError), 500),
            ("RequestTimeout", None, 400),
            ("RequestTimeoutException", None, 400),
        ];

        for (code, retryable, status) in cases {
            let mut context = InterceptorContext::new(Input::doesnt_matter());
            context.set_response(http_response(status));
            context.set_output_or_error(Err(OrchestratorError::operation(Error::erase(
                MockServiceError::new(code, retryable),
            ))));
            let orchestrated = SendOutcome::classify_orchestrator(&context, &runtime_components);
            let direct = SendOutcome::classify(&service_error_with_outcome(
                code,
                retryable,
                status,
                orchestrated,
            ));

            assert_eq!(direct, orchestrated, "classifiers disagreed for {code}");
        }
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
