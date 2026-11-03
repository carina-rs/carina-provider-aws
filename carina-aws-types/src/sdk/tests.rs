use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_runtime_api::client::http::{
    HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
};
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_types::{SdkConfig, region::Region};
use std::error::Error;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct UnusedConnector;

impl HttpConnector for UnusedConnector {
    fn call(&self, _request: HttpRequest) -> HttpConnectorFuture {
        panic!("config inspection must not make HTTP requests");
    }
}

#[test]
fn regression_s3_express_session_auth_is_disabled() {
    let config = SdkConfig::builder()
        .region(Region::new("us-east-1"))
        .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
            "test",
            "test",
            None,
            None,
            "s3-config-test",
        )))
        .http_client(http_client_fn(|_, _| {
            SharedHttpConnector::new(UnusedConnector)
        }))
        .sleep_impl(TokioSleep::new())
        .build();
    let client = super::s3::S3Client::new(&config, |_| crate::Idempotency::NotIdempotent);
    // SDK 1.156.0 has no public getter for this config field. Its Debug view
    // exposes the typed config entry and lets this pin the factory's setting.
    assert!(format!("{:?}", client.inner().config()).contains("DisableS3ExpressSessionAuth(true)"));
}

#[derive(Clone, Debug)]
struct RecordedConnector(Arc<Mutex<Vec<String>>>);

impl HttpConnector for RecordedConnector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        self.0.lock().unwrap().push(request.uri().to_string());
        HttpConnectorFuture::new(async {
            Err(ConnectorError::user(
                "test connector rejects network requests".into(),
            ))
        })
    }
}

#[tokio::test]
async fn review_round1_s3_express_override_cannot_start_create_session() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let connector = RecordedConnector(requests.clone());
    let config = SdkConfig::builder()
        .region(Region::new("us-east-1"))
        .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
            "test",
            "test",
            None,
            None,
            "s3-override-test",
        )))
        .http_client(http_client_fn(move |_, _| {
            SharedHttpConnector::new(connector.clone())
        }))
        .sleep_impl(TokioSleep::new())
        .build();
    let client = super::s3::S3Client::new(&config, |_| crate::Idempotency::NotIdempotent);
    let override_config = client
        .inner()
        .config()
        .to_builder()
        .disable_s3_express_session_auth(false);
    let error = client
        .inner()
        .get_object()
        .bucket("example--use1-az5--x-s3")
        .key("example")
        .customize()
        .config_override(override_config)
        .send()
        .await
        .unwrap_err();
    let requests = requests.lock().unwrap();
    assert!(
        requests.is_empty(),
        "an S3 Express override reached the connector: {requests:?}"
    );
    let mut cause: Option<&(dyn Error + 'static)> = Some(&error);
    let mut unsupported = false;
    while let Some(error) = cause {
        unsupported |= error
            .to_string()
            .contains("S3 Express session auth is not supported");
        cause = error.source();
    }
    assert!(
        unsupported,
        "the caller must see the explicit unsupported-auth error"
    );
}
