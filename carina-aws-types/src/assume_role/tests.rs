use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aws_credential_types::credential_feature::AwsCredentialFeature;
use aws_credential_types::credential_fn::provide_credentials_fn;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::{Credentials, provider::ProvideCredentials};
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_async::test_util::ManualTimeSource;
use aws_smithy_runtime::client::identity::IdentityCache;
use aws_smithy_runtime_api::client::http::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_types::body::SdkBody;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_types::{SdkConfig, region::Region};
use carina_plugin_sdk::wasi_http::{SendPhase, transport_connector_error_for_test};
use futures::{FutureExt, channel::oneshot};

use super::*;
use crate::Transience;

const NOW: u64 = 1_234_567_000;

#[derive(Debug)]
enum Reply {
    ConnectError,
    ResponseTimeout,
    Response(Box<HttpResponse>),
    Pending(oneshot::Receiver<Result<HttpResponse, ConnectorError>>),
}

#[derive(Debug)]
struct Request {
    uri: String,
    body: String,
    authorization: String,
    token: String,
}

#[derive(Debug, Default)]
struct Script {
    replies: VecDeque<Reply>,
    requests: Vec<Request>,
}

#[derive(Clone, Debug)]
struct Connector(Arc<Mutex<Script>>);

impl Connector {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self(Arc::new(Mutex::new(Script {
            replies: replies.into_iter().collect(),
            requests: Vec::new(),
        })))
    }
}

impl HttpConnector for Connector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let mut script = self.0.lock().unwrap();
        script.requests.push(Request {
            uri: request.uri().to_string(),
            body: String::from_utf8(request.body().bytes().unwrap().to_vec()).unwrap(),
            authorization: request.headers().get("authorization").unwrap().to_owned(),
            token: request
                .headers()
                .get("x-amz-security-token")
                .unwrap_or_default()
                .to_owned(),
        });
        let reply = script.replies.pop_front().expect("unexpected HTTP attempt");
        HttpConnectorFuture::new(async move {
            match reply {
                Reply::ConnectError => Err(transport_connector_error_for_test(SendPhase::NotSent)),
                Reply::ResponseTimeout => Err(ConnectorError::timeout(
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "response timed out").into(),
                )),
                Reply::Response(response) => Ok(*response),
                Reply::Pending(response) => response.await.expect("response sender retained"),
            }
        })
    }
}

fn complete_role(sender: oneshot::Sender<Result<HttpResponse, ConnectorError>>, key: &str) {
    let Reply::Response(response) = role_response(key, NOW + 3600) else {
        panic!("role_response must provide an HTTP response");
    };
    sender
        .send(Ok(*response))
        .expect("in-flight request retained");
}

#[tokio::test]
async fn regression_dropped_load_keeps_the_transmitted_assume_role_in_flight() {
    let (complete, response) = oneshot::channel();
    let connector = Connector::new([
        Reply::Pending(response),
        role_response("DUPLICATE", NOW + 3600),
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let provider = provider(&config(&connector, &clock));
    let mut first = Box::pin(provider.provide_credentials());
    assert!(futures::poll!(first.as_mut()).is_pending());
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    drop(first);

    let mut second = Box::pin(provider.provide_credentials());
    let second_poll = futures::poll!(second.as_mut());
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert!(second_poll.is_pending());
    complete_role(complete, "ORIGINAL");
    assert_eq!(second.await.unwrap().access_key_id(), "ORIGINAL");
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
}

#[tokio::test]
async fn regression_concurrent_loads_share_success_and_then_clear_the_slot() {
    let (complete, response) = oneshot::channel();
    let connector = Connector::new([Reply::Pending(response), role_response("NEXT", NOW + 7200)]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let provider = provider(&config(&connector, &clock));
    let mut first = Box::pin(provider.provide_credentials());
    let mut second = Box::pin(provider.provide_credentials());
    assert!(futures::poll!(first.as_mut()).is_pending());
    let second_poll = futures::poll!(second.as_mut());
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert!(second_poll.is_pending());

    complete_role(complete, "SHARED");
    assert_eq!(first.await.unwrap().access_key_id(), "SHARED");
    assert_eq!(second.await.unwrap().access_key_id(), "SHARED");
    assert_eq!(
        provider
            .provide_credentials()
            .await
            .unwrap()
            .access_key_id(),
        "NEXT"
    );
    assert_eq!(connector.0.lock().unwrap().requests.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn regression_concurrent_loads_share_failure_and_then_clear_the_slot() {
    let (complete, response) = oneshot::channel();
    let connector = Connector::new([Reply::Pending(response), role_response("NEXT", NOW + 7200)]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let provider = provider(&config(&connector, &clock));
    let mut first = Box::pin(provider.provide_credentials());
    let mut second = Box::pin(provider.provide_credentials());
    assert!(futures::poll!(first.as_mut()).is_pending());
    let second_poll = futures::poll!(second.as_mut());
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert!(second_poll.is_pending());

    complete
        .send(Err(ConnectorError::timeout("response timed out".into())))
        .unwrap();
    let first = first.await.unwrap_err();
    let second = second.await.unwrap_err();
    assert!(matches!(
        failure(&first).classification(),
        SendClassification::MaybeSent { .. }
    ));
    assert_eq!(
        failure(&first).classification(),
        failure(&second).classification()
    );
    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert_eq!(
        provider
            .provide_credentials()
            .await
            .unwrap()
            .access_key_id(),
        "NEXT"
    );
    assert_eq!(connector.0.lock().unwrap().requests.len(), 2);
}

#[derive(Debug)]
struct DisableGuardAfterAttempt;

impl aws_smithy_runtime_api::client::interceptors::Intercept for DisableGuardAfterAttempt {
    fn name(&self) -> &'static str {
        "test.DisableGuardAfterAttempt"
    }

    fn read_after_attempt(
        &self,
        _context: &aws_smithy_runtime_api::client::interceptors::context::FinalizerInterceptorContextRef<'_>,
        _runtime_components: &aws_smithy_runtime_api::client::runtime_components::RuntimeComponents,
        cfg: &mut aws_smithy_types::config_bag::ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::box_error::BoxError> {
        cfg.interceptor_state().store_put(
            aws_smithy_runtime_api::client::interceptors::disable_interceptor::<crate::ResendGuard>(
                "try to bypass the resend guard after transmission",
            ),
        );
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn regression_operation_cannot_disable_the_assume_role_resend_guard() {
    let connector = Connector::new([
        Reply::ResponseTimeout,
        role_response("MUSTNOTBEISSUED", NOW + 3600),
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let provider = source(&config(&connector, &clock));
    let send = provider
        .request
        .clone()
        .customize()
        .interceptor(DisableGuardAfterAttempt)
        .send();
    let result = std::panic::AssertUnwindSafe(send).catch_unwind().await;

    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    // Smithy's permanent interceptors reject disable markers with a diagnostic
    // panic in debug builds. Release builds must continue through the guard's veto.
    #[cfg(debug_assertions)]
    {
        let panic = result.expect_err("disabling a permanent interceptor must be rejected");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("diagnostic panic");
        assert!(
            message.contains("attempted to disable permanent interceptor `carina.ResendGuard`")
        );
    }
    #[cfg(not(debug_assertions))]
    {
        let error = result
            .expect("release interceptors must not panic")
            .unwrap_err();
        assert!(matches!(
            SendClassification::of(&error),
            SendClassification::MaybeSent { .. }
        ));
    }
}

fn response(status: u16, body: String) -> Reply {
    let mut response = HttpResponse::new(status.try_into().unwrap(), SdkBody::from(body));
    response.headers_mut().insert("content-type", "text/xml");
    Reply::Response(Box::new(response))
}

fn role_response(key: &str, expires: u64) -> Reply {
    let expiration = aws_smithy_types::DateTime::from_secs(expires as i64)
        .fmt(aws_smithy_types::date_time::Format::DateTime)
        .unwrap();
    response(
        200,
        format!(
            "<AssumeRoleResponse><AssumeRoleResult><Credentials><AccessKeyId>{key}</AccessKeyId><SecretAccessKey>assumed-secret</SecretAccessKey><SessionToken>assumed-token</SessionToken><Expiration>{expiration}</Expiration></Credentials><AssumedRoleUser><AssumedRoleId>ROLE:session</AssumedRoleId><Arn>arn:aws:sts::222222222222:assumed-role/test/session</Arn></AssumedRoleUser></AssumeRoleResult></AssumeRoleResponse>"
        ),
    )
}

fn caller_response() -> Reply {
    response(200, "<GetCallerIdentityResponse><GetCallerIdentityResult><Account>222222222222</Account><Arn>arn:aws:sts::222222222222:assumed-role/test/session</Arn><UserId>ROLE:session</UserId></GetCallerIdentityResult></GetCallerIdentityResponse>".into())
}

fn config(connector: &Connector, clock: &ManualTimeSource) -> crate::AwsSdkConfig {
    let connector = connector.clone();
    crate::AwsSdkConfig::for_test(
        SdkConfig::builder()
            .region(Region::new("eu-west-1"))
            .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
                "HOSTKEY",
                "host-secret",
                Some("host-token".into()),
                None,
                "host-test",
            )))
            .http_client(http_client_fn(move |_, _| {
                SharedHttpConnector::new(connector.clone())
            }))
            .time_source(clock.clone())
            .sleep_impl(TokioSleep::new())
            .retry_config(RetryConfig::standard().with_max_attempts(3))
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_attempt_timeout(Duration::from_secs(30))
                    .operation_timeout(Duration::from_secs(60))
                    .build(),
            )
            .identity_cache(IdentityCache::lazy().build())
            .build(),
    )
}

fn source(config: &crate::AwsSdkConfig) -> GuardedAssumeRoleProvider {
    GuardedAssumeRoleProvider::new(
        config,
        "arn:aws:iam::222222222222:role/test".into(),
        Some("test-session".into()),
        Some("external-id".into()),
        Some(Duration::from_secs(1800)),
    )
}

fn provider(config: &crate::AwsSdkConfig) -> SharedCredentialsProvider {
    config
        .clone()
        .with_source(source(config))
        .sdk_config()
        .credentials_provider()
        .unwrap()
}

#[tokio::test]
async fn session_duration_overflow_is_invalid_configuration_without_sending() {
    for seconds in [2_147_483_648, 4_294_967_297, u64::MAX] {
        let connector = Connector::new([]);
        let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
        let base = config(&connector, &clock);
        let source = GuardedAssumeRoleProvider::new(
            &base,
            "arn:aws:iam::222222222222:role/test".into(),
            Some("test-session".into()),
            None,
            Some(Duration::from_secs(seconds)),
        );
        let provider = base
            .with_source(source)
            .sdk_config()
            .credentials_provider()
            .unwrap();
        let error = provider
            .provide_credentials()
            .await
            .expect_err("duration must not wrap");
        assert!(
            matches!(error, CredentialsError::InvalidConfiguration(_)),
            "{error:?}"
        );
        assert!(connector.0.lock().unwrap().requests.is_empty());
    }
}

fn failure(error: &CredentialsError) -> &AssumeRoleFailure {
    error
        .source()
        .unwrap()
        .downcast_ref::<AssumeRoleFailure>()
        .expect("the credentials error retains the STS failure and classification")
}

fn assert_secret_free(error: &CredentialsError, secrets: &[&str]) {
    let mut renderings = vec![
        format!("{error:?}"),
        error.to_string(),
        DisplayErrorContext(error).to_string(),
    ];
    let mut source = error.source();
    while let Some(error) = source {
        renderings.push(format!("{error:?}"));
        renderings.push(error.to_string());
        source = error.source();
    }
    for rendering in renderings {
        for secret in secrets {
            assert!(
                !rendering.contains(secret),
                "credentials error exposed a secret: {rendering}"
            );
        }
    }
}

#[tokio::test]
async fn review_round2_malformed_success_response_does_not_expose_credentials() {
    let secret = "distinctive-sts-secret-access-key-round2";
    let token = "distinctive-sts-session-token-round2";
    let connector = Connector::new([response(
        200,
        format!(
            "<AssumeRoleResponse><AssumeRoleResult><Credentials>\
             <AccessKeyId>ASSUMED</AccessKeyId>\
             <SecretAccessKey>{secret}</SecretAccessKey>\
             <SessionToken>{token}</SessionToken>\
             <Expiration>invalid-timestamp</Expiration>\
             </Credentials></AssumeRoleResult></AssumeRoleResponse>"
        ),
    )]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let error = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap_err();

    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert_secret_free(&error, &[secret, token]);
    assert!(matches!(
        failure(&error).classification(),
        SendClassification::MaybeSent { .. }
    ));
    let summary = failure(&error).to_string();
    // Generated STS XML deserialization failures are unhandled service errors.
    assert!(summary.contains("ServiceError"), "{summary}");
    assert!(summary.contains("HTTP 200"), "{summary}");
}

#[test]
fn review_round2_response_error_does_not_retain_raw_credentials() {
    let secret = "distinctive-response-error-secret-access-key-round2";
    let token = "distinctive-response-error-session-token-round2";
    let raw = HttpResponse::new(
        200.try_into().unwrap(),
        SdkBody::from(format!(
            "<SecretAccessKey>{secret}</SecretAccessKey><SessionToken>{token}</SessionToken>"
        )),
    );
    let sdk_error = SdkError::response_error("STS response could not be parsed", raw);
    let error = AssumeRoleFailure::from_sdk_error(sdk_error).into_credentials_error();

    assert_secret_free(&error, &[secret, token]);
    assert!(matches!(
        failure(&error).classification(),
        SendClassification::MaybeSent { .. }
    ));
    let summary = failure(&error).to_string();
    assert!(summary.contains("ResponseError"), "{summary}");
    assert!(summary.contains("HTTP 200"), "{summary}");
}

#[tokio::test]
async fn review_round2_service_error_keeps_code_without_raw_response_or_message() {
    let secret = "distinctive-service-error-secret-access-key-round2";
    let token = "distinctive-service-error-session-token-round2";
    let connector = Connector::new([response(
        403,
        format!(
            "<ErrorResponse><Error><Code>AccessDenied</Code>\
             <Message>{secret} {token}</Message></Error></ErrorResponse>"
        ),
    )]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let error = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap_err();

    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert_secret_free(&error, &[secret, token]);
    let summary = failure(&error).to_string();
    for expected in ["ServiceError", "HTTP 403", "AccessDenied"] {
        assert!(summary.contains(expected), "{summary}");
    }
}

#[tokio::test(start_paused = true)]
async fn response_timeout_after_transmit_is_not_resent_and_surfaces_maybe_sent() {
    let connector = Connector::new([
        Reply::ResponseTimeout,
        role_response("MUSTNOTBEISSUED", NOW + 3600),
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let error = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap_err();

    assert_eq!(connector.0.lock().unwrap().requests.len(), 1);
    assert!(matches!(
        failure(&error).classification(),
        SendClassification::MaybeSent { .. }
    ));
    assert!(failure(&error).to_string().contains("MaybeSent"));
    assert!(failure(&error).source().is_none());
}

#[tokio::test(start_paused = true)]
async fn not_sent_connect_failure_is_retried() {
    let connector = Connector::new([Reply::ConnectError, role_response("ASSUMED", NOW + 3600)]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let credentials = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap();

    assert_eq!(connector.0.lock().unwrap().requests.len(), 2);
    assert_eq!(credentials.access_key_id(), "ASSUMED");
    assert_eq!(credentials.secret_access_key(), "assumed-secret");
    assert_eq!(credentials.session_token(), Some("assumed-token"));
    assert_eq!(credentials.account_id().unwrap().as_str(), "222222222222");
    assert_eq!(
        credentials.expiry(),
        Some(UNIX_EPOCH + Duration::from_secs(NOW + 3600))
    );
    assert_eq!(
        credentials
            .get_property::<Vec<AwsCredentialFeature>>()
            .unwrap(),
        &[AwsCredentialFeature::CredentialsStsAssumeRole]
    );
}

#[tokio::test(start_paused = true)]
async fn exhausted_not_sent_failures_retain_classification_and_retry_bounds() {
    let connector = Connector::new([
        Reply::ConnectError,
        Reply::ConnectError,
        Reply::ConnectError,
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let error = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap_err();

    assert_eq!(connector.0.lock().unwrap().requests.len(), 3);
    assert_eq!(
        failure(&error).classification(),
        // The host test helper proves NotSent but carries no wasi error code;
        // the existing transience policy therefore conservatively says Permanent.
        SendClassification::NotSent(Transience::Permanent)
    );
}

#[tokio::test]
async fn each_load_preserves_options_region_and_refreshes_base_credentials() {
    let connector = Connector::new([
        role_response("FIRST", NOW + 3600),
        role_response("SECOND", NOW + 7200),
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let loads = Arc::new(AtomicUsize::new(0));
    let base = {
        let loads = loads.clone();
        provide_credentials_fn(move || {
            let call = loads.fetch_add(1, Ordering::SeqCst);
            async move {
                Ok(Credentials::new(
                    format!("HOST{call}"),
                    "host-secret",
                    Some(format!("host-token-{call}")),
                    Some(UNIX_EPOCH + Duration::from_secs(NOW + 86400)),
                    "refreshing-host-test",
                ))
            }
        })
    };
    let base = Arc::new(base);
    let config =
        config(&connector, &clock).with_source(crate::AwsCredentialsSource::new(move || {
            let base = base.clone();
            async move { base.provide_credentials().await }
        }));
    let provider = provider(&config);
    let first = provider.provide_credentials().await.unwrap();
    let second = provider.provide_credentials().await.unwrap();

    assert_eq!(first.access_key_id(), "FIRST");
    assert_eq!(second.access_key_id(), "SECOND");
    assert_eq!(loads.load(Ordering::SeqCst), 2);
    let script = connector.0.lock().unwrap();
    for (index, request) in script.requests.iter().enumerate() {
        assert_eq!(request.uri, "https://sts.eu-west-1.amazonaws.com/");
        assert!(
            request
                .authorization
                .contains(&format!("Credential=HOST{index}/"))
        );
        assert!(
            request
                .authorization
                .contains("/eu-west-1/sts/aws4_request")
        );
        assert_eq!(request.token, format!("host-token-{index}"));
        assert!(request.body.contains("Action=AssumeRole"));
        assert!(
            request
                .body
                .contains("RoleArn=arn%3Aaws%3Aiam%3A%3A222222222222%3Arole%2Ftest")
        );
        assert!(request.body.contains("RoleSessionName=test-session"));
        assert!(request.body.contains("ExternalId=external-id"));
        assert!(request.body.contains("DurationSeconds=1800"));
    }
}

#[tokio::test]
async fn omitted_session_name_keeps_sdk_default_and_omits_optional_fields() {
    let connector = Connector::new([role_response("ASSUMED", NOW + 3600)]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let base = config(&connector, &clock);
    let source = GuardedAssumeRoleProvider::new(
        &base,
        "arn:aws:iam::222222222222:role/test".into(),
        None,
        None,
        None,
    );
    let provider = base
        .with_source(source)
        .sdk_config()
        .credentials_provider()
        .unwrap();
    provider.provide_credentials().await.unwrap();
    let script = connector.0.lock().unwrap();
    let request = &script.requests[0];
    assert!(
        request
            .body
            .contains("RoleSessionName=assume-role-provider-1234567000000")
    );
    assert!(!request.body.contains("ExternalId"));
    assert!(!request.body.contains("DurationSeconds"));
}

#[tokio::test]
async fn service_client_caches_role_credentials_and_refreshes_at_expiry() {
    let connector = Connector::new([
        role_response("FIRST", NOW + 3600),
        caller_response(),
        caller_response(),
        role_response("SECOND", NOW + 10800),
        caller_response(),
    ]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let base = config(&connector, &clock);
    let assumed = base.clone().with_source(source(&base));
    let client = StsClient::new(&assumed, |operation| {
        if operation == "GetCallerIdentity" {
            Idempotency::ReadOnly
        } else {
            Idempotency::NotIdempotent
        }
    });
    for _call in 0..2 {
        client.inner().get_caller_identity().send().await.unwrap();
    }
    assert_eq!(connector.0.lock().unwrap().requests.len(), 3);
    clock.advance(Duration::from_secs(7200));
    client.inner().get_caller_identity().send().await.unwrap();

    let script = connector.0.lock().unwrap();
    assert_eq!(script.requests.len(), 5);
    assert!(script.replies.is_empty());
    assert!(
        script.requests[1]
            .authorization
            .contains("Credential=FIRST/")
    );
    assert!(
        script.requests[2]
            .authorization
            .contains("Credential=FIRST/")
    );
    assert!(
        script.requests[4]
            .authorization
            .contains("Credential=SECOND/")
    );
    for index in [0, 3] {
        assert!(
            script.requests[index]
                .authorization
                .contains("Credential=HOSTKEY/")
        );
        assert_eq!(script.requests[index].token, "host-token");
    }
}

#[tokio::test]
async fn invalid_configuration_errors_keep_the_sdk_error_category() {
    for code in ["RegionDisabledException", "MalformedPolicyDocument"] {
        let connector = Connector::new([response(
            400,
            format!(
                "<ErrorResponse><Error><Code>{code}</Code><Message>invalid configuration</Message></Error></ErrorResponse>"
            ),
        )]);
        let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
        let error = provider(&config(&connector, &clock))
            .provide_credentials()
            .await
            .unwrap_err();
        assert!(
            matches!(error, CredentialsError::InvalidConfiguration(_)),
            "{code}: {error:?}"
        );
        assert_eq!(
            failure(&error).classification(),
            SendClassification::Rejected
        );
    }
}

#[tokio::test]
async fn missing_sts_credentials_is_an_unhandled_credentials_error() {
    let connector = Connector::new([response(
        200,
        "<AssumeRoleResponse><AssumeRoleResult/></AssumeRoleResponse>".into(),
    )]);
    let clock = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(NOW));
    let error = provider(&config(&connector, &clock))
        .provide_credentials()
        .await
        .unwrap_err();
    assert!(matches!(error, CredentialsError::Unhandled(_)));
}
