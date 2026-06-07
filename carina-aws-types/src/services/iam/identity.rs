use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{aws_resource_id, provider_type};

/// Validate IAM Role ID format: starts with "AROA" followed by alphanumeric characters.
pub fn validate_iam_role_id(id: &str) -> Result<(), String> {
    let Some(rest) = id.strip_prefix("AROA") else {
        return Err("must start with 'AROA'".to_string());
    };
    if rest.is_empty() {
        return Err("must have characters after 'AROA' prefix".to_string());
    }
    if !rest.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("characters after prefix must be alphanumeric".to_string());
    }
    Ok(())
}

/// IAM Role ID type (e.g., "AROAEXAMPLEID")
pub fn iam_role_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("iam", "Role", "Id")),
        aws_resource_id(),
        Some("^AROA[A-Z0-9]+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_role_id(s)
                    .map_err(|reason| format!("Invalid IAM Role ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
