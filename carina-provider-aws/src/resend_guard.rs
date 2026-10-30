//! Integration tests for the shared resend guard using real generated clients.

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use aws_smithy_runtime_api::client::http::{
        HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
    };
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use carina_aws_types::SendOutcome;
    use carina_plugin_sdk::wasi_http::{SendPhase, transport_connector_error_for_test};

    use crate::aws_operation::ClientToken;
    use crate::helpers::send_aws_operation_with_token;
    use crate::{AwsProvider, BoundedSdkConfig};

    #[derive(Debug)]
    enum Action {
        Io,
        Timeout,
        User,
        Transport(SendPhase),
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
                    Action::User => Err(ConnectorError::user(
                        std::io::Error::other("invalid test request").into(),
                    )),
                    Action::Transport(phase) => Err(transport_connector_error_for_test(phase)),
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
        let bounded = BoundedSdkConfig::for_test_http_client(http_client, "https://example.test");
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
    const NO_SUCH_BUCKET_POLICY: &str = r#"<Error><Code>NoSuchBucketPolicy</Code>
        <Message>The bucket policy does not exist</Message><RequestId>request-id</RequestId>
        <HostId>host-id</HostId></Error>"#;

    fn connector_error<E>(
        error: &aws_smithy_runtime_api::client::result::SdkError<E, HttpResponse>,
    ) -> &ConnectorError {
        let aws_smithy_runtime_api::client::result::SdkError::DispatchFailure(context) = error
        else {
            panic!("expected dispatch failure");
        };
        context
            .as_connector_error()
            .expect("dispatch failure has a connector error")
    }

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

        let error = provider
            .ec2_client
            .create_vpc()
            .cidr_block("10.0.0.0/16")
            .send()
            .await
            .expect_err("the resend guard must refuse the ambiguous retry");

        assert_eq!(connector.requests().len(), 1);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
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

    #[tokio::test(start_paused = true)]
    async fn generated_ec2_client_retries_only_readonly_maybe_sent_operation() {
        let readonly_connector = ScriptedConnector::new([
            Action::Transport(SendPhase::MaybeSent),
            Action::Response {
                status: 200,
                body: DESCRIBE_VPCS_OK,
            },
        ]);
        let readonly_provider = provider(readonly_connector.clone());

        readonly_provider
            .ec2_client
            .describe_vpcs()
            .send()
            .await
            .expect("generated read-only operation should retry a maybe-sent failure");

        assert_eq!(readonly_connector.requests().len(), 2);

        let mutating_connector = ScriptedConnector::new([
            Action::Transport(SendPhase::MaybeSent),
            Action::Response {
                status: 200,
                body: CREATE_VPC_OK,
            },
        ]);
        let mutating_provider = provider(mutating_connector.clone());

        let error = mutating_provider
            .ec2_client
            .create_vpc()
            .cidr_block("10.0.0.0/16")
            .send()
            .await
            .expect_err("generated non-idempotent operation must not retry a maybe-sent failure");

        assert_eq!(mutating_connector.requests().len(), 1);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[tokio::test(start_paused = true)]
    async fn readonly_maybe_sent_failures_exhaust_attempts_and_remain_maybe_sent() {
        let connector = ScriptedConnector::new([
            Action::Transport(SendPhase::MaybeSent),
            Action::Transport(SendPhase::MaybeSent),
            Action::Transport(SendPhase::MaybeSent),
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .ec2_client
            .describe_vpcs()
            .send()
            .await
            .expect_err("three maybe-sent failures should exhaust the retry budget");

        assert_eq!(connector.requests().len(), 3);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[tokio::test(start_paused = true)]
    async fn idempotent_and_token_operations_do_not_retry_maybe_sent_transport_failures() {
        let idempotent_connector = ScriptedConnector::new([
            Action::Transport(SendPhase::MaybeSent),
            Action::Response {
                status: 200,
                body: "",
            },
        ]);
        let idempotent_provider = provider(idempotent_connector.clone());

        let idempotent_error = idempotent_provider
            .s3_client
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("idempotent operation should retain the current no-retry behavior");

        assert_eq!(idempotent_connector.requests().len(), 1);
        assert_eq!(
            SendOutcome::classify(&idempotent_error),
            SendOutcome::MaybeSent
        );

        let token_connector = ScriptedConnector::new([
            Action::Transport(SendPhase::MaybeSent),
            Action::Response {
                status: 200,
                body: CREATE_NAT_GATEWAY_OK,
            },
        ]);
        let token_provider = provider(token_connector.clone());
        let request = token_provider
            .ec2_client
            .create_nat_gateway()
            .configure(|builder| builder.subnet_id("subnet-123"));

        let token_error = send_aws_operation_with_token(&ClientToken::new(), request)
            .await
            .expect_err("token operation should retain the current no-retry behavior");

        assert_eq!(token_connector.requests().len(), 1);
        assert_eq!(SendOutcome::classify(&token_error), SendOutcome::MaybeSent);
    }

    #[tokio::test(start_paused = true)]
    async fn readonly_user_failure_is_not_retried() {
        let connector = ScriptedConnector::new([
            Action::User,
            Action::Response {
                status: 200,
                body: DESCRIBE_VPCS_OK,
            },
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .ec2_client
            .describe_vpcs()
            .send()
            .await
            .expect_err("user connector failures must not be retried");

        assert_eq!(connector.requests().len(), 1);
        assert!(connector_error(&error).is_user());
    }

    #[tokio::test(start_paused = true)]
    async fn not_sent_failure_is_retried_for_non_idempotent_operation() {
        let connector = ScriptedConnector::new([
            Action::Transport(SendPhase::NotSent),
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
            .expect("a proven not-sent mutation should be retried");

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
        let provider = provider(connector.clone());
        let token = ClientToken::new();
        let request = provider
            .ec2_client
            .create_nat_gateway()
            .configure(|builder| builder.subnet_id("subnet-123"));

        send_aws_operation_with_token(&token, request)
            .await
            .expect("token-bearing request should be retried");

        let requests = connector.requests();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|request| request.contains(token.as_str()))
        );
    }

    #[tokio::test]
    async fn token_operation_maybe_sent_then_not_sent_is_maybe_sent() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Transport(SendPhase::NotSent),
            Action::Transport(SendPhase::NotSent),
        ]);
        let provider = provider(connector.clone());
        let token = ClientToken::new();
        let request = provider
            .ec2_client
            .create_nat_gateway()
            .configure(|builder| builder.subnet_id("subnet-123"));

        let error = send_aws_operation_with_token(&token, request)
            .await
            .expect_err("all SDK attempts should fail");

        assert_eq!(connector.requests().len(), 3);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
        assert!(connector_error(&error).is_io());
    }

    #[tokio::test]
    async fn idempotent_operation_maybe_sent_then_not_sent_is_maybe_sent() {
        let connector = ScriptedConnector::new([
            Action::Timeout,
            Action::Transport(SendPhase::NotSent),
            Action::Transport(SendPhase::NotSent),
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .s3_client
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("all SDK attempts should fail");

        assert_eq!(connector.requests().len(), 3);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
        assert!(connector_error(&error).is_io());
    }

    #[tokio::test]
    async fn idempotent_operation_maybe_sent_then_4xx_is_typed_and_maybe_sent() {
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
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("the second attempt should surface NoSuchBucketPolicy");

        assert_eq!(connector.requests().len(), 2);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
        let service_error = error.into_service_error();
        assert_eq!(
            ProvideErrorMetadata::code(&service_error),
            Some("NoSuchBucketPolicy")
        );
    }

    #[tokio::test]
    async fn all_attempts_not_sent_is_not_sent() {
        let connector = ScriptedConnector::new([
            Action::Transport(SendPhase::NotSent),
            Action::Transport(SendPhase::NotSent),
            Action::Transport(SendPhase::NotSent),
        ]);
        let provider = provider(connector.clone());

        let error = provider
            .s3_client
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("all SDK attempts should fail before sending");

        assert_eq!(connector.requests().len(), 3);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::NotSent);
        assert!(connector_error(&error).is_io());
    }

    #[tokio::test]
    async fn final_connector_timeout_preserves_dispatch_kind() {
        let connector = ScriptedConnector::new([Action::Timeout, Action::Timeout, Action::Timeout]);
        let provider = provider(connector.clone());

        let error = provider
            .s3_client
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("all SDK attempts should time out");

        assert_eq!(connector.requests().len(), 3);
        assert!(connector_error(&error).is_timeout());
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
    }

    #[tokio::test]
    async fn single_attempt_4xx_is_rejected_with_guard_verdict() {
        let connector = ScriptedConnector::new([Action::Response {
            status: 404,
            body: NO_SUCH_BUCKET_POLICY,
        }]);
        let provider = provider(connector.clone());

        let error = provider
            .s3_client
            .delete_bucket_policy()
            .bucket("test-bucket")
            .send()
            .await
            .expect_err("the service should reject the request");

        assert_eq!(connector.requests().len(), 1);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::Rejected);
        let service_error = error.into_service_error();
        assert_eq!(
            ProvideErrorMetadata::code(&service_error),
            Some("NoSuchBucketPolicy")
        );
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
        let provider = provider(connector.clone());
        let token = ClientToken::new();
        let request = provider
            .ec2_client
            .create_nat_gateway()
            .configure(|builder| builder.subnet_id("subnet-123"));

        let error = send_aws_operation_with_token(&token, request)
            .await
            .expect_err("all three SDK attempts should fail");
        assert_eq!(connector.requests().len(), 3);
        assert_eq!(SendOutcome::classify(&error), SendOutcome::MaybeSent);
        let aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) = &*error
        else {
            panic!("expected the modeled service error, got {error:?}");
        };
        assert_eq!(
            ProvideErrorMetadata::code(context.err()),
            Some("RequestLimitExceeded")
        );
        assert_eq!(
            context.raw().headers().get("x-amzn-requestid"),
            Some("final-throttling-request-id")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn token_send_keeps_maybe_sent_after_sdk_attempt_timeout() {
        let connector = ScriptedConnector::new([
            Action::DelayedTimeout(Duration::from_secs(31)),
            Action::Response {
                status: 429,
                body: THROTTLED,
            },
            Action::Response {
                status: 429,
                body: THROTTLED,
            },
        ]);
        let provider = provider(connector.clone());
        let token = ClientToken::new();
        let request = provider
            .ec2_client
            .create_nat_gateway()
            .configure(|builder| builder.subnet_id("subnet-123"));

        let error = send_aws_operation_with_token(&token, request)
            .await
            .expect_err("all three SDK attempts should fail");

        assert_eq!(connector.requests().len(), 3);
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
