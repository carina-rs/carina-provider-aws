use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

/// A single object tag (`key` / `value`) used inside S3 lifecycle and
/// replication rule filters.
pub(super) fn s3_filter_tag() -> AttributeType {
    AttributeType::struct_(
        "FilterTag".to_string(),
        vec![
            StructField::new("key", AttributeType::string())
                .with_provider_name("Key")
                .required(),
            StructField::new("value", AttributeType::string())
                .with_provider_name("Value")
                .required(),
        ],
    )
}

/// `Destination` for a replication rule. Minimal fields: target bucket
/// ARN and optional storage class / account.
fn s3_replication_destination() -> AttributeType {
    AttributeType::struct_(
        "ReplicationDestination".to_string(),
        vec![
            StructField::new("bucket", AttributeType::string())
                .with_provider_name("Bucket")
                .required(),
            StructField::new("account", AttributeType::string()).with_provider_name("Account"),
            StructField::new(
                "storage_class",
                enum_with_dsl_aliases(
&[
                        "STANDARD",
                        "REDUCED_REDUNDANCY",
                        "STANDARD_IA",
                        "ONEZONE_IA",
                        "INTELLIGENT_TIERING",
                        "GLACIER",
                        "DEEP_ARCHIVE",
                        "GLACIER_IR",
                    ],
carina_core::schema::enum_identity(
                        "StorageClass",
                        Some("aws.s3.BucketReplicationConfiguration.ReplicationRule.ReplicationDestination"),
                    )
),
            )
            .with_provider_name("StorageClass"),
        ],
    )
}

fn s3_replication_status() -> AttributeType {
    enum_with_dsl_aliases(
        &["Enabled", "Disabled"],
        carina_core::schema::enum_identity(
            "Status",
            Some("aws.s3.BucketReplicationConfiguration.ReplicationRule"),
        ),
    )
}

/// The `And` operator for a replication rule filter — combines a prefix
/// with multiple tags. Required by S3 whenever a filter needs more than
/// one condition.
fn s3_replication_filter_and() -> AttributeType {
    AttributeType::struct_(
        "ReplicationRuleAndOperator".to_string(),
        vec![
            StructField::new("prefix", AttributeType::string()).with_provider_name("Prefix"),
            StructField::new("tags", AttributeType::list(s3_filter_tag()))
                .with_provider_name("Tags")
                .with_block_name("tag"),
        ],
    )
}

/// Filter selecting which objects a replication rule applies to. A V2
/// replication rule requires a `Filter` element; the provider emits an
/// empty one automatically if this attribute is omitted.
fn s3_replication_rule_filter() -> AttributeType {
    AttributeType::struct_(
        "ReplicationRuleFilter".to_string(),
        vec![
            StructField::new("prefix", AttributeType::string()).with_provider_name("Prefix"),
            StructField::new("tag", s3_filter_tag())
                .with_provider_name("Tag")
                .with_block_name("tag"),
            StructField::new("and", s3_replication_filter_and())
                .with_provider_name("And")
                .with_block_name("and"),
        ],
    )
}

/// Whether delete markers are replicated. A V2 replication rule that
/// carries a `Filter` must also declare `DeleteMarkerReplication`; the
/// provider defaults it to `Disabled` when omitted.
fn s3_delete_marker_replication() -> AttributeType {
    AttributeType::struct_(
        "DeleteMarkerReplication".to_string(),
        vec![
            StructField::new(
                "status",
                enum_with_dsl_aliases(
&["Enabled", "Disabled"],
carina_core::schema::enum_identity(
                        "Status",
                        Some("aws.s3.BucketReplicationConfiguration.ReplicationRule.DeleteMarkerReplication"),
                    )
),
            )
            .with_provider_name("Status")
            .required(),
        ],
    )
}

fn s3_replication_rule() -> AttributeType {
    AttributeType::struct_(
        "ReplicationRule".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("ID"),
            StructField::new("priority", AttributeType::int()).with_provider_name("Priority"),
            StructField::new("filter", s3_replication_rule_filter())
                .with_provider_name("Filter")
                .with_block_name("filter"),
            StructField::new("status", s3_replication_status())
                .with_provider_name("Status")
                .required(),
            StructField::new("delete_marker_replication", s3_delete_marker_replication())
                .with_provider_name("DeleteMarkerReplication")
                .with_block_name("delete_marker_replication"),
            StructField::new("destination", s3_replication_destination())
                .with_provider_name("Destination")
                .with_block_name("destination")
                .required(),
        ],
    )
}

pub fn bucket_replication_rules() -> AttributeType {
    AttributeType::list(s3_replication_rule())
}
