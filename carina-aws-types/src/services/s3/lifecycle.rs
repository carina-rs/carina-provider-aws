use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

use super::replication::s3_filter_tag;

/// Lifecycle rule status (Enabled / Disabled).
fn s3_lifecycle_status() -> AttributeType {
    enum_with_dsl_aliases(
        &["Enabled", "Disabled"],
        carina_core::schema::enum_identity(
            "Status",
            Some("aws.s3.BucketLifecycleConfiguration.LifecycleRule"),
        ),
    )
}

/// Storage class for lifecycle transitions (Glacier / IA / etc.).
fn s3_transition_storage_class() -> AttributeType {
    enum_with_dsl_aliases(
        &[
            "GLACIER",
            "STANDARD_IA",
            "ONEZONE_IA",
            "INTELLIGENT_TIERING",
            "DEEP_ARCHIVE",
            "GLACIER_IR",
        ],
        carina_core::schema::enum_identity(
            "StorageClass",
            Some("aws.s3.BucketLifecycleConfiguration.LifecycleRule.LifecycleTransition"),
        ),
    )
}

fn s3_lifecycle_expiration() -> AttributeType {
    AttributeType::struct_(
        "LifecycleExpiration".to_string(),
        vec![
            StructField::optional("days", AttributeType::int())
                .with_provider_name("Days")
                .build(),
            StructField::optional("date", crate::s3_lifecycle_date())
                .with_provider_name("Date")
                .build(),
            StructField::optional("expired_object_delete_marker", AttributeType::bool())
                .with_provider_name("ExpiredObjectDeleteMarker")
                .build(),
        ],
    )
}

fn s3_lifecycle_transition() -> AttributeType {
    AttributeType::struct_(
        "LifecycleTransition".to_string(),
        vec![
            StructField::optional("days", AttributeType::int())
                .with_provider_name("Days")
                .build(),
            StructField::optional("date", crate::s3_lifecycle_date())
                .with_provider_name("Date")
                .build(),
            StructField::required("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .build(),
        ],
    )
}

fn s3_noncurrent_version_expiration() -> AttributeType {
    AttributeType::struct_(
        "NoncurrentVersionExpiration".to_string(),
        vec![
            StructField::optional("noncurrent_days", AttributeType::int())
                .with_provider_name("NoncurrentDays")
                .build(),
            StructField::optional("newer_noncurrent_versions", AttributeType::int())
                .with_provider_name("NewerNoncurrentVersions")
                .build(),
        ],
    )
}

fn s3_noncurrent_version_transition() -> AttributeType {
    AttributeType::struct_(
        "NoncurrentVersionTransition".to_string(),
        vec![
            StructField::optional("noncurrent_days", AttributeType::int())
                .with_provider_name("NoncurrentDays")
                .build(),
            StructField::required("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .build(),
            StructField::optional("newer_noncurrent_versions", AttributeType::int())
                .with_provider_name("NewerNoncurrentVersions")
                .build(),
        ],
    )
}

fn s3_abort_multipart() -> AttributeType {
    AttributeType::struct_(
        "AbortIncompleteMultipartUpload".to_string(),
        vec![
            StructField::optional("days_after_initiation", AttributeType::int())
                .with_provider_name("DaysAfterInitiation")
                .build(),
        ],
    )
}

/// The `And` operator for a lifecycle rule filter — combines a prefix,
/// multiple tags, and object-size bounds. Required by S3 whenever a
/// filter needs more than one condition.
fn s3_lifecycle_filter_and() -> AttributeType {
    AttributeType::struct_(
        "LifecycleRuleAndOperator".to_string(),
        vec![
            StructField::optional("prefix", AttributeType::string())
                .with_provider_name("Prefix")
                .build(),
            StructField::optional("tags", AttributeType::list(s3_filter_tag()))
                .with_provider_name("Tags")
                .with_block_name("tag")
                .build(),
            StructField::optional("object_size_greater_than", AttributeType::int())
                .with_provider_name("ObjectSizeGreaterThan")
                .build(),
            StructField::optional("object_size_less_than", AttributeType::int())
                .with_provider_name("ObjectSizeLessThan")
                .build(),
        ],
    )
}

/// Filter selecting which objects a lifecycle rule applies to. When a
/// rule needs no filter it still requires an empty `<Filter/>` element;
/// the provider emits one automatically if this attribute is omitted.
fn s3_lifecycle_rule_filter() -> AttributeType {
    AttributeType::struct_(
        "LifecycleRuleFilter".to_string(),
        vec![
            StructField::optional("prefix", AttributeType::string())
                .with_provider_name("Prefix")
                .build(),
            StructField::optional("tag", s3_filter_tag())
                .with_provider_name("Tag")
                .with_block_name("tag")
                .build(),
            StructField::optional("object_size_greater_than", AttributeType::int())
                .with_provider_name("ObjectSizeGreaterThan")
                .build(),
            StructField::optional("object_size_less_than", AttributeType::int())
                .with_provider_name("ObjectSizeLessThan")
                .build(),
            StructField::optional("and", s3_lifecycle_filter_and())
                .with_provider_name("And")
                .with_block_name("and")
                .build(),
        ],
    )
}

fn s3_lifecycle_rule() -> AttributeType {
    AttributeType::struct_(
        "LifecycleRule".to_string(),
        vec![
            StructField::optional("id", AttributeType::string())
                .with_provider_name("ID")
                .build(),
            StructField::required("status", s3_lifecycle_status())
                .with_provider_name("Status")
                .build(),
            StructField::optional("filter", s3_lifecycle_rule_filter())
                .with_provider_name("Filter")
                .with_block_name("filter")
                .build(),
            StructField::optional("expiration", s3_lifecycle_expiration())
                .with_provider_name("Expiration")
                .with_block_name("expiration")
                .build(),
            StructField::optional(
                "transitions",
                AttributeType::list(s3_lifecycle_transition()),
            )
            .with_provider_name("Transitions")
            .with_block_name("transition")
            .build(),
            StructField::optional(
                "noncurrent_version_expiration",
                s3_noncurrent_version_expiration(),
            )
            .with_provider_name("NoncurrentVersionExpiration")
            .with_block_name("noncurrent_version_expiration")
            .build(),
            StructField::optional(
                "noncurrent_version_transitions",
                AttributeType::list(s3_noncurrent_version_transition()),
            )
            .with_provider_name("NoncurrentVersionTransitions")
            .with_block_name("noncurrent_version_transition")
            .build(),
            StructField::optional("abort_incomplete_multipart_upload", s3_abort_multipart())
                .with_provider_name("AbortIncompleteMultipartUpload")
                .with_block_name("abort_incomplete_multipart_upload")
                .build(),
        ],
    )
}

pub fn bucket_lifecycle_rules() -> AttributeType {
    AttributeType::list(s3_lifecycle_rule())
}
