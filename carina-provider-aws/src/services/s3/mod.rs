pub mod bucket;
pub mod bucket_acl;
pub mod bucket_cors;
pub mod bucket_encryption;
pub mod bucket_lifecycle;
pub mod bucket_logging;
pub mod bucket_notification;
pub mod bucket_object_lock_configuration;
pub mod bucket_ownership_controls;
pub mod bucket_policy;
pub mod bucket_public_access_block;
pub mod bucket_replication;
pub mod bucket_versioning;
pub mod bucket_website;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InvalidBucketStateReason {
    VersioningNotEnabled,
    ObjectLockConfigurationPresent,
    Other,
}

pub(super) fn classify_invalid_bucket_state(
    code: Option<&str>,
    message: Option<&str>,
) -> InvalidBucketStateReason {
    if code != Some("InvalidBucketState") {
        return InvalidBucketStateReason::Other;
    }

    match message {
        Some(message) if message.contains("Versioning must be 'Enabled' on the bucket") => {
            InvalidBucketStateReason::VersioningNotEnabled
        }
        Some(message) if message.contains("Object Lock configuration is present") => {
            InvalidBucketStateReason::ObjectLockConfigurationPresent
        }
        _ => InvalidBucketStateReason::Other,
    }
}
