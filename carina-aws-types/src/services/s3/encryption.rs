use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

/// SSE algorithm enum for `aws.s3.BucketServerSideEncryptionConfiguration`.
pub(crate) fn s3_sse_algorithm() -> AttributeType {
    enum_with_dsl_aliases(
        &["AES256", "aws:kms", "aws:kms:dsse"],
        carina_core::schema::enum_identity(
            "SseAlgorithm",
            Some("aws.s3.BucketServerSideEncryptionConfiguration.SseRule.SseByDefault"),
        ),
    )
}

/// `ApplyServerSideEncryptionByDefault` struct: SSE algorithm + optional KMS key.
fn s3_sse_by_default() -> AttributeType {
    AttributeType::struct_(
        "SseByDefault".to_string(),
        vec![
            StructField::new("sse_algorithm", s3_sse_algorithm())
                .with_provider_name("SSEAlgorithm")
                .required(),
            StructField::new("kms_master_key_id", AttributeType::string())
                .with_provider_name("KMSMasterKeyID"),
        ],
    )
}

/// A single SSE rule: per-bucket default + optional bucket-key flag.
fn s3_sse_rule() -> AttributeType {
    AttributeType::struct_(
        "SseRule".to_string(),
        vec![
            StructField::new(
                "apply_server_side_encryption_by_default",
                s3_sse_by_default(),
            )
            .with_provider_name("ApplyServerSideEncryptionByDefault")
            .with_block_name("apply_server_side_encryption_by_default"),
            StructField::new("bucket_key_enabled", AttributeType::bool())
                .with_provider_name("BucketKeyEnabled"),
        ],
    )
}

/// List of SSE rules — the `rules` attribute on
/// `aws.s3.BucketServerSideEncryptionConfiguration`.
pub fn bucket_encryption_rules() -> AttributeType {
    AttributeType::list(s3_sse_rule())
}
