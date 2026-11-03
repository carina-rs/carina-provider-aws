//! Input boundary for the idempotent S3 DeleteObjects operation.

use carina_aws_types::sdk::s3::types::{Delete, ObjectIdentifier};

/// Identifies one object version, including S3's literal `"null"` version ID.
/// Private fields and the required version ID prevent key-only deletions.
pub(crate) struct VersionedObjectId {
    key: String,
    version_id: String,
}

impl VersionedObjectId {
    pub(crate) fn new(key: impl Into<String>, version_id: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            version_id: version_id.into(),
        }
    }
}

pub(crate) fn versioned_delete(
    objects: Vec<VersionedObjectId>,
) -> Result<Delete, aws_smithy_types::error::operation::BuildError> {
    let objects = objects
        .into_iter()
        .map(|object| {
            ObjectIdentifier::builder()
                .key(object.key)
                .version_id(object.version_id)
                .build()
        })
        .collect::<Result<Vec<_>, _>>()?;

    Delete::builder()
        .set_objects(Some(objects))
        .quiet(true)
        .build()
}
