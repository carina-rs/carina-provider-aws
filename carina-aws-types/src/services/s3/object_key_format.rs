use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

fn s3_partition_date_source() -> AttributeType {
    enum_with_dsl_aliases(
        &["EventTime", "DeliveryTime"],
        carina_core::schema::enum_identity(
            "PartitionDateSource",
            Some("aws.s3.BucketLogging.TargetObjectKeyFormat.PartitionedPrefix"),
        ),
    )
}

fn s3_partitioned_prefix() -> AttributeType {
    AttributeType::struct_(
        "PartitionedPrefix".to_string(),
        vec![
            StructField::new("partition_date_source", s3_partition_date_source())
                .with_provider_name("PartitionDateSource"),
        ],
    )
}

fn s3_simple_prefix() -> AttributeType {
    AttributeType::struct_("SimplePrefix".to_string(), vec![])
}

pub fn bucket_target_object_key_format() -> AttributeType {
    AttributeType::struct_(
        "TargetObjectKeyFormat".to_string(),
        vec![
            StructField::new("simple_prefix", s3_simple_prefix())
                .with_provider_name("SimplePrefix"),
            StructField::new("partitioned_prefix", s3_partitioned_prefix())
                .with_provider_name("PartitionedPrefix"),
        ],
    )
}
