//! KMS reference batching for the initialized provider instance.

use crate::{AwsProvider, BoundedSdkConfig};
use carina_aws_types::kms_reference::{
    DescribeKeyClient, DescribeKeyFailure, resolve_kms_key_reference,
};
use carina_aws_types::{Idempotency, ResendGuard};
use carina_core::provider::BoxFuture;
use carina_provider_protocol::types::{
    ProviderError, ProviderErrorKind, ReferenceError, ReferenceOutcome, ReferenceRequest,
};
use futures::{StreamExt, stream};

const PROVIDER_NAME: &str = "aws";
const MAX_CONCURRENT_DESCRIBE_KEY: usize = 8;

/// Bounded, guarded KMS client wrapper. Regional clients retain the instance's
/// credentials provider, including its assumed-role session and refresh path.
pub(crate) struct KmsClient {
    config: BoundedSdkConfig,
}

impl KmsClient {
    pub(crate) fn from_bounded_config(config: &BoundedSdkConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    // Raw SDK constructors are confined to client wrappers, as for generated clients.
    #[allow(clippy::disallowed_methods)]
    fn client_in_region(&self, region: &str) -> aws_sdk_kms::Client {
        let config = self.config.in_region(region);
        aws_sdk_kms::Client::from_conf(
            aws_sdk_kms::config::Builder::from(config.sdk_config())
                .interceptor(ResendGuard::new(|operation| match operation {
                    // DescribeKey may initialize a managed key; a repeat is still safe.
                    "DescribeKey" => Idempotency::ReadOnly,
                    _ => Idempotency::NotIdempotent,
                }))
                .build(),
        )
    }

    #[cfg(test)]
    pub(crate) fn config(&self) -> aws_sdk_kms::Config {
        self.client_in_region(self.config.sdk_config().region().unwrap().as_ref())
            .config()
            .clone()
    }
}

impl DescribeKeyClient for KmsClient {
    fn describe_key<'a>(
        &'a self,
        region: &'a str,
        raw: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>> {
        Box::pin(async move {
            let output = self
                .client_in_region(region)
                .describe_key()
                .key_id(raw)
                .send()
                .await
                .map_err(describe_key_failure)?;
            Ok(output
                .key_metadata()
                .and_then(|metadata| metadata.arn())
                .map(str::to_owned))
        })
    }
}

fn describe_key_failure(
    failure: aws_sdk_kms::error::SdkError<aws_sdk_kms::operation::describe_key::DescribeKeyError>,
) -> DescribeKeyFailure {
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_types::request_id::RequestId;

    // SDK Display is a safe error category; service metadata supplies the AWS
    // message, including for unmodeled errors. Never format request/config Debug
    // or arbitrary credential-provider sources (DisplayErrorContext adds Debug).
    let message = match failure.message() {
        Some(message) => format!("{failure}: {message}"),
        None => failure.to_string(),
    };
    DescribeKeyFailure {
        message,
        code: failure.code().map(str::to_owned),
        status: failure.raw_response().map(|r| r.status().as_u16()),
        request_id: failure.request_id().map(str::to_owned),
    }
}

pub(crate) async fn resolve_reference_batch(
    client: &impl DescribeKeyClient,
    region: &str,
    requests: Vec<ReferenceRequest>,
) -> Vec<Result<ReferenceOutcome, ReferenceError>> {
    let identity = carina_aws_types::AwsType::KmsKeyReference.identity();
    stream::iter(requests.into_iter().map(|request| {
        let identity = &identity;
        async move {
            if identity.provider.as_deref() != Some(request.ty.provider.as_str())
                || identity.segments != request.ty.segments
                || identity.kind != request.ty.kind
            {
                let ty = std::iter::once(request.ty.provider)
                    .chain(request.ty.segments)
                    .chain(std::iter::once(request.ty.kind))
                    .collect::<Vec<_>>()
                    .join(".");
                return Err(ReferenceError::InvalidReference(Box::new(ProviderError {
                    kind: ProviderErrorKind::InvalidInput,
                    message: format!("unsupported reference identity {ty}"),
                    operation: Some("resolve_references".into()),
                    resource_id: None,
                    cause: None,
                    provider_name: Some(PROVIDER_NAME.into()),
                    status: None,
                    code: None,
                    request_id: None,
                })));
            }
            resolve_kms_key_reference(client, PROVIDER_NAME, region, &request.value).await
        }
    }))
    .buffered(MAX_CONCURRENT_DESCRIBE_KEY)
    .collect()
    .await
}

impl AwsProvider {
    /// Resolve one positional batch in the initialized instance's scope.
    pub async fn resolve_references(
        &self,
        requests: Vec<ReferenceRequest>,
    ) -> Vec<Result<ReferenceOutcome, ReferenceError>> {
        resolve_reference_batch(&self.kms_client, &self.region, requests).await
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CONCURRENT_DESCRIBE_KEY, describe_key_failure, resolve_reference_batch};
    use aws_sdk_kms::error::SdkError;
    use aws_sdk_kms::operation::describe_key::DescribeKeyError;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::{body::SdkBody, error::ErrorMetadata};
    use carina_aws_types::kms_reference::{DescribeKeyClient, DescribeKeyFailure};
    use carina_provider_protocol::types::{
        ReferenceError, ReferenceOutcome, ReferenceRequest, TypeIdentity,
    };
    use std::sync::Mutex;

    #[test]
    fn sdk_failure_preserves_service_message_and_metadata_without_debug() {
        for code in [
            "NotFoundException",
            "AccessDeniedException",
            "InvalidArnException",
        ] {
            let metadata = ErrorMetadata::builder()
                .code(code)
                .message("KMS key is unavailable to this caller")
                .custom("sensitive-extra", "must-not-format-extra")
                .build();
            let mut response = HttpResponse::new(
                400.try_into().unwrap(),
                SdkBody::from("must-not-format-body"),
            );
            response
                .headers_mut()
                .insert("x-amzn-requestid", "request-123");
            response
                .headers_mut()
                .insert("x-amz-security-token", "must-not-format-token");
            let service_error = if code == "NotFoundException" {
                DescribeKeyError::NotFoundException(
                    aws_sdk_kms::types::error::NotFoundException::builder()
                        .message("KMS key is unavailable to this caller")
                        .meta(metadata)
                        .build(),
                )
            } else {
                DescribeKeyError::generic(metadata)
            };
            let failure = describe_key_failure(SdkError::service_error(service_error, response));
            assert_eq!(
                failure.message,
                "service error: KMS key is unavailable to this caller"
            );
            assert_eq!(failure.code.as_deref(), Some(code));
            assert_eq!(failure.status, Some(400));
            assert_eq!(failure.request_id.as_deref(), Some("request-123"));
        }
    }

    #[test]
    fn sdk_failure_does_not_format_credential_sources() {
        let failure = describe_key_failure(SdkError::construction_failure(std::io::Error::other(
            "must-not-format-credentials",
        )));
        assert_eq!(failure.message, "failed to construct request");
        assert!(failure.code.is_none());
        assert!(failure.status.is_none());
        assert!(failure.request_id.is_none());
    }

    struct Fake(Mutex<Vec<String>>);
    impl DescribeKeyClient for Fake {
        fn describe_key<'a>(
            &'a self,
            region: &'a str,
            raw: &'a str,
        ) -> carina_core::provider::BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>
        {
            Box::pin(async move {
                assert_eq!(region, "us-east-1");
                self.0.lock().unwrap().push(raw.into());
                tokio::task::yield_now().await;
                Ok(Some(format!(
                    "arn:aws:kms:us-east-1:123456789012:key/{}",
                    if raw == "alias/first" {
                        "11111111-1111-1111-1111-111111111111"
                    } else {
                        "22222222-2222-2222-2222-222222222222"
                    }
                )))
            })
        }
    }

    fn request(value: &str) -> ReferenceRequest {
        ReferenceRequest {
            ty: TypeIdentity {
                provider: "aws".into(),
                segments: vec!["kms".into(), "Key".into()],
                kind: "Reference".into(),
            },
            value: value.into(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn batch_bounds_concurrency_and_keeps_positions_when_calls_finish_out_of_order() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        #[derive(Default)]
        struct ConcurrentClient {
            active: AtomicUsize,
            peak: AtomicUsize,
            completed: Mutex<Vec<usize>>,
        }

        impl DescribeKeyClient for ConcurrentClient {
            fn describe_key<'a>(
                &'a self,
                _region: &'a str,
                raw: &'a str,
            ) -> carina_core::provider::BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>
            {
                Box::pin(async move {
                    let index: usize = raw.strip_prefix("alias/").unwrap().parse().unwrap();
                    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                    self.peak.fetch_max(active, Ordering::SeqCst);
                    // Each window completes in reverse order under the paused clock.
                    tokio::time::sleep(Duration::from_secs(
                        (MAX_CONCURRENT_DESCRIBE_KEY - index % MAX_CONCURRENT_DESCRIBE_KEY) as u64,
                    ))
                    .await;
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    self.completed.lock().unwrap().push(index);
                    Ok(Some(format!(
                        "arn:aws:kms:us-east-1:123456789012:key/{index:08x}-0000-0000-0000-000000000000"
                    )))
                })
            }
        }

        let client = ConcurrentClient::default();
        let count = MAX_CONCURRENT_DESCRIBE_KEY * 3 + 1;
        let results = resolve_reference_batch(
            &client,
            "us-east-1",
            (0..count)
                .map(|index| request(&format!("alias/{index}")))
                .collect(),
        )
        .await;

        assert_eq!(
            client.peak.load(Ordering::SeqCst),
            MAX_CONCURRENT_DESCRIBE_KEY
        );
        assert_eq!(client.active.load(Ordering::SeqCst), 0);
        assert_eq!(
            client.completed.lock().unwrap()[0],
            MAX_CONCURRENT_DESCRIBE_KEY - 1
        );
        assert_eq!(results.len(), count);
        for (index, result) in results.into_iter().enumerate() {
            assert!(matches!(result, Ok(ReferenceOutcome::Resolved(arn))
                if arn.ends_with(&format!("{index:08x}-0000-0000-0000-000000000000"))));
        }
    }

    #[tokio::test]
    async fn batch_preserves_positions_and_rejects_unsupported_identities() {
        let fake = Fake(Mutex::new(vec![]));
        let mut unsupported = request("alias/ignored");
        unsupported.ty.provider = "awscc".into();
        let mut wrong_kind = request("alias/ignored");
        wrong_kind.ty.kind = "Arn".into();
        let results = super::resolve_reference_batch(
            &fake,
            "us-east-1",
            vec![
                request("alias/first"),
                unsupported,
                request("alias/second"),
                wrong_kind,
            ],
        )
        .await;
        assert_eq!(results.len(), 4);
        assert!(
            matches!(&results[0], Ok(ReferenceOutcome::Resolved(arn)) if arn.ends_with("11111111-1111-1111-1111-111111111111"))
        );
        assert!(matches!(
            &results[1],
            Err(ReferenceError::InvalidReference(_))
        ));
        assert!(
            matches!(&results[2], Ok(ReferenceOutcome::Resolved(arn)) if arn.ends_with("22222222-2222-2222-2222-222222222222"))
        );
        assert!(matches!(
            &results[3],
            Err(ReferenceError::InvalidReference(_))
        ));
        for index in [1, 3] {
            let Err(ReferenceError::InvalidReference(error)) = &results[index] else {
                unreachable!();
            };
            assert_eq!(error.provider_name.as_deref(), Some("aws"));
        }
        assert_eq!(*fake.0.lock().unwrap(), ["alias/first", "alias/second"]);
        assert!(
            super::resolve_reference_batch(&fake, "us-east-1", vec![])
                .await
                .is_empty()
        );
    }
}
