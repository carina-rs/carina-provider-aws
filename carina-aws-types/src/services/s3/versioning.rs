use carina_core::schema::{AttributeType, enum_identity};

use crate::enum_with_dsl_aliases;

pub fn s3_versioning_status() -> AttributeType {
    enum_with_dsl_aliases(
        &["Enabled", "Suspended"],
        enum_identity("VersioningStatus", Some("aws.s3.BucketVersioning")),
    )
}

pub fn s3_mfa_delete() -> AttributeType {
    enum_with_dsl_aliases(
        &["Enabled", "Disabled"],
        enum_identity("MFADelete", Some("aws.s3.BucketVersioning")),
    )
}
