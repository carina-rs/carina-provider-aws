use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

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
    AttributeType::custom(
        Some(provider_bare_type(&["s3"], "Grantee")),
        AttributeType::string(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                if s.is_empty() {
                    return Err("Grantee specification must not be empty".to_string());
                }
                // Split by comma and validate each grantee spec
                for part in s.split(',') {
                    let trimmed = part.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let valid_prefixes = ["id=", "emailAddress=", "uri="];
                    if !valid_prefixes.iter().any(|p| trimmed.starts_with(p)) {
                        return Err(format!(
                            "Invalid grantee spec '{}': must start with id=, emailAddress=, or uri=",
                            trimmed
                        ));
                    }
                }
                Ok(())
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
