//! In-process HTTP responses for tests that exercise generated SDK clients.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use aws_smithy_runtime_api::client::http::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_types::body::SdkBody;

use crate::{AwsProvider, BoundedSdkConfig};

#[derive(Debug)]
struct Script {
    responses: VecDeque<(u16, String)>,
    requests: Vec<String>,
    unexpected_requests: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptedConnector(Arc<Mutex<Script>>);

impl ScriptedConnector {
    pub(crate) fn requests(&self) -> Vec<String> {
        self.0.lock().expect("script lock").requests.clone()
    }

    pub(crate) fn assert_finished(&self) {
        let (remaining, unexpected) = {
            let script = self.0.lock().expect("script lock");
            (script.responses.len(), script.unexpected_requests.clone())
        };
        assert!(
            unexpected.is_empty(),
            "unexpected request(s): {}",
            unexpected.join("; ")
        );
        assert_eq!(remaining, 0, "{remaining} scripted responses remain");
    }
}

impl HttpConnector for ScriptedConnector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let request_body = request
            .body()
            .bytes()
            .map(|body| String::from_utf8_lossy(body).into_owned());
        let action = request
            .headers()
            .get("x-amz-target")
            .or_else(|| request_body.as_deref().and_then(query_action))
            .or_else(|| {
                request
                    .uri()
                    .split_once('?')
                    .and_then(|(_, query)| query_action(query))
            })
            .unwrap_or("<unavailable>");
        let request_description = format!(
            "action={action}; uri={}; body={}",
            request.uri(),
            request_body.as_deref().unwrap_or("<unavailable>")
        );
        let response = {
            let mut script = self.0.lock().expect("script lock");
            script.requests.push(request_description.clone());
            let response = script.responses.pop_front();
            if response.is_none() {
                script.unexpected_requests.push(request_description.clone());
            }
            response
        };
        // Release the lock before panicking so assert_finished can still report
        // the unexpected request if the SDK catches the connector panic.
        let (status, body) =
            response.unwrap_or_else(|| panic!("unexpected request: {request_description}"));
        HttpConnectorFuture::new(async move {
            let content_type = if body.trim_start().starts_with('{') {
                "application/x-amz-json-1.1"
            } else {
                "text/xml"
            };
            let mut response = HttpResponse::new(
                status.try_into().expect("valid test status"),
                SdkBody::from(body),
            );
            response.headers_mut().insert("content-type", content_type);
            response
                .headers_mut()
                .insert("x-amzn-requestid", "test-request");
            Ok(response)
        })
    }
}

fn query_action(parameters: &str) -> Option<&str> {
    parameters
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("Action="))
}

pub(crate) fn provider(
    responses: impl IntoIterator<Item = (u16, String)>,
) -> (AwsProvider, ScriptedConnector) {
    let connector = ScriptedConnector(Arc::new(Mutex::new(Script {
        responses: responses.into_iter().collect(),
        requests: Vec::new(),
        unexpected_requests: Vec::new(),
    })));
    let selected = connector.clone();
    let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
    let config = BoundedSdkConfig::for_test_http_client(http_client, "https://example.test");
    (
        AwsProvider::from_sdk_config("us-east-1".into(), vec![], vec![], &config),
        connector,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "2 scripted responses remain")]
    fn assert_finished_reports_remaining_responses() {
        let (_, responses) = provider([(200, String::new()), (200, String::new())]);
        responses.assert_finished();
    }

    #[tokio::test]
    async fn requests_record_action_uri_and_body() {
        let (_, responses) = provider([
            (200, String::new()),
            (200, String::new()),
            (200, String::new()),
        ]);
        for (uri, target, body, action) in [
            (
                "https://example.test/json",
                Some("Example.DoThing"),
                r#"{"value":"test"}"#,
                "Example.DoThing",
            ),
            (
                "https://example.test/query",
                None,
                "Action=DescribeVpcs&Version=2016-11-15",
                "DescribeVpcs",
            ),
            (
                "https://example.test/query?Action=DescribeSubnets",
                None,
                "",
                "DescribeSubnets",
            ),
        ] {
            let mut request = HttpRequest::new(SdkBody::from(body));
            request.set_uri(uri).unwrap();
            if let Some(target) = target {
                request.headers_mut().insert("x-amz-target", target);
            }
            responses.call(request).await.unwrap();
            let recorded = responses.requests().pop().unwrap();
            assert!(recorded.contains(&format!("action={action}")), "{recorded}");
            assert!(recorded.contains(&format!("uri={uri}")), "{recorded}");
            assert!(recorded.contains(&format!("body={body}")), "{recorded}");
        }
        responses.assert_finished();
    }

    #[tokio::test]
    async fn non_buffered_body_is_recorded_as_unavailable() {
        let (_, responses) = provider([(200, String::new())]);
        let mut request = HttpRequest::new(SdkBody::taken());
        request.set_uri("https://example.test/unbuffered").unwrap();
        request
            .headers_mut()
            .insert("x-amz-target", "Example.Unbuffered");
        responses.call(request).await.unwrap();
        let recorded = responses.requests().pop().unwrap();
        assert!(recorded.contains("action=Example.Unbuffered"), "{recorded}");
        assert!(
            recorded.contains("uri=https://example.test/unbuffered"),
            "{recorded}"
        );
        assert!(recorded.contains("body=<unavailable>"), "{recorded}");
        responses.assert_finished();
    }

    #[tokio::test]
    async fn unexpected_request_remains_visible_after_caught_panic() {
        let (provider, responses) = provider([]);
        // An SDK may turn connector panics into dispatch errors; a task also
        // catches unwinding here so assert_finished must independently fail.
        let _ = tokio::spawn(async move {
            provider
                .ec2_client
                .delete_flow_logs()
                .flow_log_ids("fl-unexpected")
                .send()
                .await
        })
        .await;
        let panic = std::panic::catch_unwind(|| responses.assert_finished())
            .expect_err("an unexpected request must fail the test even after its panic is caught");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap();
        assert!(message.contains("Action=DeleteFlowLogs"), "{message}");
        assert!(message.contains("fl-unexpected"), "{message}");
        assert!(message.contains("uri=https://example.test"), "{message}");
    }
}
