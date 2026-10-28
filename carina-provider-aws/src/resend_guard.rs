//! SDK retry guard that refuses ambiguous resends of non-idempotent operations.

use std::io;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeTransmitInterceptorContextRef, FinalizerInterceptorContextMut,
    FinalizerInterceptorContextRef,
};
use aws_smithy_runtime_api::client::interceptors::error::ContextAttachedError;
use aws_smithy_runtime_api::client::orchestrator::Metadata;
use aws_smithy_runtime_api::client::orchestrator::OrchestratorError;
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};
use carina_aws_types::{Idempotency, SendOutcome};

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

/// Associates one service client's SDK metadata with generated resend policy.
#[derive(Debug)]
pub(crate) struct ResendGuard {
    lookup: fn(&str) -> Idempotency,
}

impl ResendGuard {
    pub(crate) fn new(lookup: fn(&str) -> Idempotency) -> Self {
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
        let idempotency = cfg
            .load::<Metadata>()
            .map(|metadata| (self.lookup)(metadata.name()))
            .unwrap_or(Idempotency::NotIdempotent);
        let preserve_maybe_sent = match idempotency {
            Idempotency::ReadOnly => false,
            Idempotency::Idempotent => false,
            Idempotency::IdempotencyToken => true,
            Idempotency::NotIdempotent => false,
        };

        if !preserve_maybe_sent
            || cfg.load::<MaybeSentAttempt>().is_none()
            || context
                .output_or_error()
                .is_none_or(|result| result.is_ok())
            || SendOutcome::classify_orchestrator(context.inner(), runtime_components)
                == SendOutcome::MaybeSent
        {
            return Ok(());
        }

        let Some(Err(error)) = context.output_or_error_mut() else {
            return Ok(());
        };
        let original = std::mem::replace(
            error,
            OrchestratorError::other(io::Error::other("replacing final send failure")),
        );
        *error = OrchestratorError::other(ContextAttachedError::new(
            "an earlier attempt of this send may have reached AWS",
            original,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use aws_config::Region;
    use aws_credential_types::Credentials;
    use aws_credential_types::provider::SharedCredentialsProvider;
    use aws_smithy_async::rt::sleep::TokioSleep;
    use aws_smithy_runtime_api::client::http::{
        HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
    };
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_smithy_types::retry::RetryConfig;
    use aws_smithy_types::timeout::TimeoutConfig;
    use carina_aws_types::SendOutcome;

    use crate::resend_safety_generated::ec2_idempotency;
    use crate::{AwsProvider, BoundedSdkConfig};

    use super::ResendGuard;

    #[derive(Debug)]
    enum Action {
        Io,
        Timeout,
        DelayedTimeout(Duration),
        Response {
            status: u16,
            body: &'static str,
        },
        ResponseWithRequestId {
            status: u16,
            body: &'static str,
            request_id: &'static str,
        },
    }

    #[derive(Debug, Default)]
    struct ScriptState {
        actions: VecDeque<Action>,
        requests: Vec<String>,
        completed_delayed_timeouts: usize,
    }

    #[derive(Clone, Debug)]
    struct ScriptedConnector(Arc<Mutex<ScriptState>>);

    impl ScriptedConnector {
        fn new(actions: impl IntoIterator<Item = Action>) -> Self {
            Self(Arc::new(Mutex::new(ScriptState {
                actions: actions.into_iter().collect(),
                requests: Vec::new(),
                completed_delayed_timeouts: 0,
            })))
        }

        fn requests(&self) -> Vec<String> {
            self.0.lock().expect("script lock").requests.clone()
        }

        fn completed_delayed_timeouts(&self) -> usize {
            self.0
                .lock()
                .expect("script lock")
                .completed_delayed_timeouts
        }
    }

    impl HttpConnector for ScriptedConnector {
        fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
            let uri = request.uri().to_string();
            let body = request
                .body()
                .bytes()
                .map(String::from_utf8_lossy)
                .map(|body| body.into_owned())
                .unwrap_or_default();
            let action = {
                let mut state = self.0.lock().expect("script lock");
                state.requests.push(format!("{uri}\n{body}"));
                state.actions.pop_front().expect("script exhausted")
            };
            let state = Arc::clone(&self.0);

            HttpConnectorFuture::new(async move {
                match action {
                    Action::Io => Err(ConnectorError::io(
                        std::io::Error::other("post-send I/O failure").into(),
                    )),
                    Action::Timeout => Err(ConnectorError::timeout(
                        std::io::Error::other("post-send timeout").into(),
                    )),
                    Action::DelayedTimeout(delay) => {
                        tokio::time::sleep(delay).await;
                        state
                            .lock()
                            .expect("script lock")
                            .completed_delayed_timeouts += 1;
                        Err(ConnectorError::timeout(
                            std::io::Error::other("delayed post-send timeout").into(),
                        ))
                    }
                    Action::Response { status, body } => {
                        let mut response = HttpResponse::new(
                            status.try_into().expect("valid test status"),
                            SdkBody::from(body),
                        );
                        response.headers_mut().insert("content-type", "text/xml");
                        Ok(response)
                    }
                    Action::ResponseWithRequestId {
                        status,
                        body,
                        request_id,
                    } => {
                        let mut response = HttpResponse::new(
                            status.try_into().expect("valid test status"),
                            SdkBody::from(body),
                        );
                        response.headers_mut().insert("content-type", "text/xml");
                        response
                            .headers_mut()
                            .insert("x-amzn-requestid", request_id);
                        Ok(response)
                    }
                }
            })
        }
    }

    fn provider(connector: ScriptedConnector) -> AwsProvider {
        let selected = connector.clone();
        let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
        let credentials = Credentials::new("test", "test", None, None, "resend-guard-test");
        let config = aws_config::SdkConfig::builder()
            .region(Region::new("us-east-1"))
            .endpoint_url("https://example.test")
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .http_client(http_client)
            .build();
        let bounded = BoundedSdkConfig::new(config);
        AwsProvider::from_sdk_config("us-east-1".to_string(), Vec::new(), Vec::new(), &bounded)
    }

    fn raw_ec2_client(
        connector: ScriptedConnector,
        timeout_config: TimeoutConfig,
        max_attempts: u32,
    ) -> aws_sdk_ec2::Client {
        let selected = connector.clone();
        let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
        let credentials = Credentials::new("test", "test", None, None, "resend-guard-test");
        let config = aws_config::SdkConfig::builder()
            .region(Region::new("us-east-1"))
            .endpoint_url("https://example.test")
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .http_client(http_client)
            .sleep_impl(TokioSleep::new())
            .timeout_config(timeout_config)
            .retry_config(
                RetryConfig::standard()
                    .with_max_attempts(max_attempts)
                    .with_initial_backoff(Duration::from_millis(1)),
            )
            .build();
        aws_sdk_ec2::Client::from_conf(
            aws_sdk_ec2::config::Builder::from(&config)
                .interceptor(ResendGuard::new(ec2_idempotency))
                .build(),
        )
    }

    fn ec2_client(connector: ScriptedConnector) -> aws_sdk_ec2::Client {
        raw_ec2_client(connector, TimeoutConfig::disabled(), 3)
    }

    fn ec2_client_with_attempt_timeout(
        connector: ScriptedConnector,
        operation_attempt_timeout: Duration,
    ) -> aws_sdk_ec2::Client {
        raw_ec2_client(
            connector,
            TimeoutConfig::builder()
                .operation_attempt_timeout(operation_attempt_timeout)
                .build(),
            2,
        )
    }

    const CREATE_VPC_OK: &str = r#"<CreateVpcResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
        <requestId>request-id</requestId><vpc><vpcId>vpc-123</vpcId></vpc>
    </CreateVpcResponse>"#;
    const DESCRIBE_VPCS_OK: &str = r#"<DescribeVpcsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
        <requestId>request-id</requestId><vpcSet />
    </DescribeVpcsResponse>"#;
    const CREATE_NAT_GATEWAY_OK: &str = r#"<CreateNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
        <requestId>request-id</requestId><natGateway><natGatewayId>nat-123</natGatewayId><state>pending</state></natGateway>
    </CreateNatGatewayResponse>"#;
    const THROTTLED: &str = r#"<Response><Errors><Error><Code>RequestLimitExceeded</Code>
        <Message>throttled</Message></Error></Errors><RequestID>request-id</RequestID></Response>"#;
    const INTERNAL_ERROR: &str = r#"<Response><Errors><Error><Code>InternalError</Code>
        <Message>first attempt exploded</Message></Error></Errors><RequestID>first-attempt-request-id</RequestID></Response>"#;
    const NO_SUCH_BUCKET_POLICY: &str = r#"<Error><Code>NoSuchBucketPolicy</Code>
        <Message>The bucket policy does not exist</Message><RequestId>request-id</RequestId>
        <HostId>host-id</HostId></Error>"#;

    #[tokio::test]
    async fn non_idempotent_create_vpc_is_not_resent_after_io_failure() {
        let connector = ScriptedConnector::new([
            Action::Io,
            Action::Response {
                status: 200,
                body: CREATE_VPC_OK,
            },
        ]);
        let provider = provider(connector.clone());

        let result = provider
            .ec2_client
            .create_vpc()
            .cidr_block("10.0.0.0/16")
            .send()
            .await;

        assert!(result.is_err());
        assert_eq!(connector.requests().len(), 1);
    }

    #[tokio::test]
    async fn guard_refusal_keeps_first_attempt_aws_error_details() {
        let connector = ScriptedConnector::new([
            Action::ResponseWithRequestId {
                status: 500,
                body: INTERNAL_ERROR,
                request_id: "first-attempt-request-id",
            },
            Action::Response {
                status: 200,
                body: CREATE_VPC_OK,
            },
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .ec2_client
            .create_vpc()
            .cidr_block("10.0.0.0/16")
            .send()
            .await
            .expect_err("the resend guard must refuse the ambiguous retry");
        // This is the same source-chain rendering used by provider call sites;
        // `SdkError`'s own Display intentionally prints only its outer variant.
        let rendered = crate::helpers::sdk_error_message("CreateVpc failed", &error);

        assert_eq!(connector.requests().len(), 1);
        assert!(rendered.contains("InternalError"), "{rendered}");
        assert!(rendered.contains("first attempt exploded"), "{rendered}");
        assert!(rendered.contains("first-attempt-request-id"), "{rendered}");
    }

    #[tokio::test]
    async fn readonly_describe_vpcs_is_resent_after_io_failure() {
        let connector = ScriptedConnector::new([
            Action::Io,
            Action::Response {
                status: 200,
                body: DESCRIBE_VPCS_OK,
            },
        ]);
        let provider = provider(connector.clone());

        provider
            .ec2_client
            .describe_vpcs()
            .send()
            .await
            .expect("read-only request should be retried");

        assert_eq!(connector.requests().len(), 2);
    }

    #[tokio::test]
    async fn readonly_error_after_maybe_sent_attempt_remains_a_typed_service_error() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Response {
                status: 404,
                body: NO_SUCH_BUCKET_POLICY,
            },
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .s3_client
            .get_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("the second attempt should surface NoSuchBucketPolicy");

        assert_eq!(connector.requests().len(), 2);
        let aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) = &error else {
            panic!("expected a typed service error, got {error:?}");
        };
        assert_eq!(
            ProvideErrorMetadata::code(context.err()),
            Some("NoSuchBucketPolicy")
        );
    }

    #[tokio::test]
    async fn token_create_nat_gateway_resends_the_same_token_after_timeout() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Response {
                status: 200,
                body: CREATE_NAT_GATEWAY_OK,
            },
        ]);
        let client = ec2_client(connector.clone());
        let token = "stable-client-token";

        client
            .create_nat_gateway()
            .subnet_id("subnet-123")
            .client_token(token)
            .send()
            .await
            .expect("token-bearing request should be retried");

        let requests = connector.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.contains(token)));
    }

    #[tokio::test]
    async fn token_send_keeps_maybe_sent_after_final_throttling() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Response {
                status: 429,
                body: THROTTLED,
            },
            Action::ResponseWithRequestId {
                status: 429,
                body: THROTTLED,
                request_id: "final-throttling-request-id",
            },
        ]);
        let client = ec2_client(connector.clone());

        let error = client
            .create_nat_gateway()
            .subnet_id("subnet-123")
            .client_token("stable-client-token")
            .send()
            .await
            .expect_err("all three SDK attempts should fail");
        let rendered = crate::helpers::sdk_error_message("CreateNatGateway failed", &error);

        assert_eq!(connector.requests().len(), 3);
        assert!(matches!(
            &error,
            aws_smithy_runtime_api::client::result::SdkError::ResponseError(_)
        ));
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
        assert!(
            rendered.contains("an earlier attempt of this send may have reached AWS"),
            "{rendered}"
        );
        let readable_chain = rendered
            .split(" (ResponseError")
            .next()
            .expect("rendered error has a readable source chain");
        assert!(readable_chain.contains("throttled"), "{rendered}");
        assert!(
            readable_chain.contains("final-throttling-request-id"),
            "{rendered}"
        );
        assert_eq!(
            readable_chain
                .matches("final-throttling-request-id")
                .count(),
            1,
            "the attached source must render the final failure details exactly once: {rendered}"
        );
    }

    #[tokio::test]
    async fn token_send_keeps_maybe_sent_after_sdk_attempt_timeout() {
        let connector = ScriptedConnector::new([
            Action::DelayedTimeout(Duration::from_secs(1)),
            Action::Response {
                status: 429,
                body: THROTTLED,
            },
        ]);
        let client = ec2_client_with_attempt_timeout(connector.clone(), Duration::from_millis(20));

        let error = client
            .create_nat_gateway()
            .subnet_id("subnet-123")
            .client_token("stable-client-token")
            .send()
            .await
            .expect_err("both SDK attempts should fail");

        assert_eq!(connector.requests().len(), 2);
        assert_eq!(connector.completed_delayed_timeouts(), 0);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[tokio::test]
    async fn rejected_throttled_create_vpc_is_resent() {
        let connector = ScriptedConnector::new([
            Action::Response {
                status: 503,
                body: THROTTLED,
            },
            Action::Response {
                status: 200,
                body: CREATE_VPC_OK,
            },
        ]);
        let provider = provider(connector.clone());

        provider
            .ec2_client
            .create_vpc()
            .cidr_block("10.0.0.0/16")
            .send()
            .await
            .expect("a rejected throttled request is safe to retry");

        assert_eq!(connector.requests().len(), 2);
    }
}
