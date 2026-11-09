//! SDK retry guard that aggregates every attempt into one operation verdict.

use std::sync::LazyLock;
use std::time::Instant;
use std::{fmt, io};

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeSerializationInterceptorContextRef, BeforeTransmitInterceptorContextRef, Error,
    FinalizerInterceptorContextMut, FinalizerInterceptorContextRef, InterceptorContext,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpResponse, Metadata, OrchestratorError};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};
use aws_smithy_types::error::ErrorMetadata;
use aws_smithy_types::retry::{ErrorKind, RetryConfig};
use carina_core::provider_text::ProviderDiagnostic;
use carina_plugin_sdk::{EventLevel, ProviderDiagnostics, ProviderEvent};
use regex::Regex;

use super::ServiceErrorCode;
use super::send_outcome::{
    AttemptOutcome, AttemptPhase, FinalAttempt, Idempotency, OperationSendFailure,
    OperationVerdict, SendClassification, not_sent_transience,
};

impl Storable for AttemptPhase {
    type Storer = StoreReplace<Self>;
}

fn attempt_phase(cfg: &ConfigBag) -> AttemptPhase {
    // A missing witness breaks the interceptor invariant, so assume the request may have been sent.
    cfg.load::<AttemptPhase>()
        .copied()
        .unwrap_or(AttemptPhase::TransmitReached)
}

/// Classifies one failed attempt, retaining transience only for NotSent.
pub(super) fn classify_attempt(
    phase: AttemptPhase,
    context: &InterceptorContext,
    runtime_components: &RuntimeComponents,
    error: &OrchestratorError<Error>,
) -> SendClassification {
    match AttemptOutcome::classify_orchestrator(phase, context, runtime_components) {
        AttemptOutcome::NotSent => SendClassification::NotSent(not_sent_transience(error)),
        AttemptOutcome::Rejected => SendClassification::Rejected,
        AttemptOutcome::MaybeSent => SendClassification::MaybeSent {
            final_attempt: FinalAttempt::Sent,
        },
    }
}

#[derive(Debug, Clone)]
struct PreviousAttempt {
    outcome: AttemptOutcome,
    failure: String,
    retry_summary: RetryFailure,
    finished_at: Instant,
}

impl Storable for PreviousAttempt {
    type Storer = StoreReplace<Self>;
}

#[derive(Debug, Clone, Copy)]
struct LastStartedAttempt(RequestAttempts);

impl Storable for LastStartedAttempt {
    type Storer = StoreReplace<Self>;
}

#[derive(Debug, Clone, Copy)]
struct MaybeSentAttempt;

impl Storable for MaybeSentAttempt {
    type Storer = StoreReplace<Self>;
}

fn error_message(error: &(dyn std::error::Error + 'static)) -> String {
    // Match the Display/source-chain portion of DisplayErrorContext. Its Debug
    // tail may contain requests or signing data, so it must not reach stderr.
    // Display and source messages can themselves contain signing data. Sanitize
    // each separately so an Authorization value cannot swallow the next cause.
    static URL_QUERY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(\b[a-z][a-z0-9+.-]*://[^\s?<>"']*)\?[^\s<>"']*"#)
            .expect("valid URL query pattern")
    });
    static AUTHORIZATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(\bauthorization\b["']?\s*:\s*)[^\r\n]*"#)
            .expect("valid Authorization pattern")
    });
    static SIGNING_VALUE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)(\b(?:x-amz-(?:signature|security-token|credential)|signature)\b["']?\s*[:=]\s*)(?:"[^"\r\n]*"|'[^'\r\n]*'|[^\s&,;]+)"#,
        )
        .expect("valid signing value pattern")
    });
    let redact = |message: &str| {
        let message = URL_QUERY.replace_all(message, "${1}");
        let message = AUTHORIZATION.replace_all(&message, "${1}[REDACTED]");
        SIGNING_VALUE
            .replace_all(&message, "${1}[REDACTED]")
            .into_owned()
    };
    let mut rendered = redact(&error.to_string());
    let mut source = error.source();
    while let Some(error) = source {
        rendered.push_str(": ");
        rendered.push_str(&redact(&error.to_string()));
        source = error.source();
    }
    rendered
}

fn rendered_failure(
    error: &(dyn std::error::Error + 'static),
    response: Option<&aws_smithy_runtime_api::client::orchestrator::HttpResponse>,
) -> String {
    let mut rendered = error_message(error);
    if let Some(response) = response {
        rendered.push_str(&format!("; status: {}", response.status().as_u16()));
        if let Some(request_id) = request_id(response) {
            rendered.push_str("; request_id: ");
            rendered.push_str(request_id);
        }
    }
    rendered
}

fn request_id(response: &HttpResponse) -> Option<&str> {
    [
        "x-amzn-requestid",
        "x-amzn-request-id",
        "x-amz-request-id",
        "x-amz-id-2",
    ]
    .into_iter()
    .find_map(|header| response.headers().get(header))
}

fn find_source<'a, T: std::error::Error + 'static>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a T> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(typed) = error.downcast_ref::<T>() {
            return Some(typed);
        }
        current = error.source();
    }
    None
}

/// Only typed fields enter retry diagnostics; raw error formatters never run.
#[derive(Debug, Clone)]
enum RetryFailure {
    Http { status: u16, code: Option<String> },
    ConnectorTimeout,
    ConnectorIo(Option<io::ErrorKind>),
    Connector,
    Timeout,
    Other,
}

impl fmt::Display for RetryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { status, code } => {
                write!(f, "HTTP {status}")?;
                if let Some(code) = code {
                    write!(f, " {code}")?;
                }
                Ok(())
            }
            Self::ConnectorTimeout => f.write_str("connector timeout"),
            Self::ConnectorIo(kind) => {
                f.write_str("connector io error")?;
                if let Some(kind) = kind {
                    write!(f, " ({kind:?})")?;
                }
                Ok(())
            }
            Self::Connector => f.write_str("connector error"),
            Self::Timeout => f.write_str("Timeout"),
            Self::Other => f.write_str("Other"),
        }
    }
}

fn retry_summary(
    error: &OrchestratorError<Error>,
    response: Option<&HttpResponse>,
) -> RetryFailure {
    if (error.is_response_error() || error.is_operation_error())
        && let Some(response) = response
    {
        // Modeled errors may hide metadata; unavailable metadata yields status only.
        let metadata = error
            .as_operation_error()
            .and_then(|error| error.downcast_ref::<ErrorMetadata>())
            .or_else(|| find_source::<ErrorMetadata>(error));
        let code = metadata
            .and_then(ErrorMetadata::code)
            .and_then(ServiceErrorCode::parse)
            .map(|code| code.as_str().to_owned());
        return RetryFailure::Http {
            status: response.status().as_u16(),
            code,
        };
    }
    match OrchestratorKind::of(error) {
        OrchestratorKind::Connector(ConnectorKind::Timeout) => RetryFailure::ConnectorTimeout,
        OrchestratorKind::Connector(ConnectorKind::Io) => {
            RetryFailure::ConnectorIo(find_source::<io::Error>(error).map(io::Error::kind))
        }
        OrchestratorKind::Connector(ConnectorKind::User | ConnectorKind::Other(_)) => {
            RetryFailure::Connector
        }
        OrchestratorKind::Timeout => RetryFailure::Timeout,
        OrchestratorKind::Other => RetryFailure::Other,
    }
}

#[derive(Debug)]
enum RetryDiagnostic {
    Retrying {
        service: String,
        operation: String,
        attempt: u32,
        max: u32,
        elapsed: f64,
        failure: RetryFailure,
    },
    GaveUp {
        service: String,
        operation: String,
        attempts: u32,
    },
}
impl fmt::Display for RetryDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retrying {
                service,
                operation,
                attempt,
                max,
                elapsed,
                failure,
            } => write!(
                f,
                "  Retrying {service}.{operation} (SDK attempt {attempt}/{max} after {elapsed:.1}s): {failure}"
            ),
            Self::GaveUp {
                service,
                operation,
                attempts,
            } => write!(
                f,
                "  Gave up {service}.{operation} after {attempts} SDK attempts"
            ),
        }
    }
}
impl std::error::Error for RetryDiagnostic {}

#[derive(Debug, Clone, Copy)]
enum ConnectorKind {
    Timeout,
    User,
    Io,
    Other(Option<ErrorKind>),
}

impl ConnectorKind {
    fn of(error: &ConnectorError) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else if error.is_user() {
            Self::User
        } else if error.is_io() {
            Self::Io
        } else {
            Self::Other(error.as_other())
        }
    }

    fn with_source(self, source: BoxError) -> ConnectorError {
        match self {
            Self::Timeout => ConnectorError::timeout(source),
            Self::User => ConnectorError::user(source),
            Self::Io => ConnectorError::io(source),
            Self::Other(kind) => ConnectorError::other(source, kind),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum OrchestratorKind {
    Connector(ConnectorKind),
    Timeout,
    Other,
}

impl OrchestratorKind {
    fn of<E>(error: &OrchestratorError<E>) -> Self {
        if let Some(error) = error.as_connector_error() {
            Self::Connector(ConnectorKind::of(error))
        } else if error.is_timeout_error() {
            Self::Timeout
        } else {
            Self::Other
        }
    }

    fn with_source<E>(self, source: BoxError) -> OrchestratorError<E> {
        match self {
            Self::Connector(kind) => OrchestratorError::connector(kind.with_source(source)),
            Self::Timeout => OrchestratorError::timeout(source),
            Self::Other => OrchestratorError::other(source),
        }
    }
}

/// Associates one service client's SDK metadata with generated resend policy.
pub struct ResendGuard {
    lookup: fn(&str) -> Idempotency,
    diagnostics: ProviderDiagnostics,
}

impl fmt::Debug for ResendGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResendGuard")
            .field("lookup", &self.lookup)
            .finish_non_exhaustive()
    }
}

impl ResendGuard {
    /// Creates a guard using the service's generated idempotency lookup.
    pub fn new(lookup: fn(&str) -> Idempotency, diagnostics: ProviderDiagnostics) -> Self {
        Self {
            lookup,
            diagnostics,
        }
    }
}

impl Intercept for ResendGuard {
    fn name(&self) -> &'static str {
        "carina.ResendGuard"
    }

    fn read_before_execution(
        &self,
        _context: &BeforeSerializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        cfg.interceptor_state()
            .store_put(AttemptPhase::BeforeTransmit);
        cfg.interceptor_state().unset::<LastStartedAttempt>();
        Ok(())
    }

    fn read_before_attempt(
        &self,
        _context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let started_at = Instant::now();
        let attempts = cfg
            .load::<RequestAttempts>()
            .copied()
            .unwrap_or_else(|| RequestAttempts::new(1));
        // The SDK advances RequestAttempts before sleeping. Retain its value
        // here so a timeout during backoff doesn't count an unstarted attempt.
        cfg.interceptor_state()
            .store_put(LastStartedAttempt(attempts));
        let attempt = attempts.attempts();
        let previous_phase = attempt_phase(cfg);
        cfg.interceptor_state()
            .store_put(AttemptPhase::BeforeTransmit);
        if attempt < 2 {
            return Ok(());
        }

        let operation = cfg
            .load::<Metadata>()
            .map(|metadata| metadata.name().to_string());
        let idempotency = operation
            .as_deref()
            .map(self.lookup)
            .unwrap_or(Idempotency::NotIdempotent);
        let previous = cfg.load::<PreviousAttempt>().cloned().unwrap_or_else(|| {
            // Attempt finalizers should always run; this fallback defensively
            // uses the transmit witness if that Smithy invariant is broken.
            let outcome = match previous_phase {
                AttemptPhase::BeforeTransmit => AttemptOutcome::NotSent,
                AttemptPhase::TransmitReached => AttemptOutcome::MaybeSent,
            };
            PreviousAttempt {
                outcome,
                failure: "failure details unavailable".to_string(),
                retry_summary: RetryFailure::Other,
                finished_at: started_at,
            }
        });
        if previous.outcome == AttemptOutcome::MaybeSent {
            cfg.interceptor_state().store_put(MaybeSentAttempt);
        }
        cfg.interceptor_state().unset::<PreviousAttempt>();

        if let Some(metadata) = cfg.load::<Metadata>() {
            let max = cfg
                .load::<RetryConfig>()
                .map(RetryConfig::max_attempts)
                .unwrap_or_else(|| RetryConfig::standard().max_attempts());
            let elapsed = started_at
                .duration_since(previous.finished_at)
                .as_secs_f64();
            let diagnostic = ProviderDiagnostic::new(RetryDiagnostic::Retrying {
                service: metadata.service().to_owned(),
                operation: metadata.name().to_owned(),
                attempt,
                max,
                elapsed,
                failure: previous.retry_summary,
            });
            self.diagnostics.emit(ProviderEvent::diagnostic(
                EventLevel::Warn,
                "SDK retry".into(),
                &diagnostic,
            ));
        }

        if previous.outcome.resend_is_safe(idempotency) {
            return Ok(());
        }

        let operation = operation.as_deref().unwrap_or("unknown operation");
        Err(io::Error::other(format!(
            "previous attempt may have reached AWS; refusing to resend non-idempotent {operation}; previous attempt failed: {}",
            previous.failure
        ))
        .into())
    }

    fn read_before_transmit(
        &self,
        _context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        cfg.interceptor_state()
            .store_put(AttemptPhase::TransmitReached);
        Ok(())
    }

    fn modify_before_attempt_completion(
        &self,
        context: &mut FinalizerInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let is_read_only = cfg
            .load::<Metadata>()
            .is_some_and(|metadata| (self.lookup)(metadata.name()) == Idempotency::ReadOnly);
        if !is_read_only || attempt_phase(cfg) != AttemptPhase::TransmitReached {
            return Ok(());
        }

        let is_unclassified_connector_failure = context
            .output_or_error()
            .and_then(Result::err)
            .and_then(OrchestratorError::as_connector_error)
            .is_some_and(|error| error.is_other() && error.as_other().is_none());
        if !is_unclassified_connector_failure {
            return Ok(());
        }

        let Some(Err(error)) = context.output_or_error_mut() else {
            return Ok(());
        };
        let original = std::mem::replace(
            error,
            OrchestratorError::other(io::Error::other(
                "replacing read-only unclassified connector failure",
            )),
        );
        // Smithy's attempt finalizers run before retry classification. Preserve
        // the original MaybeSent failure as the source while making only this
        // read-only attempt transient to the standard classifier.
        *error = OrchestratorError::connector(ConnectorError::io(Box::new(original)));
        Ok(())
    }

    fn read_after_attempt(
        &self,
        context: &FinalizerInterceptorContextRef<'_>,
        runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let outcome = match context.output_or_error() {
            Some(Ok(_)) => {
                cfg.interceptor_state().unset::<PreviousAttempt>();
                cfg.interceptor_state().unset::<MaybeSentAttempt>();
                return Ok(());
            }
            Some(Err(_)) | None => {
                let phase = attempt_phase(cfg);
                AttemptOutcome::classify_orchestrator(phase, context.inner(), runtime_components)
            }
        };
        if outcome == AttemptOutcome::MaybeSent {
            cfg.interceptor_state().store_put(MaybeSentAttempt);
        }
        // The interceptor only lends us the SDK error, and `SdkError` is not
        // `Clone`. A later refusal can therefore carry the previous failure
        // only as rendered source-chain text rather than as the typed error.
        let failure = context
            .output_or_error()
            .and_then(Result::err)
            .map(|error| rendered_failure(error, context.response()))
            .unwrap_or_else(|| "failure details unavailable".to_string());
        let retry_summary = context
            .output_or_error()
            .and_then(Result::err)
            .map(|error| retry_summary(error, context.response()))
            .unwrap_or(RetryFailure::Other);
        let finished_at = Instant::now();
        cfg.interceptor_state().store_put(PreviousAttempt {
            outcome,
            failure,
            retry_summary,
            finished_at,
        });
        Ok(())
    }

    fn modify_before_completion(
        &self,
        context: &mut FinalizerInterceptorContextMut<'_>,
        runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let Some(Err(error)) = context.output_or_error() else {
            return Ok(());
        };
        if error.is_response_error() {
            return Ok(());
        }

        let final_phase = attempt_phase(cfg);
        let classification =
            classify_attempt(final_phase, context.inner(), runtime_components, error);
        let classification = if cfg.load::<MaybeSentAttempt>().is_some() {
            classification.with_prior_maybe_sent()
        } else {
            classification
        };
        let verdict = OperationVerdict::new(classification);

        if let Some(response) = context.response_mut() {
            response.add_extension(verdict);
            return Ok(());
        }

        let Some(Err(error)) = context.output_or_error_mut() else {
            return Ok(());
        };
        let error_kind = OrchestratorKind::of(error);
        let original = std::mem::replace(
            error,
            OrchestratorError::other(io::Error::other("replacing final operation failure")),
        );
        let failure = OperationSendFailure::new(verdict, Box::new(original));
        *error = error_kind.with_source(Box::new(failure));
        Ok(())
    }

    fn read_after_execution(
        &self,
        context: &FinalizerInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if !matches!(context.output_or_error(), Some(Err(_))) {
            return Ok(());
        }
        if let (Some(attempt), Some(metadata)) =
            (cfg.load::<LastStartedAttempt>(), cfg.load::<Metadata>())
            && attempt.0.attempts() > 1
        {
            let diagnostic = ProviderDiagnostic::new(RetryDiagnostic::GaveUp {
                service: metadata.service().to_owned(),
                operation: metadata.name().to_owned(),
                attempts: attempt.0.attempts(),
            });
            self.diagnostics.emit(ProviderEvent::diagnostic(
                EventLevel::Warn,
                "SDK retry".into(),
                &diagnostic,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod retry_visibility_tests;

#[cfg(test)]
mod tests {
    use carina_plugin_sdk::ProviderDiagnostics;
    use std::io;

    use aws_smithy_async::time::StaticTimeSource;
    use aws_smithy_runtime_api::client::interceptors::Intercept;
    use aws_smithy_runtime_api::client::interceptors::context::{Input, InterceptorContext};
    use aws_smithy_runtime_api::client::orchestrator::{
        HttpRequest, HttpResponse, Metadata, OrchestratorError,
    };
    use aws_smithy_runtime_api::client::result::{ConnectorError, SdkError};
    use aws_smithy_runtime_api::client::retries::RequestAttempts;
    use aws_smithy_runtime_api::client::runtime_components::{
        RuntimeComponents, RuntimeComponentsBuilder,
    };
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::config_bag::ConfigBag;
    use aws_smithy_types::error::ErrorMetadata;
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};

    use super::{Idempotency, OperationVerdict, ResendGuard};
    use crate::common::send_outcome::{FinalAttempt, SendClassification, Transience};

    #[derive(Debug)]
    struct MockServiceError(ErrorMetadata);

    impl std::fmt::Display for MockServiceError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("mock service error")
        }
    }

    impl std::error::Error for MockServiceError {}

    impl ProvideErrorKind for MockServiceError {
        fn retryable_error_kind(&self) -> Option<ErrorKind> {
            None
        }

        fn code(&self) -> Option<&str> {
            self.0.code()
        }
    }

    impl ProvideErrorMetadata for MockServiceError {
        fn meta(&self) -> &ErrorMetadata {
            &self.0
        }
    }

    fn not_idempotent(_operation: &str) -> Idempotency {
        Idempotency::NotIdempotent
    }

    fn token_idempotent(_operation: &str) -> Idempotency {
        Idempotency::IdempotencyToken
    }

    fn read_only(_operation: &str) -> Idempotency {
        Idempotency::ReadOnly
    }

    fn before_transmit_context() -> InterceptorContext {
        let mut context = InterceptorContext::new(Input::doesnt_matter());
        context.enter_serialization_phase();
        let _ = context.take_input();
        context.set_request(HttpRequest::empty());
        context.enter_before_transmit_phase();
        context
    }

    fn runtime_components() -> RuntimeComponents {
        RuntimeComponentsBuilder::for_tests()
            .with_time_source(Some(StaticTimeSource::new(std::time::UNIX_EPOCH)))
            .build()
            .expect("test runtime components")
    }

    fn operation_config(attempt: u32) -> ConfigBag {
        let mut cfg = ConfigBag::base();
        cfg.interceptor_state()
            .store_put(Metadata::new("CreateThing", "TestService"));
        cfg.interceptor_state()
            .store_put::<RequestAttempts>(attempt.into());
        cfg
    }

    fn http_response(status: u16) -> HttpResponse {
        HttpResponse::new(
            status.try_into().expect("valid HTTP status"),
            SdkBody::empty(),
        )
    }

    fn classify_final_context(
        guard: &ResendGuard,
        runtime_components: &RuntimeComponents,
        mut context: InterceptorContext,
        cfg: &mut ConfigBag,
    ) -> (
        SendClassification,
        SdkError<MockServiceError, aws_smithy_runtime_api::client::orchestrator::HttpResponse>,
    ) {
        guard
            .modify_before_completion(&mut (&mut context).into(), runtime_components, cfg)
            .expect("operation finalization succeeds");
        let error = context
            .finalize()
            .expect_err("the operation failed")
            .map_service_error(|_| -> MockServiceError {
                unreachable!("the test contexts never contain service errors")
            });
        (SendClassification::of(&error), error)
    }

    fn error_chain_contains(error: &(dyn std::error::Error + 'static), expected: &str) -> bool {
        let mut current = Some(error);
        while let Some(error) = current {
            if error.to_string().contains(expected) {
                return true;
            }
            current = error.source();
        }
        false
    }

    fn run_pre_transmit_failure(
        failure: OrchestratorError<aws_smithy_runtime_api::client::interceptors::context::Error>,
    ) -> (
        SendClassification,
        SdkError<MockServiceError, aws_smithy_runtime_api::client::orchestrator::HttpResponse>,
    ) {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        context.set_output_or_error(Err(failure));
        guard
            .read_after_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, context, &mut cfg);
        (outcome, error)
    }

    #[test]
    fn pre_transmit_other_failure_is_typed_not_sent() {
        let failure = "credentials not loaded";
        let (outcome, error) =
            run_pre_transmit_failure(OrchestratorError::other(io::Error::other(failure)));

        assert_eq!(outcome, SendClassification::NotSent(Transience::Permanent));
        assert!(error_chain_contains(&error, failure), "{error:?}");
    }

    #[test]
    fn typed_classification_carries_final_attempt_transience() {
        use carina_plugin_sdk::wasi_http::{SendPhase, transport_connector_error_for_test};

        let source = || Box::new(io::Error::other("pre-transmit connector failure"));
        let mut cases = vec![
            (ConnectorError::timeout(source()), Transience::Transient),
            (ConnectorError::io(source()), Transience::Transient),
            (ConnectorError::other(source(), None), Transience::Permanent),
            (ConnectorError::user(source()), Transience::Permanent),
            (
                transport_connector_error_for_test(SendPhase::NotSent),
                Transience::Permanent,
            ),
            (
                ConnectorError::other(source(), Some(ErrorKind::ClientError)),
                Transience::Permanent,
            ),
        ];
        for kind in [
            ErrorKind::TransientError,
            ErrorKind::ThrottlingError,
            ErrorKind::ServerError,
        ] {
            cases.push((
                ConnectorError::other(source(), Some(kind)),
                Transience::Transient,
            ));
        }

        for (connector, expected) in cases {
            let (_, error) = run_pre_transmit_failure(OrchestratorError::connector(connector));
            assert_eq!(
                SendClassification::of(&error),
                SendClassification::NotSent(expected),
                "{error:?}"
            );
        }
    }

    #[test]
    fn pre_transmit_orchestrator_timeout_is_typed_not_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        context.set_output_or_error(Err(OrchestratorError::timeout(Box::new(io::Error::other(
            "operation attempt timed out during identity resolution",
        )))));
        guard
            .read_after_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, context, &mut cfg);

        assert!(matches!(error, SdkError::TimeoutError(_)), "{error:?}");
        assert_eq!(outcome, SendClassification::NotSent(Transience::Transient));
        assert_eq!(
            SendClassification::of(&error),
            SendClassification::NotSent(Transience::Transient)
        );
        assert!(error_chain_contains(
            &error,
            "operation attempt timed out during identity resolution"
        ));
    }

    #[test]
    fn absent_attempt_phase_fails_closed_to_maybe_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        context.set_output_or_error(Err(OrchestratorError::other(io::Error::other(
            "failure without a transmit witness",
        ))));
        guard
            .read_after_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("missing phase is handled conservatively");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, context, &mut cfg);

        assert!(matches!(outcome, SendClassification::MaybeSent { .. }));
        assert!(error_chain_contains(
            &error,
            "failure without a transmit witness"
        ));
    }

    #[test]
    fn serialization_failure_keeps_construction_variant_and_source() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut context = InterceptorContext::new(Input::doesnt_matter());
        let mut cfg = operation_config(1);

        guard
            .read_before_execution(&(&context).into(), &mut cfg)
            .expect("execution initialization succeeds");
        context.enter_serialization_phase();
        context.set_output_or_error(Err(OrchestratorError::other(io::Error::other(
            "request serialization failed",
        ))));

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, context, &mut cfg);

        assert!(
            matches!(error, SdkError::ConstructionFailure(_)),
            "{error:?}"
        );
        assert_eq!(outcome, SendClassification::NotSent(Transience::Permanent));
        assert!(error_chain_contains(&error, "request serialization failed"));
    }

    #[test]
    fn other_connector_failure_after_transmit_stays_maybe_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&context).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        context.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("post-transmit connector failure")),
            None,
        ))));
        guard
            .read_after_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, context, &mut cfg);

        assert!(matches!(outcome, SendClassification::MaybeSent { .. }));
        assert!(error_chain_contains(
            &error,
            "post-transmit connector failure"
        ));
    }

    #[test]
    fn pre_transmit_readonly_unclassified_connector_failure_is_not_rewritten() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(read_only, ProviderDiagnostics::from_session(&session));
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        context.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("pre-transmit local failure")),
            None,
        ))));
        guard
            .modify_before_attempt_completion(
                &mut (&mut context).into(),
                &runtime_components,
                &mut cfg,
            )
            .expect("attempt completion succeeds");

        let connector = context
            .output_or_error()
            .and_then(Result::err)
            .and_then(OrchestratorError::as_connector_error)
            .expect("connector error remains typed");
        assert!(connector.is_other());
        assert!(connector.as_other().is_none());
    }

    #[test]
    fn post_transmit_readonly_unclassified_connector_failure_is_rewritten() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(read_only, ProviderDiagnostics::from_session(&session));
        let runtime_components = runtime_components();
        let mut context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&context).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        context.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("post-transmit local failure")),
            None,
        ))));
        guard
            .modify_before_attempt_completion(
                &mut (&mut context).into(),
                &runtime_components,
                &mut cfg,
            )
            .expect("attempt completion succeeds");

        let connector = context
            .output_or_error()
            .and_then(Result::err)
            .and_then(OrchestratorError::as_connector_error)
            .expect("connector error remains typed");
        assert!(connector.is_io());
    }

    #[test]
    fn maybe_sent_attempt_then_pre_transmit_failure_is_maybe_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut cfg = operation_config(1);
        let mut first = before_transmit_context();

        guard
            .read_before_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&first).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        first.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("post-transmit connector failure")),
            None,
        ))));
        guard
            .read_after_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt finalization succeeds");

        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());
        let mut second = before_transmit_context();
        guard
            .read_before_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("token operation can retry");
        second.set_output_or_error(Err(OrchestratorError::other(io::Error::other(
            "credentials expired before retry",
        ))));
        guard
            .read_after_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("second attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, second, &mut cfg);
        let classification = SendClassification::of(&error);

        assert!(matches!(outcome, SendClassification::MaybeSent { .. }));
        assert_eq!(
            classification,
            SendClassification::MaybeSent {
                final_attempt: FinalAttempt::NotSent(Transience::Permanent),
            }
        );
    }

    #[test]
    fn maybe_sent_attempt_then_pre_transmit_timeout_keeps_transient_final_attempt() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut cfg = operation_config(1);
        let mut first = before_transmit_context();

        guard
            .read_before_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&first).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        first.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("post-transmit connector failure")),
            None,
        ))));
        guard
            .read_after_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt finalization succeeds");

        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());
        let mut second = before_transmit_context();
        guard
            .read_before_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("token operation can retry");
        second.set_output_or_error(Err(OrchestratorError::timeout(Box::new(io::Error::other(
            "operation attempt timed out before transmit",
        )))));
        guard
            .read_after_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("second attempt finalization succeeds");

        let (classification, error) =
            classify_final_context(&guard, &runtime_components, second, &mut cfg);

        assert!(matches!(error, SdkError::TimeoutError(_)), "{error:?}");
        assert_eq!(
            classification,
            SendClassification::MaybeSent {
                final_attempt: FinalAttempt::NotSent(Transience::Transient),
            }
        );
    }

    #[test]
    fn maybe_sent_attempt_then_response_error_is_maybe_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut cfg = operation_config(1);
        let mut first = before_transmit_context();

        guard
            .read_before_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&first).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        first.set_output_or_error(Err(OrchestratorError::connector(ConnectorError::other(
            Box::new(io::Error::other("post-transmit connector failure")),
            None,
        ))));
        guard
            .read_after_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt finalization succeeds");

        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());
        let mut second = before_transmit_context();
        guard
            .read_before_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("token operation can retry");
        guard
            .read_before_transmit(&(&second).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");
        second.enter_transmit_phase();
        let _ = second.take_request();
        second.set_response(http_response(400));
        second.enter_before_deserialization_phase();
        second.set_output_or_error(Err(OrchestratorError::response(Box::new(
            io::Error::other("response deserialization failed"),
        ))));
        guard
            .read_after_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("second attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, second, &mut cfg);

        assert!(matches!(error, SdkError::ResponseError(_)), "{error:?}");
        assert!(matches!(outcome, SendClassification::MaybeSent { .. }));
        assert!(
            error
                .raw_response()
                .expect("response error retains the raw response")
                .extension::<OperationVerdict>()
                .is_none(),
            "response errors must not carry a resend-guard verdict"
        );
        assert!(error_chain_contains(
            &error,
            "response deserialization failed"
        ));
    }

    #[test]
    fn all_attempts_pre_transmit_is_not_sent() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(
            token_idempotent,
            ProviderDiagnostics::from_session(&session),
        );
        let runtime_components = runtime_components();
        let mut cfg = operation_config(1);
        let mut first = before_transmit_context();

        guard
            .read_before_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        first.set_output_or_error(Err(OrchestratorError::other(io::Error::other(
            "first pre-transmit failure",
        ))));
        guard
            .read_after_attempt(&(&first).into(), &runtime_components, &mut cfg)
            .expect("first attempt finalization succeeds");

        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());
        let mut second = before_transmit_context();
        guard
            .read_before_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("pre-transmit failure is safe to retry");
        second.set_output_or_error(Err(OrchestratorError::other(io::Error::other(
            "second pre-transmit failure",
        ))));
        guard
            .read_after_attempt(&(&second).into(), &runtime_components, &mut cfg)
            .expect("second attempt finalization succeeds");

        let (outcome, error) =
            classify_final_context(&guard, &runtime_components, second, &mut cfg);

        assert_eq!(outcome, SendClassification::NotSent(Transience::Permanent));
        assert!(error_chain_contains(&error, "second pre-transmit failure"));
    }

    #[test]
    fn non_idempotent_retry_is_allowed_when_previous_attempt_never_reached_transmit() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(not_idempotent, ProviderDiagnostics::from_session(&session));
        let runtime_components = runtime_components();
        let context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");

        // Simulate Smithy's attempt-finalizer invariant being broken. The
        // defensive fallback still knows transmit was never reached.
        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("a proven pre-transmit failure is safe to resend");
    }

    #[test]
    fn non_idempotent_retry_is_refused_when_transmit_was_reached_without_finalizer() {
        let session = carina_core::provider::ProviderPlan::state_only().session("aws", None, None);
        let guard = ResendGuard::new(not_idempotent, ProviderDiagnostics::from_session(&session));
        let runtime_components = runtime_components();
        let context = before_transmit_context();
        let mut cfg = operation_config(1);

        guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect("first attempt is allowed");
        guard
            .read_before_transmit(&(&context).into(), &runtime_components, &mut cfg)
            .expect("transmit is allowed");

        // Simulate Smithy's attempt-finalizer invariant being broken. The
        // transmit witness must keep that missing record conservative.
        cfg.interceptor_state()
            .store_put::<RequestAttempts>(2_u32.into());

        let error = guard
            .read_before_attempt(&(&context).into(), &runtime_components, &mut cfg)
            .expect_err("an in-flight non-idempotent attempt cannot be resent");
        assert!(error.to_string().contains("may have reached AWS"));
    }
}
