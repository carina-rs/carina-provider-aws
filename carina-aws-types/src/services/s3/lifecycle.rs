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
            StructField::new("days", AttributeType::int()).with_provider_name("Days"),
            StructField::new("date", AttributeType::string()).with_provider_name("Date"),
            StructField::new("expired_object_delete_marker", AttributeType::bool())
                .with_provider_name("ExpiredObjectDeleteMarker"),
        ],
    )
}

fn s3_lifecycle_transition() -> AttributeType {
    AttributeType::struct_(
        "LifecycleTransition".to_string(),
        vec![
            StructField::new("days", AttributeType::int()).with_provider_name("Days"),
            StructField::new("date", AttributeType::string()).with_provider_name("Date"),
            StructField::new("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .required(),
        ],
    )
}

fn s3_noncurrent_version_expiration() -> AttributeType {
    AttributeType::struct_(
        "NoncurrentVersionExpiration".to_string(),
        vec![
            StructField::new("noncurrent_days", AttributeType::int())
                .with_provider_name("NoncurrentDays"),
            StructField::new("newer_noncurrent_versions", AttributeType::int())
                .with_provider_name("NewerNoncurrentVersions"),
        ],
    )
}

fn s3_noncurrent_version_transition() -> AttributeType {
    AttributeType::struct_(
        "NoncurrentVersionTransition".to_string(),
        vec![
            StructField::new("noncurrent_days", AttributeType::int())
                .with_provider_name("NoncurrentDays"),
            StructField::new("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .required(),
            StructField::new("newer_noncurrent_versions", AttributeType::int())
                .with_provider_name("NewerNoncurrentVersions"),
        ],
    )
}

fn s3_abort_multipart() -> AttributeType {
    AttributeType::struct_(
        "AbortIncompleteMultipartUpload".to_string(),
        vec![
            StructField::new("days_after_initiation", AttributeType::int())
                .with_provider_name("DaysAfterInitiation"),
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
            StructField::new("prefix", AttributeType::string()).with_provider_name("Prefix"),
            StructField::new("tags", AttributeType::list(s3_filter_tag()))
                .with_provider_name("Tags")
                .with_block_name("tag"),
            StructField::new("object_size_greater_than", AttributeType::int())
                .with_provider_name("ObjectSizeGreaterThan"),
            StructField::new("object_size_less_than", AttributeType::int())
                .with_provider_name("ObjectSizeLessThan"),
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
            StructField::new("prefix", AttributeType::string()).with_provider_name("Prefix"),
            StructField::new("tag", s3_filter_tag())
                .with_provider_name("Tag")
                .with_block_name("tag"),
            StructField::new("object_size_greater_than", AttributeType::int())
                .with_provider_name("ObjectSizeGreaterThan"),
            StructField::new("object_size_less_than", AttributeType::int())
                .with_provider_name("ObjectSizeLessThan"),
            StructField::new("and", s3_lifecycle_filter_and())
                .with_provider_name("And")
                .with_block_name("and"),
        ],
    )
}

fn s3_lifecycle_rule() -> AttributeType {
    AttributeType::struct_(
        "LifecycleRule".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("ID"),
            StructField::new("status", s3_lifecycle_status())
                .with_provider_name("Status")
                .required(),
            StructField::new("filter", s3_lifecycle_rule_filter())
                .with_provider_name("Filter")
                .with_block_name("filter"),
            StructField::new("expiration", s3_lifecycle_expiration())
                .with_provider_name("Expiration")
                .with_block_name("expiration"),
            StructField::new(
                "transitions",
                AttributeType::list(s3_lifecycle_transition()),
            )
            .with_provider_name("Transitions")
            .with_block_name("transition"),
            StructField::new(
                "noncurrent_version_expiration",
                s3_noncurrent_version_expiration(),
            )
            .with_provider_name("NoncurrentVersionExpiration")
            .with_block_name("noncurrent_version_expiration"),
            StructField::new(
                "noncurrent_version_transitions",
                AttributeType::list(s3_noncurrent_version_transition()),
            )
            .with_provider_name("NoncurrentVersionTransitions")
            .with_block_name("noncurrent_version_transition"),
            StructField::new("abort_incomplete_multipart_upload", s3_abort_multipart())
                .with_provider_name("AbortIncompleteMultipartUpload")
                .with_block_name("abort_incomplete_multipart_upload"),
        ],
    )
}

pub fn bucket_lifecycle_rules() -> AttributeType {
    AttributeType::list(s3_lifecycle_rule())
}
