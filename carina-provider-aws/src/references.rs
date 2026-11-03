//! KMS reference batching for the initialized provider instance.

use crate::{AwsProvider, BoundedSdkConfig};
use carina_aws_types::Idempotency;
use carina_aws_types::kms_reference::{
    DescribeKeyClient, DescribeKeyFailure, describe_key_failure, resolve_reference_batch,
};
use carina_aws_types::sdk::kms;
use carina_core::provider::BoxFuture;
use carina_provider_protocol::types::{ReferenceError, ReferenceOutcome, ReferenceRequest};

const PROVIDER_NAME: &str = "aws";

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

    fn client_in_region(&self, region: &str) -> kms::KmsClient {
        let config = self.config.in_region(region);
        kms::KmsClient::new(config.sdk_config(), |operation| {
            // DescribeKey may initialize a managed key; a repeat is still safe.
            if operation == "DescribeKey" {
                Idempotency::ReadOnly
            } else {
                Idempotency::NotIdempotent
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn guarded_client(&self) -> kms::KmsClient {
        self.client_in_region(self.config.sdk_config().region().unwrap().as_ref())
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
                .inner()
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

impl AwsProvider {
    /// Resolve one positional batch in the initialized instance's scope.
    pub async fn resolve_references(
        &self,
        requests: Vec<ReferenceRequest>,
    ) -> Vec<Result<ReferenceOutcome, ReferenceError>> {
        resolve_reference_batch(&self.kms_client, PROVIDER_NAME, &self.region, requests).await
    }
}
