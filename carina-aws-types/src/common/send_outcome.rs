use std::fmt;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry, RetryAction, RetryReason,
};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::retry::ErrorKind;

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

/// Whether the final attempt failed before transport, and whether that failure may clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalAttempt {
    /// The final attempt did not reach transport.
    ///
    /// A pre-transmit [`SdkError::TimeoutError`] is intentionally transient:
    /// the configured operation-attempt timeout bounds each attempt.
    /// Identity-cache load timeouts instead surface as an unclassified
    /// connector failure and are permanent here.
    NotSent(Transience),
    /// The final attempt reached transport or its phase is unknown.
    Sent,
}

/// Whether retrying a pre-transport failure may succeed without configuration changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transience {
    /// The failure may clear on a later attempt.
    Transient,
    /// Repeating the same operation cannot be expected to clear the failure.
    Permanent,
}

/// Resend safety across the operation and transience of its final attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendClassification {
    operation_outcome: SendOutcome,
    final_attempt: FinalAttempt,
}

impl SendClassification {
    /// Classifies an SDK failure once for all downstream resend decisions.
    ///
    /// The operation outcome aggregates every attempt recorded by
    /// [`ResendGuard`](super::ResendGuard). The final-attempt verdict reports
    /// whether the last attempt reached transport and, for a proven pre-send
    /// failure, whether its typed transport cause is transient. Unannotated
    /// failures fail closed, except construction failures, which occur before
    /// an attempt. Response parsing failures are always treated as sent.
    pub fn of<E>(error: &SdkError<E, HttpResponse>) -> Self
    where
        E: std::error::Error + 'static,
    {
        let verdict = operation_verdict(error);
        let final_attempt = if verdict.final_attempt_outcome() == SendOutcome::NotSent {
            FinalAttempt::NotSent(not_sent_transience(error))
        } else {
            FinalAttempt::Sent
        };
        Self {
            operation_outcome: verdict.operation_outcome(),
            final_attempt,
        }
    }

    /// Returns the resend-safety outcome aggregated across every SDK attempt.
    pub fn operation_outcome(self) -> SendOutcome {
        self.operation_outcome
    }

    /// Returns the typed transport verdict for the final SDK attempt.
    pub fn final_attempt(self) -> FinalAttempt {
        self.final_attempt
    }

    /// Returns whether resending cannot duplicate an already-applied effect.
    ///
    /// This says only whether a resend is safe; deciding whether the final
    /// error is worth retrying remains the provider's responsibility.
    pub fn resend_is_safe(self, idempotency: Idempotency) -> bool {
        self.operation_outcome.resend_is_safe(idempotency)
    }

    /// Records that an earlier provider-level attempt may have been sent.
    ///
    /// The final-attempt verdict remains unchanged so retry transience is
    /// still decided from the most recent attempt.
    pub fn with_prior_maybe_sent(mut self) -> Self {
        self.operation_outcome = SendOutcome::MaybeSent;
        self
    }

    /// Constructs a classification for provider retry tests.
    #[cfg(feature = "test-utils")]
    pub fn for_test(operation_outcome: SendOutcome, final_attempt: FinalAttempt) -> Self {
        Self {
            operation_outcome,
            final_attempt,
        }
    }
}

/// The raw resend-guard verdict retained in the SDK error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperationVerdict {
    operation_outcome: SendOutcome,
    final_attempt_outcome: SendOutcome,
}

impl OperationVerdict {
    pub(crate) fn new(operation_outcome: SendOutcome, final_attempt_outcome: SendOutcome) -> Self {
        Self {
            operation_outcome,
            final_attempt_outcome,
        }
    }

    pub(crate) fn operation_outcome(self) -> SendOutcome {
        self.operation_outcome
    }

    pub(crate) fn final_attempt_outcome(self) -> SendOutcome {
        self.final_attempt_outcome
    }
}

/// The furthest point one SDK attempt reached in the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptPhase {
    /// The attempt has not reached the hook immediately before transmission.
    BeforeTransmit,
    /// The attempt reached `read_before_transmit` and may have entered the connector.
    TransmitReached,
}

/// Attaches a single-attempt resend-guard verdict to a mock response.
///
/// This is test-only. Production verdicts come only from [`ResendGuard`](super::ResendGuard).
#[cfg(feature = "test-utils")]
pub fn attach_single_attempt_verdict_for_test(response: &mut HttpResponse, outcome: SendOutcome) {
    response.add_extension(OperationVerdict::new(outcome, outcome));
}

/// An SDK failure annotated with the resend guard's all-attempts verdict.
#[derive(Debug)]
pub(crate) struct OperationSendFailure {
    verdict: OperationVerdict,
    source: BoxError,
}

impl OperationSendFailure {
    pub(crate) fn new(verdict: OperationVerdict, source: BoxError) -> Self {
        Self { verdict, source }
    }

    pub(crate) fn verdict(&self) -> OperationVerdict {
        self.verdict
    }
}

impl fmt::Display for OperationSendFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AWS operation failed with {:?} send outcome",
            self.verdict.operation_outcome()
        )
    }
}

impl std::error::Error for OperationSendFailure {
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
    /// Classifies a type-erased failed SDK attempt from an interceptor hook.
    ///
    /// [`AttemptPhase::BeforeTransmit`] short-circuits to [`SendOutcome::NotSent`].
    ///
    /// For attempts that reached transmit, the effective configured retry
    /// classification preserves modeled throttling and timeout semantics even
    /// though the operation error is type-erased at this layer. Unknown future
    /// classifier results fail closed as [`SendOutcome::MaybeSent`].
    pub(crate) fn classify_orchestrator(
        phase: AttemptPhase,
        context: &InterceptorContext,
        runtime_components: &RuntimeComponents,
    ) -> SendOutcome {
        if phase == AttemptPhase::BeforeTransmit {
            return Self::NotSent;
        }

        #[cfg(any(target_arch = "wasm32", feature = "test-utils", test))]
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

    pub(crate) fn resend_is_safe(self, idempotency: Idempotency) -> bool {
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

fn operation_verdict<E>(error: &SdkError<E, HttpResponse>) -> OperationVerdict
where
    E: std::error::Error + 'static,
{
    let maybe_sent = OperationVerdict::new(SendOutcome::MaybeSent, SendOutcome::MaybeSent);
    match error {
        // Response parsing proves transport was reached but not whether AWS
        // applied the request, so guard annotations never change this case.
        SdkError::ResponseError(_) => maybe_sent,
        SdkError::ServiceError(context) => context
            .raw()
            .extension::<OperationVerdict>()
            .copied()
            .unwrap_or(maybe_sent),
        SdkError::ConstructionFailure(_) => operation_verdict_at(std::error::Error::source(error))
            .unwrap_or_else(|| OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent)),
        SdkError::TimeoutError(_) => {
            operation_verdict_at(std::error::Error::source(error)).unwrap_or(maybe_sent)
        }
        SdkError::DispatchFailure(context) => context
            .as_connector_error()
            .and_then(std::error::Error::source)
            .and_then(|source| source.downcast_ref::<OperationSendFailure>())
            .map(OperationSendFailure::verdict)
            .unwrap_or(maybe_sent),
        // `SdkError` is non-exhaustive. Unknown future failures must fail
        // closed because they may represent a request that reached AWS.
        _ => maybe_sent,
    }
}

/// Applies the pre-transport retry policy documented on [`FinalAttempt::NotSent`].
fn not_sent_transience<E>(error: &SdkError<E, HttpResponse>) -> Transience {
    match error {
        SdkError::TimeoutError(_) => Transience::Transient,
        SdkError::DispatchFailure(context) => context
            .as_connector_error()
            .map(connector_transience)
            .unwrap_or(Transience::Permanent),
        _ => Transience::Permanent,
    }
}

fn connector_transience(
    error: &aws_smithy_runtime_api::client::result::ConnectorError,
) -> Transience {
    if error.is_timeout() {
        return Transience::Transient;
    }
    if error.is_io() {
        #[cfg(any(target_arch = "wasm32", feature = "test-utils", test))]
        if let Some(failure) = transport_failure_in_source_chain(error) {
            return transport_failure_transience(failure);
        }
        return Transience::Transient;
    }
    if matches!(
        error.as_other(),
        Some(ErrorKind::TransientError | ErrorKind::ThrottlingError | ErrorKind::ServerError)
    ) {
        Transience::Transient
    } else {
        Transience::Permanent
    }
}

#[cfg(any(target_arch = "wasm32", feature = "test-utils", test))]
fn transport_failure_in_source_chain(
    error: &aws_smithy_runtime_api::client::result::ConnectorError,
) -> Option<&carina_plugin_sdk::wasi_http::TransportFailure> {
    let mut source = std::error::Error::source(error);
    while let Some(error) = source {
        if let Some(failure) =
            error.downcast_ref::<carina_plugin_sdk::wasi_http::TransportFailure>()
        {
            return Some(failure);
        }
        source = error.source();
    }
    None
}

#[cfg(any(target_arch = "wasm32", feature = "test-utils", test))]
fn transport_failure_transience(
    failure: &carina_plugin_sdk::wasi_http::TransportFailure,
) -> Transience {
    let Some(code) = failure.code() else {
        return Transience::Permanent;
    };

    wasi_error_code_transience(code)
}

#[cfg(any(target_arch = "wasm32", feature = "test-utils", test))]
fn wasi_error_code_transience(code: &wasi::http::types::ErrorCode) -> Transience {
    use wasi::http::types::ErrorCode;

    match code {
        ErrorCode::DnsTimeout
        | ErrorCode::DestinationNotFound
        | ErrorCode::DestinationUnavailable
        | ErrorCode::DestinationIpUnroutable
        | ErrorCode::ConnectionRefused
        | ErrorCode::ConnectionTimeout => Transience::Transient,
        ErrorCode::DnsError(_payload) => Transience::Transient,
        ErrorCode::DestinationIpProhibited
        | ErrorCode::ConnectionTerminated
        | ErrorCode::ConnectionReadTimeout
        | ErrorCode::ConnectionWriteTimeout
        | ErrorCode::ConnectionLimitReached
        | ErrorCode::TlsProtocolError
        | ErrorCode::TlsCertificateError
        | ErrorCode::HttpRequestDenied
        | ErrorCode::HttpRequestLengthRequired
        | ErrorCode::HttpRequestMethodInvalid
        | ErrorCode::HttpRequestUriInvalid
        | ErrorCode::HttpRequestUriTooLong
        | ErrorCode::HttpResponseIncomplete
        | ErrorCode::HttpResponseTimeout
        | ErrorCode::HttpUpgradeFailed
        | ErrorCode::HttpProtocolError
        | ErrorCode::LoopDetected
        | ErrorCode::ConfigurationError => Transience::Permanent,
        ErrorCode::TlsAlertReceived(_payload) => Transience::Permanent,
        ErrorCode::HttpRequestBodySize(_size) => Transience::Permanent,
        ErrorCode::HttpRequestHeaderSectionSize(_size) => Transience::Permanent,
        ErrorCode::HttpRequestHeaderSize(_size) => Transience::Permanent,
        ErrorCode::HttpRequestTrailerSectionSize(_size) => Transience::Permanent,
        ErrorCode::HttpRequestTrailerSize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseHeaderSectionSize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseHeaderSize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseBodySize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseTrailerSectionSize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseTrailerSize(_size) => Transience::Permanent,
        ErrorCode::HttpResponseTransferCoding(_coding) => Transience::Permanent,
        ErrorCode::HttpResponseContentCoding(_coding) => Transience::Permanent,
        ErrorCode::InternalError(_message) => Transience::Permanent,
    }
}

fn operation_verdict_at(
    source: Option<&(dyn std::error::Error + 'static)>,
) -> Option<OperationVerdict> {
    source
        .and_then(|source| source.downcast_ref::<OperationSendFailure>())
        .map(OperationSendFailure::verdict)
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

    use carina_plugin_sdk::wasi_http::{SendPhase, transport_connector_error_for_test};
    use wasi::http::types::{
        DnsErrorPayload, ErrorCode, FieldSizePayload, TlsAlertReceivedPayload,
    };

    use super::{
        AttemptPhase, FinalAttempt, Idempotency, OperationSendFailure, OperationVerdict,
        SendClassification, SendOutcome, Transience, wasi_error_code_transience,
    };

    #[derive(Debug)]
    struct WrappedConnectorError(ConnectorError);

    impl std::fmt::Display for WrappedConnectorError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("wrapped connector error")
        }
    }

    impl std::error::Error for WrappedConnectorError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[derive(Debug)]
    struct NestedOperationFailure(OperationSendFailure);

    impl std::fmt::Display for NestedOperationFailure {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("outer SDK call failed")
        }
    }

    impl std::error::Error for NestedOperationFailure {
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
        response.add_extension(OperationVerdict::new(outcome, outcome));
        SdkError::service_error(MockServiceError::new(code, retryable), response)
    }

    #[test]
    fn typed_classification_carries_final_attempt_transience() {
        let construction: SdkError<MockServiceError, HttpResponse> =
            SdkError::construction_failure("request construction failed");
        let classification = SendClassification::of(&construction);
        assert_eq!(classification.operation_outcome(), SendOutcome::NotSent);
        assert_eq!(
            classification.final_attempt(),
            FinalAttempt::NotSent(Transience::Permanent)
        );

        let timeout_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("identity resolution timed out")),
        );
        let timeout: SdkError<MockServiceError, HttpResponse> =
            SdkError::timeout_error(timeout_failure);
        assert_eq!(
            SendClassification::of(&timeout).final_attempt(),
            FinalAttempt::NotSent(Transience::Transient)
        );

        let native_io_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("connection refused")),
        );
        let native_io: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::io(Box::new(native_io_failure)));
        assert_eq!(
            SendClassification::of(&native_io).final_attempt(),
            FinalAttempt::NotSent(Transience::Transient)
        );

        let other_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("credentials not loaded")),
        );
        let other: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::other(Box::new(other_failure), None));
        let user_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("invalid local request")),
        );
        let user: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::user(Box::new(user_failure)));
        for error in [&other, &user] {
            assert_eq!(
                SendClassification::of(error).final_attempt(),
                FinalAttempt::NotSent(Transience::Permanent)
            );
        }

        let wasi_pre_send_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(transport_connector_error_for_test(SendPhase::NotSent)),
        );
        let wasi_pre_send: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::io(Box::new(wasi_pre_send_failure)));
        assert_eq!(
            SendClassification::of(&wasi_pre_send).final_attempt(),
            FinalAttempt::NotSent(Transience::Permanent)
        );

        for kind in [
            ErrorKind::TransientError,
            ErrorKind::ThrottlingError,
            ErrorKind::ServerError,
        ] {
            let failure = OperationSendFailure::new(
                OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
                Box::new(io::Error::other("classified connector failure")),
            );
            let error: SdkError<MockServiceError, HttpResponse> =
                SdkError::dispatch_failure(ConnectorError::other(Box::new(failure), Some(kind)));
            assert_eq!(
                SendClassification::of(&error).final_attempt(),
                FinalAttempt::NotSent(Transience::Transient),
                "{kind:?}"
            );
        }

        let client_failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("classified client failure")),
        );
        let client: SdkError<MockServiceError, HttpResponse> = SdkError::dispatch_failure(
            ConnectorError::other(Box::new(client_failure), Some(ErrorKind::ClientError)),
        );
        assert_eq!(
            SendClassification::of(&client).final_attempt(),
            FinalAttempt::NotSent(Transience::Permanent)
        );
    }

    #[test]
    fn wasi_transient_error_codes_are_transient() {
        let codes = [
            ErrorCode::DnsTimeout,
            ErrorCode::DnsError(DnsErrorPayload {
                rcode: None,
                info_code: None,
            }),
            ErrorCode::DestinationNotFound,
            ErrorCode::DestinationUnavailable,
            ErrorCode::DestinationIpUnroutable,
            ErrorCode::ConnectionRefused,
            ErrorCode::ConnectionTimeout,
        ];

        for code in codes {
            assert_eq!(
                wasi_error_code_transience(&code),
                Transience::Transient,
                "{code:?}"
            );
        }
    }

    #[test]
    fn wasi_permanent_error_codes_are_permanent() {
        let field_size = || FieldSizePayload {
            field_name: None,
            field_size: None,
        };
        let codes = [
            ErrorCode::DestinationIpProhibited,
            ErrorCode::ConnectionTerminated,
            ErrorCode::ConnectionReadTimeout,
            ErrorCode::ConnectionWriteTimeout,
            ErrorCode::ConnectionLimitReached,
            ErrorCode::TlsProtocolError,
            ErrorCode::TlsCertificateError,
            ErrorCode::TlsAlertReceived(TlsAlertReceivedPayload {
                alert_id: None,
                alert_message: None,
            }),
            ErrorCode::HttpRequestDenied,
            ErrorCode::HttpRequestLengthRequired,
            ErrorCode::HttpRequestBodySize(None),
            ErrorCode::HttpRequestMethodInvalid,
            ErrorCode::HttpRequestUriInvalid,
            ErrorCode::HttpRequestUriTooLong,
            ErrorCode::HttpRequestHeaderSectionSize(None),
            ErrorCode::HttpRequestHeaderSize(None),
            ErrorCode::HttpRequestTrailerSectionSize(None),
            ErrorCode::HttpRequestTrailerSize(field_size()),
            ErrorCode::HttpResponseIncomplete,
            ErrorCode::HttpResponseHeaderSectionSize(None),
            ErrorCode::HttpResponseHeaderSize(field_size()),
            ErrorCode::HttpResponseBodySize(None),
            ErrorCode::HttpResponseTrailerSectionSize(None),
            ErrorCode::HttpResponseTrailerSize(field_size()),
            ErrorCode::HttpResponseTransferCoding(None),
            ErrorCode::HttpResponseContentCoding(None),
            ErrorCode::HttpResponseTimeout,
            ErrorCode::HttpUpgradeFailed,
            ErrorCode::HttpProtocolError,
            ErrorCode::LoopDetected,
            ErrorCode::ConfigurationError,
            ErrorCode::InternalError(None),
        ];

        for code in codes {
            assert_eq!(
                wasi_error_code_transience(&code),
                Transience::Permanent,
                "{code:?}"
            );
        }
    }

    #[test]
    fn construction_failure_was_not_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::construction_failure("request construction failed");

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::NotSent
        );
    }

    #[test]
    fn timeout_error_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::timeout_error("request timed out");

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn unclassified_dispatch_failure_may_have_been_sent() {
        let connector_error = ConnectorError::other(
            io::Error::other("unclassified connector failure").into(),
            None,
        );
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(connector_error);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn io_dispatch_failure_may_have_been_sent() {
        let connector_error = ConnectorError::io(io::Error::other("socket failure").into());
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(connector_error);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn guard_annotated_dispatch_failure_uses_operation_verdict() {
        let failure = OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("original connector failure")),
        );
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(ConnectorError::io(Box::new(failure)));

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::NotSent
        );
    }

    #[test]
    fn raw_transport_not_sent_without_operation_verdict_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::dispatch_failure(transport_connector_error_for_test(SendPhase::NotSent));

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

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

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn response_error_may_have_been_sent() {
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::response_error("invalid response", http_response(502));

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn response_error_ignores_guard_verdict() {
        let mut response = http_response(400);
        response.add_extension(OperationVerdict::new(
            SendOutcome::Rejected,
            SendOutcome::Rejected,
        ));
        let error: SdkError<MockServiceError, HttpResponse> =
            SdkError::response_error("invalid response body", response);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn nested_guard_verdict_does_not_annotate_timeout() {
        let nested = NestedOperationFailure(OperationSendFailure::new(
            OperationVerdict::new(SendOutcome::NotSent, SendOutcome::NotSent),
            Box::new(io::Error::other("nested SDK call failed")),
        ));
        let error: SdkError<MockServiceError, HttpResponse> = SdkError::timeout_error(nested);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn service_error_with_operation_verdict_uses_verdict() {
        let error =
            service_error_with_outcome("ValidationException", None, 400, SendOutcome::Rejected);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::Rejected
        );
    }

    #[test]
    fn service_error_without_operation_verdict_may_have_been_sent() {
        let error = service_error("ValidationException", None, 400);

        assert_eq!(
            SendClassification::of(&error).operation_outcome(),
            SendOutcome::MaybeSent
        );
    }

    #[test]
    fn orchestrator_service_failures_have_expected_outcomes() {
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
            (
                "Throttling",
                Some(ErrorKind::ThrottlingError),
                400,
                SendOutcome::Rejected,
            ),
            ("SlowDown", None, 503, SendOutcome::Rejected),
            ("RequestLimitExceeded", None, 503, SendOutcome::Rejected),
            ("OperationAborted", None, 409, SendOutcome::Rejected),
            ("ValidationException", None, 400, SendOutcome::Rejected),
            ("RequestTimeout", None, 400, SendOutcome::MaybeSent),
            ("RequestTimeoutException", None, 400, SendOutcome::MaybeSent),
            (
                "InternalError",
                Some(ErrorKind::ServerError),
                500,
                SendOutcome::MaybeSent,
            ),
        ];

        for (code, retryable, status, expected) in cases {
            let mut context = InterceptorContext::new(Input::doesnt_matter());
            context.set_response(http_response(status));
            context.set_output_or_error(Err(OrchestratorError::operation(Error::erase(
                MockServiceError::new(code, retryable),
            ))));

            assert_eq!(
                SendOutcome::classify_orchestrator(
                    AttemptPhase::TransmitReached,
                    &context,
                    &runtime_components,
                ),
                expected,
                "unexpected per-attempt outcome for {code}"
            );
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
            let classification = SendClassification {
                operation_outcome: outcome,
                final_attempt: FinalAttempt::Sent,
            };
            assert_eq!(
                classification.resend_is_safe(idempotency),
                expected,
                "unexpected resend safety for {outcome:?} and {idempotency:?}"
            );
        }
    }

    #[test]
    fn prior_maybe_sent_attempt_preserves_the_final_attempt_verdict() {
        let classification = SendClassification {
            operation_outcome: SendOutcome::NotSent,
            final_attempt: FinalAttempt::NotSent(Transience::Permanent),
        }
        .with_prior_maybe_sent();

        assert_eq!(classification.operation_outcome(), SendOutcome::MaybeSent);
        assert_eq!(
            classification.final_attempt(),
            FinalAttempt::NotSent(Transience::Permanent)
        );
    }
}
