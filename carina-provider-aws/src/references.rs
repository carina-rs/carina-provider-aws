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
            use aws_smithy_types::error::metadata::ProvideErrorMetadata;
            use aws_types::request_id::RequestId;
            let output = self
                .client_in_region(region)
                .describe_key()
                .key_id(raw)
                .send()
                .await
                .map_err(|failure| DescribeKeyFailure {
                    code: failure.code().map(str::to_owned),
                    status: failure.raw_response().map(|r| r.status().as_u16()),
                    request_id: failure.request_id().map(str::to_owned),
                })?;
            Ok(output
                .key_metadata()
                .and_then(|metadata| metadata.arn())
                .map(str::to_owned))
        })
    }
}

pub(crate) async fn resolve_reference_batch(
    client: &impl DescribeKeyClient,
    region: &str,
    requests: Vec<ReferenceRequest>,
) -> Vec<Result<ReferenceOutcome, ReferenceError>> {
    let identity = carina_aws_types::AwsType::KmsKeyReference.identity();
    futures::future::join_all(requests.into_iter().map(|request| {
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
                    provider_name: Some("aws".into()),
                    status: None,
                    code: None,
                    request_id: None,
                })));
            }
            resolve_kms_key_reference(client, region, &request.value).await
        }
    }))
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
    use carina_aws_types::kms_reference::{DescribeKeyClient, DescribeKeyFailure};
    use carina_provider_protocol::types::{
        ReferenceError, ReferenceOutcome, ReferenceRequest, TypeIdentity,
    };
    use std::sync::Mutex;

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
        assert_eq!(*fake.0.lock().unwrap(), ["alias/first", "alias/second"]);
        assert!(
            super::resolve_reference_batch(&fake, "us-east-1", vec![])
                .await
                .is_empty()
        );
    }
}
