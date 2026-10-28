//! SDK retry guard that refuses ambiguous resends of non-idempotent operations.

use std::io;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeTransmitInterceptorContextRef, FinalizerInterceptorContextRef,
};
use aws_smithy_runtime_api::client::orchestrator::Metadata;
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
        let previous = cfg
            .load::<PreviousAttempt>()
            .cloned()
            .unwrap_or_else(|| PreviousAttempt {
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
                return Ok(());
            }
            Some(Err(_)) | None => {
                SendOutcome::classify_orchestrator(context.inner(), runtime_components)
            }
        };
        let mut failure = context
            .output_or_error()
            .and_then(Result::err)
            .map(|error| {
                let mut rendered = error.to_string();
                let mut source = std::error::Error::source(error);
                while let Some(error) = source {
                    rendered.push_str(": ");
                    rendered.push_str(&error.to_string());
                    source = error.source();
                }
                rendered
            })
            .unwrap_or_else(|| "failure details unavailable".to_string());
        if let Some(response) = context.response() {
            failure.push_str(&format!("; status: {}", response.status().as_u16()));
            for header in [
                "x-amzn-requestid",
                "x-amzn-request-id",
                "x-amz-request-id",
                "x-amz-id-2",
            ] {
                if let Some(request_id) = response.headers().get(header) {
                    failure.push_str("; request_id: ");
                    failure.push_str(request_id);
                    break;
                }
            }
        }
        cfg.interceptor_state()
            .store_put(PreviousAttempt { outcome, failure });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use aws_config::Region;
    use aws_credential_types::Credentials;
    use aws_credential_types::provider::SharedCredentialsProvider;
    use aws_smithy_runtime_api::client::http::{
        HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
    };
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;

    use crate::{AwsProvider, BoundedSdkConfig};

    #[derive(Debug)]
    enum Action {
        Io,
        Timeout,
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
    }

    #[derive(Clone, Debug)]
    struct ScriptedConnector(Arc<Mutex<ScriptState>>);

    impl ScriptedConnector {
        fn new(actions: impl IntoIterator<Item = Action>) -> Self {
            Self(Arc::new(Mutex::new(ScriptState {
                actions: actions.into_iter().collect(),
                requests: Vec::new(),
            })))
        }

        fn requests(&self) -> Vec<String> {
            self.0.lock().expect("script lock").requests.clone()
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

            HttpConnectorFuture::new(async move {
                match action {
                    Action::Io => Err(ConnectorError::io(
                        std::io::Error::other("post-send I/O failure").into(),
                    )),
                    Action::Timeout => Err(ConnectorError::timeout(
                        std::io::Error::other("post-send timeout").into(),
                    )),
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
    async fn token_create_nat_gateway_resends_the_same_token_after_timeout() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Response {
                status: 200,
                body: CREATE_NAT_GATEWAY_OK,
            },
        ]);
        let provider = provider(connector.clone());
        let token = "stable-client-token";

        provider
            .ec2_client
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
