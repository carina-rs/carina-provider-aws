//! SDK retry guard that aggregates every attempt into one operation verdict.

use std::io;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeTransmitInterceptorContextRef, FinalizerInterceptorContextMut,
    FinalizerInterceptorContextRef,
};
use aws_smithy_runtime_api::client::orchestrator::{Metadata, OrchestratorError};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};
use aws_smithy_types::retry::ErrorKind;

use super::send_outcome::{
    Idempotency, OperationDispatchFailure, OperationSendOutcome, SendOutcome,
};

#[derive(Debug, Clone)]
struct PreviousAttempt {
    outcome: SendOutcome,
    failure: String,
}

impl Storable for PreviousAttempt {
    type Storer = StoreReplace<Self>;
}

#[derive(Debug, Clone, Copy)]
struct MaybeSentAttempt;

impl Storable for MaybeSentAttempt {
    type Storer = StoreReplace<Self>;
}

fn rendered_failure(
    error: &(dyn std::error::Error + 'static),
    response: Option<&aws_smithy_runtime_api::client::orchestrator::HttpResponse>,
) -> String {
    let mut rendered = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        rendered.push_str(": ");
        rendered.push_str(&error.to_string());
        source = error.source();
    }
    if let Some(response) = response {
        rendered.push_str(&format!("; status: {}", response.status().as_u16()));
        for header in [
            "x-amzn-requestid",
            "x-amzn-request-id",
            "x-amz-request-id",
            "x-amz-id-2",
        ] {
            if let Some(request_id) = response.headers().get(header) {
                rendered.push_str("; request_id: ");
                rendered.push_str(request_id);
                break;
            }
        }
    }
    rendered
}

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

/// Associates one service client's SDK metadata with generated resend policy.
#[derive(Debug)]
pub struct ResendGuard {
    lookup: fn(&str) -> Idempotency,
}

impl ResendGuard {
    /// Creates a guard using the service's generated idempotency lookup.
    pub fn new(lookup: fn(&str) -> Idempotency) -> Self {
        Self { lookup }
    }
}

impl Intercept for ResendGuard {
    fn name(&self) -> &'static str {
        "carina.ResendGuard"
    }

    fn read_before_attempt(
        &self,
        _context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let attempt = cfg
            .load::<RequestAttempts>()
            .map(RequestAttempts::attempts)
            .unwrap_or(1);
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
        let previous = cfg.load::<PreviousAttempt>().cloned();
        if previous
            .as_ref()
            .is_none_or(|previous| previous.outcome == SendOutcome::MaybeSent)
        {
            // The SDK's operation-attempt timeout wraps the attempt finalizers,
            // so a timed-out attempt reaches this hook without running
            // `read_after_attempt`. Absence is therefore ambiguous too.
            cfg.interceptor_state().store_put(MaybeSentAttempt);
        }
        let previous = previous.unwrap_or_else(|| PreviousAttempt {
            outcome: SendOutcome::MaybeSent,
            failure: "failure details unavailable".to_string(),
        });
        cfg.interceptor_state().unset::<PreviousAttempt>();

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

    fn modify_before_attempt_completion(
        &self,
        context: &mut FinalizerInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let is_read_only = cfg
            .load::<Metadata>()
            .is_some_and(|metadata| (self.lookup)(metadata.name()) == Idempotency::ReadOnly);
        if !is_read_only {
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
                SendOutcome::classify_orchestrator(context.inner(), runtime_components)
            }
        };
        if outcome == SendOutcome::MaybeSent {
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
        cfg.interceptor_state()
            .store_put(PreviousAttempt { outcome, failure });
        Ok(())
    }

    fn modify_before_completion(
        &self,
        context: &mut FinalizerInterceptorContextMut<'_>,
        runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if context
            .output_or_error()
            .is_none_or(|result| result.is_ok())
        {
            return Ok(());
        }

        let final_attempt = SendOutcome::classify_orchestrator(context.inner(), runtime_components);
        let operation_outcome = if cfg.load::<MaybeSentAttempt>().is_some()
            || final_attempt == SendOutcome::MaybeSent
        {
            SendOutcome::MaybeSent
        } else {
            final_attempt
        };
        let verdict = OperationSendOutcome::new(operation_outcome);

        if let Some(response) = context.response_mut() {
            response.add_extension(verdict);
            return Ok(());
        }

        let connector_kind = context
            .output_or_error()
            .and_then(Result::err)
            .and_then(OrchestratorError::as_connector_error)
            .map(ConnectorKind::of);
        let Some(connector_kind) = connector_kind else {
            // Construction, orchestrator timeout, and response errors retain
            // their SDK variants and remain conservatively MaybeSent (except
            // construction, which never started an attempt).
            return Ok(());
        };

        let Some(Err(error)) = context.output_or_error_mut() else {
            return Ok(());
        };
        let original = std::mem::replace(
            error,
            OrchestratorError::other(io::Error::other("replacing final connector failure")),
        );
        let failure = OperationDispatchFailure::new(verdict, Box::new(original));
        *error = OrchestratorError::connector(connector_kind.with_source(Box::new(failure)));
        Ok(())
    }
}
