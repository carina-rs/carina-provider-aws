//! Exercise the SDK's retry loop, clock, config bag and finalizers together.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aws_smithy_async::rt::sleep::{AsyncSleep, Sleep};
use aws_smithy_async::test_util::{InstantSleep, ManualTimeSource, instant_time_and_sleep};
use aws_smithy_runtime::client::orchestrator::operation::Operation;
use aws_smithy_runtime::client::retries::classifiers::{
    HttpStatusCodeClassifier, TransientErrorClassifier,
};
use aws_smithy_runtime_api::client::http::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse, OrchestratorError};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_types::body::SdkBody;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;

use super::{Idempotency, ResendGuard, RetryOutput};

#[derive(Clone, Debug, Default)]
struct CapturedOutput(Arc<Mutex<Vec<String>>>);

impl CapturedOutput {
    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("output lock").clone()
    }
}

impl RetryOutput for CapturedOutput {
    fn write_line(&self, line: &str) {
        self.0.lock().expect("output lock").push(line.to_owned());
    }
}

#[derive(Debug)]
struct Oversleep(InstantSleep);

impl AsyncSleep for Oversleep {
    fn sleep(&self, duration: Duration) -> Sleep {
        // The observed wait must include scheduling delay, rather than just
        // reporting the retry strategy's requested one- or two-second backoff.
        self.0.sleep(duration + Duration::from_millis(200))
    }
}

#[derive(Debug)]
enum Reply {
    Io,
    User,
    Status(u16),
}

struct ConnectionReset(HttpRequest);

impl fmt::Debug for ConnectionReset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A connector's Debug may retain a signed request. A retry logger that
        // uses DisplayErrorContext (including its Debug tail) would leak it.
        f.debug_struct("ConnectionReset")
            .field("uri", &self.0.uri())
            .field("authorization", &self.0.headers().get("authorization"))
            .field(
                "session_token",
                &self.0.headers().get("x-amz-security-token"),
            )
            .finish()
    }
}

impl fmt::Display for ConnectionReset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("connection reset")
    }
}

impl std::error::Error for ConnectionReset {}

#[derive(Debug)]
struct ServiceFailure;

impl fmt::Display for ServiceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("service unavailable")
    }
}

impl std::error::Error for ServiceFailure {}

#[derive(Debug, Default)]
struct Script {
    replies: VecDeque<Reply>,
    requests: Vec<String>,
}

#[derive(Clone, Debug)]
struct Connector {
    script: Arc<Mutex<Script>>,
    time: ManualTimeSource,
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
        // Attempt execution must not be included in the reported wait.
        self.time.advance(Duration::from_secs(7));
        HttpConnectorFuture::ready(match reply {
            Reply::Io => Err(ConnectorError::io(Box::new(ConnectionReset(request)))),
            Reply::User => Err(ConnectorError::user("invalid request".into())),
            Reply::Status(status) => {
                let mut response =
                    HttpResponse::new(status.try_into().expect("valid status"), SdkBody::empty());
                response
                    .headers_mut()
                    .insert("x-amzn-requestid", "response-id-must-not-be-logged");
                Ok(response)
            }
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum RequestSecret {
    None,
    Authorization,
    SessionToken,
    QuerySignature,
}

const AUTH_SIGNATURE: &str = "authorization-signature-must-stay-secret";
const SESSION_TOKEN: &str = "session-token-must-stay-secret";
const QUERY_SIGNATURE: &str = "query-signature-must-stay-secret";

fn operation(
    replies: impl IntoIterator<Item = Reply>,
    max_attempts: u32,
    secret: RequestSecret,
) -> (
    Operation<(), (), ServiceFailure>,
    CapturedOutput,
    Arc<Mutex<Script>>,
) {
    let output = CapturedOutput::default();
    let guard = ResendGuard {
        lookup: |_| Idempotency::ReadOnly,
        output: Box::new(output.clone()),
    };
    let (time, sleep) = instant_time_and_sleep(SystemTime::UNIX_EPOCH);
    let script = Arc::new(Mutex::new(Script {
        replies: replies.into_iter().collect(),
        requests: Vec::new(),
    }));
    let connector = Connector {
        script: script.clone(),
        time: time.clone(),
    };
    let operation = Operation::builder()
        .service_name("test")
        .operation_name("DescribeThing")
        .http_client(http_client_fn(move |_, _| SharedHttpConnector::new(connector.clone())))
        .endpoint_url("https://example.test")
        .no_auth()
        .standard_retry(&RetryConfig::standard()
            .with_max_attempts(max_attempts)
            .with_initial_backoff(Duration::from_secs(1))
            .with_use_static_exponential_base(true))
        .retry_classifier(TransientErrorClassifier::<ServiceFailure>::new())
        .retry_classifier(HttpStatusCodeClassifier::default())
        .timeout_config(TimeoutConfig::disabled())
        .sleep_impl(Oversleep(sleep))
        .time_source(time)
        .interceptor(guard)
        .serializer(move |(): ()| {
            let mut request = HttpRequest::empty();
            match secret {
                RequestSecret::None => {}
                RequestSecret::Authorization => {
                    request.headers_mut().insert("Authorization", format!(
                        "AWS4-HMAC-SHA256 Credential=test/region/service/aws4_request, SignedHeaders=host, Signature={AUTH_SIGNATURE}"
                    ));
                }
                RequestSecret::SessionToken => {
                    request.headers_mut().insert("X-Amz-Security-Token", SESSION_TOKEN);
                }
                RequestSecret::QuerySignature => {
                    request.set_uri(format!("/?X-Amz-Signature={QUERY_SIGNATURE}"))?;
                }
            }
            Ok(request)
        })
        .deserializer(|response| {
            if response.status().is_success() {
                Ok(())
            } else {
                Err(OrchestratorError::operation(ServiceFailure))
            }
        })
        .build();
    (operation, output, script)
}

#[tokio::test]
async fn retry_line_uses_sdk_counts_and_measured_wait() {
    let (operation, output, script) =
        operation([Reply::Io, Reply::Status(200)], 4, RequestSecret::None);
    operation.invoke(()).await.expect("retry succeeds");
    assert_eq!(script.lock().expect("script lock").requests.len(), 2);
    assert_eq!(
        output.lines(),
        [
            "  Retrying test.DescribeThing (SDK attempt 2/4 after 1.2s): connector error: io error: connection reset",
        ]
    );
}

#[tokio::test]
async fn exhausted_retries_print_gave_up_after_three_attempts() {
    let (operation, output, script) =
        operation([Reply::Io, Reply::Io, Reply::Io], 3, RequestSecret::None);
    operation.invoke(()).await.expect_err("retries exhausted");
    assert_eq!(script.lock().expect("script lock").requests.len(), 3);
    let lines = output.lines();
    assert_eq!(
        lines,
        [
            "  Retrying test.DescribeThing (SDK attempt 2/3 after 1.2s): connector error: io error: connection reset",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after 2.2s): connector error: io error: connection reset",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ]
    );
    // Print the injected sink's actual lines for the issue's output examples.
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
    assert_eq!(
        output.lines(),
        [
            "  Retrying test.DescribeThing (SDK attempt 2/3 after 1.2s): connector error: io error: connection reset",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after 2.2s): connector error: io error: connection reset",
        ]
    );
}

#[tokio::test]
async fn service_failures_keep_their_type_and_print_message_only() {
    let (operation, output, _) = operation(
        [Reply::Status(503), Reply::Status(503), Reply::Status(503)],
        3,
        RequestSecret::None,
    );
    let error = operation.invoke(()).await.expect_err("retries exhausted");
    assert!(
        error.as_service_error().is_some(),
        "preserve the typed service error"
    );
    assert_eq!(
        output.lines(),
        [
            "  Retrying test.DescribeThing (SDK attempt 2/3 after 1.2s): operation error: service unavailable",
            "  Retrying test.DescribeThing (SDK attempt 3/3 after 2.2s): operation error: service unavailable",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ]
    );
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
    assert_eq!(
        output.lines(),
        [
            "  Retrying test.DescribeThing (SDK attempt 2/5 after 1.2s): connector error: io error: connection reset",
            "  Retrying test.DescribeThing (SDK attempt 3/5 after 2.2s): operation error: service unavailable",
            "  Gave up test.DescribeThing after 3 SDK attempts",
        ]
    );
}

async fn assert_secret_is_not_printed(secret: RequestSecret, expected_request_value: &str) {
    let (operation, output, script) =
        operation([Reply::Io, Reply::Io, Reply::Status(400)], 3, secret);
    operation.invoke(()).await.expect_err("retried then failed");
    let script = script.lock().expect("script lock");
    assert_eq!(script.requests.len(), 3);
    for request in &script.requests {
        assert!(
            request.contains(expected_request_value),
            "secret must actually reach the connector"
        );
    }
    let lines = output.lines();
    assert_eq!(lines.len(), 3, "two retries and one final failure line");
    for line in lines {
        for forbidden in [
            AUTH_SIGNATURE,
            SESSION_TOKEN,
            QUERY_SIGNATURE,
            "Authorization",
            "authorization",
            "Signature=",
            "X-Amz-Security-Token",
            "x-amz-security-token",
            "X-Amz-Signature",
            "response-id-must-not-be-logged",
        ] {
            assert!(!line.contains(forbidden), "retry output leaked {forbidden}");
        }
    }
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
