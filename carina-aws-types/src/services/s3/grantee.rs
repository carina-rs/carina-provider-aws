use carina_core::schema::AttributeType;

use crate::provider_bare_type;

// ========== S3 helpers ==========

/// S3 grantee specification type with validation
///
/// Validates that the value contains at least one grantee spec in the format:
/// - `id="canonical-user-id"`
/// - `emailAddress="user@example.com"`
/// - `uri="http://acs.amazonaws.com/groups/global/AllUsers"`
///
/// Multiple grantees can be comma-separated.
pub fn s3_grantee() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_bare_type(&["s3"], "Grantee")),
        Some(r"^(id|emailAddress|uri)=.+(\s*,\s*(id|emailAddress|uri)=.+)*$".to_string()),
        None,
        None,
    )
}
