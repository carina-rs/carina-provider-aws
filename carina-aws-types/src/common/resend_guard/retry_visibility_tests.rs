//! Exercise the SDK's retry loop, timers, config bag and finalizers together.

use std::collections::VecDeque;
use std::fmt;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_async::time::StaticTimeSource;
use aws_smithy_runtime::client::orchestrator::operation::Operation;
use aws_smithy_runtime::client::retries::classifiers::{
    HttpStatusCodeClassifier, TransientErrorClassifier,
};
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::http::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
};
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeTransmitInterceptorContextRef, Error, FinalizerInterceptorContextRef,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse, OrchestratorError};
use aws_smithy_runtime_api::client::result::{ConnectorError, SdkError};
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::body::SdkBody;
use aws_smithy_types::config_bag::ConfigBag;
use aws_smithy_types::error::ErrorMetadata;
use aws_smithy_types::retry::{ErrorKind, RetryConfig};
use aws_smithy_types::timeout::TimeoutConfig;
use regex::Regex;

use super::{Idempotency, ResendGuard, error_message, rendered_failure, retry_summary};

#[derive(Clone, Debug, Default)]
struct CapturedOutput(Arc<Mutex<Vec<String>>>);

impl CapturedOutput {
    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("output lock").clone()
    }

    fn write_line(&self, line: &str) {
        self.0.lock().expect("output lock").push(line.to_owned());
    }
}

#[derive(Debug)]
enum Reply {
    Io,
    DelayedIo(Duration),
    User,
    Status(u16),
    CodedStatus(u16, &'static str),
    ResponseFields { code: String, request_id: String },
    Stall,
}

struct ConnectionReset {
    request: HttpRequest,
    message: String,
    source: std::io::Error,
}

impl ConnectionReset {
    fn new(request: HttpRequest, secret: RequestSecret) -> Self {
        Self {
            request,
            message: sensitive_message(secret, "display"),
            source: std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                sensitive_message(secret, "source"),
            ),
        }
    }
}

impl fmt::Debug for ConnectionReset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionReset")
            .field("request", &self.request)
            .field("message", &self.message)
            .field("source", &self.source)
            .finish()
    }
}

impl fmt::Display for ConnectionReset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConnectionReset {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
struct ServiceFailure(Option<ErrorMetadata>);

impl fmt::Display for ServiceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("service unavailable")
    }
}

impl std::error::Error for ServiceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.as_ref().map(|metadata| metadata as _)
    }
}

fn http_response(status: u16) -> HttpResponse {
    let mut response =
        HttpResponse::new(status.try_into().expect("valid status"), SdkBody::empty());
    response
        .headers_mut()
        .insert("x-amzn-requestid", "test-request-id");
    response
}

#[derive(Debug, Default)]
struct Script {
    replies: VecDeque<Reply>,
    requests: Vec<String>,
    started_attempts: Vec<u32>,
    final_sdk_attempt: u32,
    final_failure: Option<String>,
    response_fields: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct Connector {
    script: Arc<Mutex<Script>>,
    secret: RequestSecret,
}

impl HttpConnector for Connector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let reply = {
            let mut script = self.script.lock().expect("script lock");
            script.requests.push(format!(
                "{}\nauthorization: {}\nx-amz-security-token: {}",
                request.uri(),
                request.headers().get("authorization").unwrap_or_default(),
                request
                    .headers()
                    .get("x-amz-security-token")
                    .unwrap_or_default(),
            ));
            script.replies.pop_front().expect("script exhausted")
        };
        let secret = self.secret;
        let script = self.script.clone();
        HttpConnectorFuture::new(async move {
            match reply {
                Reply::Io => Err(ConnectorError::io(Box::new(ConnectionReset::new(
                    request, secret,
                )))),
                Reply::DelayedIo(delay) => {
                    tokio::time::sleep(delay).await;
                    Err(ConnectorError::io(Box::new(ConnectionReset::new(
                        request, secret,
                    ))))
                }
                Reply::User => Err(ConnectorError::user("invalid request".into())),
                Reply::Status(status) => Ok(http_response(status)),
                Reply::CodedStatus(status, code) => {
                    let mut response = http_response(status);
                    response.add_extension(ErrorMetadata::builder().code(code)
                        .message("Authorization=Bearer private-service-message; Credential=AKID/private-service-message")
                        .build());
                    Ok(response)
                }
                Reply::ResponseFields { code, request_id } => {
                    let mut response = http_response(503);
                    response
                        .headers_mut()
                        .insert("x-amzn-requestid", request_id);
                    let metadata = ErrorMetadata::builder().code(code).build();
                    script.lock().expect("script lock").response_fields.push((
                        metadata.code().expect("injected code").to_owned(),
                        response
                            .headers()
                            .get("x-amzn-requestid")
                            .expect("injected request ID")
                            .to_owned(),
                    ));
                    response.add_extension(metadata);
                    Ok(response)
                }
                Reply::Stall => std::future::pending().await,
            }
        })
    }
}

#[derive(Debug)]
struct ObserveAttempts(Arc<Mutex<Script>>);

impl Intercept for ObserveAttempts {
    fn name(&self) -> &'static str {
        "test.ObserveAttempts"
    }

    fn read_before_attempt(
        &self,
        _context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        self.0.lock().expect("script lock").started_attempts.push(
            cfg.load::<RequestAttempts>()
                .expect("SDK attempt count")
                .attempts(),
        );
        Ok(())
    }

    fn read_after_execution(
        &self,
        context: &FinalizerInterceptorContextRef<'_>,
        _runtime: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut script = self.0.lock().expect("script lock");
        script.final_sdk_attempt = cfg
            .load::<RequestAttempts>()
            .expect("SDK attempt count")
            .attempts();
        if let Some(Err(error)) = context.output_or_error() {
            script.final_failure = Some(rendered_failure(error, context.response()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestSecret {
    None,
    Authorization,
    SessionToken,
    QuerySignature,
}

const AUTH_SIGNATURE: &str = "authorization-signature-must-stay-secret";
const AUTH_CREDENTIAL: &str = "authorization-credential-must-stay-secret";
const SESSION_TOKEN: &str = "session-token-must-stay-secret";
const QUERY_SIGNATURE: &str = "query-signature-must-stay-secret";
const QUERY_CREDENTIAL: &str = "query-credential-must-stay-secret";
const QUERY_TOKEN: &str = "query-session-token-must-stay-secret";
const OTHER_QUERY_VALUE: &str = "unlisted-query-value-must-also-be-removed";

fn sensitive_message(secret: RequestSecret, location: &str) -> String {
    let covered = match secret {
        RequestSecret::None => "connection reset".to_string(),
        RequestSecret::Authorization => format!(
            "connection reset; aUtHoRiZaTiOn: AWS4-HMAC-SHA256 Credential={AUTH_CREDENTIAL}-{location}, SignedHeaders=host, Signature={AUTH_SIGNATURE}-{location}"
        ),
        RequestSecret::SessionToken => {
            format!("connection reset; x-AmZ-SeCuRiTy-ToKeN={SESSION_TOKEN}-{location}")
        }
        RequestSecret::QuerySignature => format!(
            "connection reset for https://example.test/path?X-Amz-Signature={QUERY_SIGNATURE}-{location}&X-Amz-Credential={QUERY_CREDENTIAL}-{location}&X-Amz-Security-Token={QUERY_TOKEN}-{location}&other={OTHER_QUERY_VALUE}-{location}"
        ),
    };
    let value = match secret {
        RequestSecret::None => return covered,
        RequestSecret::Authorization => AUTH_SIGNATURE,
        RequestSecret::SessionToken => SESSION_TOKEN,
        RequestSecret::QuerySignature => QUERY_SIGNATURE,
    };
    format!(
        "{covered}\nAuthorization=Bearer {value}-bypass-{location}\nCredential=AKID/{value}-bypass-{location}"
    )
}

fn operation(
    replies: impl IntoIterator<Item = Reply>,
    max_attempts: u32,
    secret: RequestSecret,
) -> (
    Operation<(), (), ServiceFailure>,
    CapturedOutput,
    Arc<Mutex<Script>>,
) {
    operation_with_timeouts(
        replies,
        max_attempts,
        secret,
        TimeoutConfig::builder()
            .operation_attempt_timeout(Duration::from_secs(2))
            .operation_timeout(Duration::from_secs(5))
            .build(),
    )
}

fn operation_with_timeouts(
    replies: impl IntoIterator<Item = Reply>,
    max_attempts: u32,
    secret: RequestSecret,
    timeout_config: TimeoutConfig,
) -> (
    Operation<(), (), ServiceFailure>,
    CapturedOutput,
    Arc<Mutex<Script>>,
) {
    let output = CapturedOutput::default();
    let captured = output.clone();
    let guard = ResendGuard {
        lookup: |_| Idempotency::ReadOnly,
        output: Box::new(move |line| captured.write_line(line)),
    };
    let script = Arc::new(Mutex::new(Script {
        replies: replies.into_iter().collect(),
        ..Script::default()
    }));
    let connector = Connector {
        script: script.clone(),
        secret,
    };
    let operation = Operation::builder()
        .service_name("test")
        .operation_name("DescribeThing")
        .http_client(http_client_fn(move |_, _| SharedHttpConnector::new(connector.clone())))
        .endpoint_url("https://example.test")
        .no_auth()
        .standard_retry(&RetryConfig::standard().with_max_attempts(max_attempts)
            .with_initial_backoff(Duration::from_millis(100)).with_use_static_exponential_base(true))
        .retry_classifier(TransientErrorClassifier::<ServiceFailure>::new())
        .retry_classifier(HttpStatusCodeClassifier::default())
        .timeout_config(timeout_config)
        .sleep_impl(TokioSleep::new())
        // A frozen SDK wall clock must not affect measured, monotonic waits.
        .time_source(StaticTimeSource::new(SystemTime::UNIX_EPOCH))
        .interceptor(guard)
        .interceptor(ObserveAttempts(script.clone()))
        .serializer(move |(): ()| {
            let mut request = HttpRequest::empty();
            match secret {
                RequestSecret::None => {}
                RequestSecret::Authorization => {
                    request.headers_mut().insert("Authorization", format!(
                        "AWS4-HMAC-SHA256 Credential={AUTH_CREDENTIAL}, SignedHeaders=host, Signature={AUTH_SIGNATURE}"
                    ));
                }
                RequestSecret::SessionToken => { request.headers_mut().insert("X-Amz-Security-Token", SESSION_TOKEN); }
                RequestSecret::QuerySignature => { request.set_uri(format!("/?X-Amz-Signature={QUERY_SIGNATURE}"))?; }
            }
            Ok(request)
        })
        .deserializer(|response| {
            if response.status().is_success() { Ok(()) } else {
                Err(OrchestratorError::operation(ServiceFailure(response.extension::<ErrorMetadata>().cloned())))
            }
        })
        .build();
    (operation, output, script)
}

fn assert_lines(actual: &[String], expected: &[&str], elapsed_range: RangeInclusive<f64>) {
    assert_eq!(actual.len(), expected.len(), "{actual:#?}");
    for (line, expected) in actual.iter().zip(expected) {
        if let Some((prefix, suffix)) = expected.split_once("{secs}") {
            let pattern = Regex::new(&format!(
                "^{}([0-9]+\\.[0-9]){}$",
                regex::escape(prefix),
                regex::escape(suffix)
            ))
            .expect("retry line pattern");
            let captures = pattern
                .captures(line)
                .unwrap_or_else(|| panic!("unexpected retry line: {line}"));
            let seconds: f64 = captures[1].parse().expect("elapsed seconds");
            assert!(
                elapsed_range.contains(&seconds),
                "elapsed wait {seconds} outside {elapsed_range:?}: {line}"
            );
        } else {
            assert_eq!(line, expected);
        }
    }
}

#[tokio::test]
async fn retry_line_uses_sdk_counts_and_monotonic_wait() {
    let (operation, output, script) = operation(
        [
            Reply::DelayedIo(Duration::from_millis(700)),
            Reply::Status(200),
        ],
        4,
        RequestSecret::None,
    );
    operation.invoke(()).await.expect("retry succeeds");
    assert_eq!(script.lock().expect("script lock").requests.len(), 2);
    // Neither the frozen SDK clock nor the 700ms attempt duration belongs in the wait.
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/4 after {secs}s): connector io error (ConnectionReset)",
        ],
        0.05..=0.6,
    );
}

#[tokio::test]
async fn exhausted_retries_print_gave_up_after_three_attempts() {
    let (operation, output, script) =
        operation([Reply::Io, Reply::Io, Reply::Io], 3, RequestSecret::None);
    operation.invoke(()).await.expect_err("retries exhausted");
    assert_eq!(script.lock().expect("script lock").requests.len(), 3);
    let lines = output.lines();
    assert_lines(
        &lines,
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): connector io error (ConnectionReset)",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after {secs}s): connector io error (ConnectionReset)",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ],
        0.05..=2.0,
    );
    println!("{}", lines.join("\n"));
}

#[tokio::test]
async fn non_retryable_first_failure_prints_nothing() {
    let (operation, output, script) = operation([Reply::User], 3, RequestSecret::None);
    operation
        .invoke(())
        .await
        .expect_err("non-retryable failure");
    assert_eq!(script.lock().expect("script lock").requests.len(), 1);
    assert!(output.lines().is_empty());
}

#[tokio::test]
async fn retryable_failure_with_one_configured_attempt_prints_nothing() {
    let (operation, output, script) = operation([Reply::Io], 1, RequestSecret::None);
    operation.invoke(()).await.expect_err("retries disabled");
    assert_eq!(script.lock().expect("script lock").requests.len(), 1);
    assert!(output.lines().is_empty());
}

#[tokio::test]
async fn single_not_found_response_prints_nothing() {
    let (operation, output, script) = operation([Reply::Status(404)], 3, RequestSecret::None);
    let error = operation.invoke(()).await.expect_err("resource absent");
    assert!(
        error.as_service_error().is_some(),
        "preserve the typed service error"
    );
    assert_eq!(script.lock().expect("script lock").requests.len(), 1);
    assert!(output.lines().is_empty());
}

#[tokio::test]
async fn successful_retry_prints_only_retry_lines() {
    let (operation, output, _) = operation(
        [Reply::Io, Reply::Io, Reply::Status(200)],
        3,
        RequestSecret::None,
    );
    operation.invoke(()).await.expect("third attempt succeeds");
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): connector io error (ConnectionReset)",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after {secs}s): connector io error (ConnectionReset)",
        ],
        0.05..=2.0,
    );
}

#[tokio::test]
async fn service_failures_keep_their_type_and_print_status_only() {
    let (operation, output, script) = operation(
        [Reply::Status(503), Reply::Status(503), Reply::Status(503)],
        3,
        RequestSecret::None,
    );
    let error = operation.invoke(()).await.expect_err("retries exhausted");
    assert!(
        error.as_service_error().is_some(),
        "preserve the typed service error"
    );
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): HTTP 503",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after {secs}s): HTTP 503",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ],
        0.05..=2.0,
    );
    assert!(
        script
            .lock()
            .expect("script lock")
            .final_failure
            .as_ref()
            .expect("final rendered failure")
            .ends_with("; status: 503; request_id: test-request-id")
    );
}

#[tokio::test]
async fn service_failures_report_typed_code_without_request_id_or_message() {
    let (operation, output, _) = operation(
        [Reply::CodedStatus(503, "SlowDown"), Reply::Status(200)],
        3,
        RequestSecret::None,
    );
    operation.invoke(()).await.expect("retry succeeds");
    assert_lines(
        &output.lines(),
        &["  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): HTTP 503 SlowDown"],
        0.05..=2.0,
    );
}

#[derive(Debug)]
struct Unprintable;

impl fmt::Display for Unprintable {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("retry summaries must not format error messages")
    }
}

impl std::error::Error for Unprintable {}

#[test]
fn connector_and_orchestrator_summaries_never_format_errors() {
    let cases: Vec<(OrchestratorError<Error>, &str)> = vec![
        (
            OrchestratorError::connector(ConnectorError::timeout(Box::new(Unprintable))),
            "connector timeout",
        ),
        (
            OrchestratorError::connector(ConnectorError::io(Box::new(Unprintable))),
            "connector io error",
        ),
        (
            OrchestratorError::connector(ConnectorError::io(Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                Unprintable,
            )))),
            "connector io error (ConnectionReset)",
        ),
        (
            OrchestratorError::connector(ConnectorError::user(Box::new(Unprintable))),
            "connector error",
        ),
        (
            OrchestratorError::connector(ConnectorError::other(Box::new(Unprintable), None)),
            "connector error",
        ),
        (
            OrchestratorError::connector(ConnectorError::other(
                Box::new(Unprintable),
                Some(ErrorKind::TransientError),
            )),
            "connector error",
        ),
        (OrchestratorError::timeout(Box::new(Unprintable)), "Timeout"),
        (OrchestratorError::other(Unprintable), "Other"),
        (
            OrchestratorError::operation(Error::erase(Unprintable)),
            "Other",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(retry_summary(&error, None), expected);
    }
}

#[test]
fn http_summaries_only_use_typed_metadata_and_response_fields() {
    let metadata = ErrorMetadata::builder()
        .code("SlowDown")
        .message("Authorization=Bearer SECRET; Credential=AKID/SECRET")
        .build();
    let error = OrchestratorError::operation(Error::erase(metadata.clone()));
    let response = HttpResponse::new(503.try_into().expect("status"), SdkBody::empty());
    assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
    let response = http_response(503);
    assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
    let error = OrchestratorError::response(Box::new(metadata));
    assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
    let error = OrchestratorError::response(Box::new(Unprintable));
    assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503");
    let error = OrchestratorError::timeout(Box::new(Unprintable));
    assert_eq!(retry_summary(&error, Some(&response)), "Timeout");
    let error = OrchestratorError::other(Unprintable);
    assert_eq!(retry_summary(&error, Some(&response)), "Other");
    let metadata = ErrorMetadata::builder()
        .code("SlowDown")
        .custom("aws_request_id", "metadata-request-id")
        .build();
    let error = OrchestratorError::operation(Error::erase(metadata));
    let response = HttpResponse::new(503.try_into().expect("status"), SdkBody::empty());
    assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
}

#[tokio::test]
async fn each_retry_reports_the_previous_failure_and_early_final_failure_is_counted() {
    let (operation, output, _) = operation(
        [Reply::Io, Reply::Status(503), Reply::Status(400)],
        5,
        RequestSecret::None,
    );
    operation
        .invoke(())
        .await
        .expect_err("third failure is non-retryable");
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/5 after {secs}s): connector io error (ConnectionReset)",
            "  Retrying test.DescribeThing (SDK attempt 3/5 after {secs}s): HTTP 503",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ],
        0.05..=2.0,
    );
}

#[test]
fn retry_error_codes_require_the_complete_bounded_ascii_shape() {
    let response = HttpResponse::new(503.try_into().expect("status"), SdkBody::empty());
    for code in [
        "A".to_owned(),
        ":".to_owned(),
        "Az09.:_-".to_owned(),
        "A".repeat(64),
    ] {
        let error = OrchestratorError::operation(Error::erase(
            ErrorMetadata::builder().code(&code).build(),
        ));
        assert_eq!(
            retry_summary(&error, Some(&response)),
            format!("HTTP 503 {code}")
        );
    }
    for code in [
        "".to_owned(),
        "A".repeat(65),
        "a code".to_owned(),
        "code=value".to_owned(),
        "code/value".to_owned(),
        "code\n".to_owned(),
        "code\r".to_owned(),
        "code\0".to_owned(),
        "café".to_owned(),
    ] {
        let error = OrchestratorError::operation(Error::erase(
            ErrorMetadata::builder().code(&code).build(),
        ));
        assert_eq!(
            retry_summary(&error, Some(&response)),
            "HTTP 503",
            "invalid code must be omitted in full"
        );
    }
}

#[test]
fn retry_summaries_omit_request_ids_from_headers_and_metadata() {
    for request_id in [
        "valid-request-id".to_owned(),
        "Az09+/=_-".to_owned(),
        "A".repeat(128),
        "".to_owned(),
        "A".repeat(129),
        "an id".to_owned(),
        "id:value".to_owned(),
        "id.value".to_owned(),
        "id\n".to_owned(),
        "id\r".to_owned(),
        "id\0".to_owned(),
        "café".to_owned(),
    ] {
        let error = OrchestratorError::operation(Error::erase(
            ErrorMetadata::builder()
                .code("SlowDown")
                .custom("aws_request_id", &request_id)
                .build(),
        ));
        let response = HttpResponse::new(503.try_into().expect("status"), SdkBody::empty());
        assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
    }
    let error = OrchestratorError::operation(Error::erase(
        ErrorMetadata::builder().code("SlowDown").build(),
    ));
    for request_id in [
        "valid-request-id".to_owned(),
        "A".repeat(128),
        "A".repeat(129),
    ] {
        let mut response = http_response(503);
        response
            .headers_mut()
            .insert("x-amzn-requestid", request_id.clone());
        assert_eq!(retry_summary(&error, Some(&response)), "HTTP 503 SlowDown");
        assert!(
            rendered_failure(&error, Some(&response))
                .ends_with(&format!("; status: 503; request_id: {request_id}")),
            "final rendered failures must retain the response request ID"
        );
    }
}

async fn assert_secret_is_not_printed(secret: RequestSecret, expected_request_value: &str) {
    {
        let (operation, output, script) = operation([Reply::Io, Reply::Io, Reply::Io], 3, secret);
        let error = operation.invoke(()).await.expect_err("retried then failed");
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        let mut raw_messages = Vec::new();
        while let Some(error) = source {
            raw_messages.push(error.to_string());
            source = error.source();
        }
        for location in ["display", "source"] {
            for payload in [
                format!("{expected_request_value}-{location}"),
                format!("Authorization=Bearer {expected_request_value}-bypass-{location}"),
                format!("Credential=AKID/{expected_request_value}-bypass-{location}"),
            ] {
                assert!(
                    raw_messages
                        .iter()
                        .any(|message| message.contains(&payload)),
                    "secret must actually occur in the {location} error"
                );
            }
        }
        let script = script.lock().expect("script lock");
        assert_eq!(script.requests.len(), 3);
        for request in &script.requests {
            assert!(
                request.contains(expected_request_value),
                "secret must actually reach the connector"
            );
        }
        let lines = output.lines();
        assert_lines(
            &lines,
            &[
                "  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): connector io error (ConnectionReset)",
                "  Retrying test.DescribeThing (SDK attempt 3/3 after {secs}s): connector io error (ConnectionReset)",
                "  Gave up test.DescribeThing after 3 SDK attempts",
            ],
            0.05..=2.0,
        );
        let values = [
            AUTH_SIGNATURE,
            AUTH_CREDENTIAL,
            SESSION_TOKEN,
            QUERY_SIGNATURE,
            QUERY_CREDENTIAL,
            QUERY_TOKEN,
            OTHER_QUERY_VALUE,
        ];
        for line in &lines {
            for value in values {
                assert!(!line.contains(value), "retry output leaked {value}");
            }
            assert!(!line.contains("Authorization="));
            assert!(!line.contains("Credential="));
            assert!(!line.contains("connection reset"));
            assert!(!line.contains('?'));
        }
        // Final free-text rendering retains its existing best-effort redaction.
        // Check only the covered payloads, not the newly introduced bypass forms.
        let final_failure = script
            .final_failure
            .as_ref()
            .expect("final rendered failure");
        for location in ["display", "source"] {
            for value in values {
                assert!(
                    !final_failure.contains(&format!("{value}-{location}")),
                    "final rendering leaked a covered {location} payload"
                );
            }
        }
        assert!(
            !final_failure.contains('?'),
            "URL queries must remain redacted"
        );
        assert!(final_failure.contains("connection reset"));
    }

    // Both fields are untrusted even after the SDK parsed them into metadata.
    // These payloads have the secret's header/query syntax, outside the allowed shapes.
    let payload = match secret {
        RequestSecret::None => unreachable!("secret tests always inject a secret"),
        RequestSecret::Authorization => format!("Authorization=Bearer {expected_request_value}"),
        RequestSecret::SessionToken => format!("X-Amz-Security-Token: {expected_request_value}"),
        RequestSecret::QuerySignature => {
            format!("https://example.test/?X-Amz-Signature={expected_request_value}")
        }
    };
    let (operation, output, script) = operation(
        [
            Reply::ResponseFields {
                code: payload.clone(),
                request_id: "valid-request-id".to_owned(),
            },
            Reply::ResponseFields {
                code: "SlowDown".to_owned(),
                request_id: payload.clone(),
            },
            Reply::ResponseFields {
                code: "SlowDown".to_owned(),
                request_id: "valid-request-id".to_owned(),
            },
            Reply::Status(400),
        ],
        4,
        secret,
    );
    operation
        .invoke(())
        .await
        .expect_err("retried HTTP failures");
    let script = script.lock().expect("script lock");
    assert_eq!(script.requests.len(), 4);
    assert_eq!(
        script.response_fields,
        [
            (payload.clone(), "valid-request-id".to_owned()),
            ("SlowDown".to_owned(), payload),
            ("SlowDown".to_owned(), "valid-request-id".to_owned()),
        ]
    );
    let lines = output.lines();
    for line in &lines {
        assert!(
            !line.contains(expected_request_value),
            "response metadata leaked a secret"
        );
        assert!(
            !line.contains("valid-request-id"),
            "valid request IDs must also be absent"
        );
        assert!(
            !line.contains("request_id:"),
            "retry output must not include a request ID field"
        );
    }
    assert_lines(
        &lines,
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/4 after {secs}s): HTTP 503",
            "  Retrying test.DescribeThing (SDK attempt 3/4 after {secs}s): HTTP 503 SlowDown",
            "  Retrying test.DescribeThing (SDK attempt 4/4 after {secs}s): HTTP 503 SlowDown",
            "  Gave up test.DescribeThing after 4 SDK attempts",
        ],
        0.05..=2.0,
    );
}

#[tokio::test]
async fn authorization_signature_is_not_printed() {
    assert_secret_is_not_printed(RequestSecret::Authorization, AUTH_SIGNATURE).await;
}

#[tokio::test]
async fn security_session_token_is_not_printed() {
    assert_secret_is_not_printed(RequestSecret::SessionToken, SESSION_TOKEN).await;
}

#[tokio::test]
async fn presigned_query_signature_is_not_printed() {
    assert_secret_is_not_printed(RequestSecret::QuerySignature, QUERY_SIGNATURE).await;
}

#[test]
fn redacts_sensitive_values_case_insensitively_and_removes_all_url_queries() {
    for (message, expected) in [
        (
            "X-Amz-Signature=secret&other=keep",
            "X-Amz-Signature=[REDACTED]&other=keep",
        ),
        (
            "x-amz-security-token: \"token with spaces\"",
            "x-amz-security-token: [REDACTED]",
        ),
        (
            "X-AMZ-CREDENTIAL='AKID/date/region'",
            "X-AMZ-CREDENTIAL=[REDACTED]",
        ),
        (
            "AWS4-HMAC-SHA256 SignedHeaders=host, sIgNaTuRe=secret",
            "AWS4-HMAC-SHA256 SignedHeaders=host, sIgNaTuRe=[REDACTED]",
        ),
        (
            "failed; aUtHoRiZaTiOn: Bearer secret credential\nnext error",
            "failed; aUtHoRiZaTiOn: [REDACTED]\nnext error",
        ),
        (
            "\"X-Amz-Security-Token\": \"secret value\"",
            "\"X-Amz-Security-Token\": [REDACTED]",
        ),
        (
            "GET https://example.test/path?ordinary=secret and HTTP://other.test/?x=hidden",
            "GET https://example.test/path and HTTP://other.test/",
        ),
        (
            "s3://example.test/key?versionId=secret#fragment",
            "s3://example.test/key",
        ),
        (
            "ordinary error without secrets",
            "ordinary error without secrets",
        ),
    ] {
        assert_eq!(error_message(&std::io::Error::other(message)), expected);
    }
}

// Paused Tokio time exercises the SDK timers and counters deterministically.
// These cases check seconds syntax only; real-clock timing is checked above.
#[tokio::test(start_paused = true)]
async fn sdk_attempt_timeout_is_retried_with_the_next_attempt_number() {
    let (operation, output, script) = operation_with_timeouts(
        [Reply::Stall, Reply::Status(200)],
        3,
        RequestSecret::None,
        TimeoutConfig::builder()
            .operation_attempt_timeout(Duration::from_millis(50))
            .operation_timeout(Duration::from_secs(5))
            .build(),
    );
    operation
        .invoke(())
        .await
        .expect("attempt timeout should be retried");
    let script = script.lock().expect("script lock");
    assert_eq!(script.started_attempts, [1, 2]);
    assert_eq!(script.final_sdk_attempt, 2);
    assert_eq!(script.requests.len(), 2);
    assert_lines(
        &output.lines(),
        &["  Retrying test.DescribeThing (SDK attempt 2/3 after {secs}s): Timeout"],
        0.0..=2.0,
    );
}

#[tokio::test(start_paused = true)]
async fn operation_timeout_after_two_started_attempts_prints_one_gave_up() {
    let (operation, output, script) = operation_with_timeouts(
        [Reply::Io, Reply::Stall],
        5,
        RequestSecret::None,
        TimeoutConfig::builder()
            .operation_attempt_timeout(Duration::from_secs(1))
            .operation_timeout(Duration::from_millis(250))
            .build(),
    );
    let error = operation.invoke(()).await.expect_err("operation timeout");
    assert!(matches!(error, SdkError::TimeoutError(_)));
    let script = script.lock().expect("script lock");
    assert_eq!(script.started_attempts, [1, 2]);
    assert_eq!(script.final_sdk_attempt, 2);
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/5 after {secs}s): connector io error (ConnectionReset)",
            "  Gave up test.DescribeThing after 2 SDK attempts",
        ],
        0.0..=2.0,
    );
}

#[tokio::test(start_paused = true)]
async fn operation_timeout_during_backoff_does_not_count_a_phantom_third_attempt() {
    let (operation, output, script) = operation_with_timeouts(
        [Reply::Io, Reply::Io, Reply::Status(200)],
        5,
        RequestSecret::None,
        TimeoutConfig::builder()
            .operation_timeout(Duration::from_millis(250))
            .build(),
    );
    let error = operation
        .invoke(())
        .await
        .expect_err("operation timeout during retry backoff");
    assert!(matches!(error, SdkError::TimeoutError(_)));
    let script = script.lock().expect("script lock");
    assert_eq!(script.started_attempts, [1, 2]);
    assert_eq!(
        script.final_sdk_attempt, 3,
        "the SDK advances this before sleeping"
    );
    assert_eq!(script.requests.len(), 2);
    assert_lines(
        &output.lines(),
        &[
            "  Retrying test.DescribeThing (SDK attempt 2/5 after {secs}s): connector io error (ConnectionReset)",
            "  Gave up test.DescribeThing after 2 SDK attempts",
        ],
        0.0..=2.0,
    );
}

#[tokio::test(start_paused = true)]
async fn operation_timeout_before_the_first_retry_stays_silent() {
    let (operation, output, script) = operation_with_timeouts(
        [Reply::Io, Reply::Status(200)],
        3,
        RequestSecret::None,
        TimeoutConfig::builder()
            .operation_timeout(Duration::from_millis(50))
            .build(),
    );
    operation
        .invoke(())
        .await
        .expect_err("operation timeout during first backoff");
    let script = script.lock().expect("script lock");
    assert_eq!(script.started_attempts, [1]);
    assert_eq!(script.final_sdk_attempt, 2);
    assert!(output.lines().is_empty());
}
