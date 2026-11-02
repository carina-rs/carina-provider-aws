use carina_core::schema::{AttributeType, enum_identity};

use crate::enum_with_dsl_aliases;

pub fn s3_object_ownership() -> AttributeType {
    enum_with_dsl_aliases(
        &[
            "BucketOwnerEnforced",
            "BucketOwnerPreferred",
            "ObjectWriter",
        ],
        enum_identity("ObjectOwnership", Some("aws.s3.BucketOwnershipControls")),
    )
}
