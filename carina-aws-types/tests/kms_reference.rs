#![cfg(feature = "test-utils")]

use carina_aws_types::kms_reference::{
    DescribeKeyClient, DescribeKeyFailure, resolve_core_reference_batch,
};
use carina_core::provider::BoxFuture;
use carina_core::references::{ReferenceError, ReferenceRequest};
use carina_core::test_utils::provider_text::NativeWireFixture as _;

struct DeniedClient;

impl DescribeKeyClient for DeniedClient {
    fn describe_key<'a>(
        &'a self,
        _region: &'a str,
        _raw: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>> {
        Box::pin(async {
            Err(DescribeKeyFailure::for_test(
                "fake access denied",
                Some("AccessDeniedException".into()),
                Some(403),
                Some("fake-request".into()),
            ))
        })
    }
}

#[tokio::test]
async fn fake_failure_constructor_preserves_classification_and_metadata() {
    let Err(ReferenceError::AccessDenied(error)) = resolve_core_reference_batch(
        &DeniedClient,
        "awscc",
        "us-east-1",
        &[ReferenceRequest {
            ty: carina_aws_types::AwsType::KmsKeyReference.identity(),
            value: "alias/test".into(),
        }],
    )
    .await
    .pop()
    .unwrap() else {
        panic!("fake access denial must reach the resolver caller");
    };
    let error = error.to_wire();
    assert_eq!(error.cause.as_deref(), Some("fake access denied"));
    assert_eq!(error.code.as_deref(), Some("AccessDeniedException"));
    assert_eq!(error.status, Some(403));
    assert_eq!(error.request_id.as_deref(), Some("fake-request"));
    assert_eq!(error.provider_name.as_deref(), Some("awscc"));
}
