use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use super::provider_bare_type;

// ========== AWS Account ID ==========

/// Validate a 12-digit AWS Account ID.
pub fn validate_aws_account_id(id: &str) -> Result<(), String> {
    if id.len() != 12 {
        return Err(format!(
            "must be exactly 12 digits, got {} characters",
            id.len()
        ));
    }
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("must contain only digits".to_string());
    }
    Ok(())
}

/// AWS Account ID type (12-digit numeric string, e.g., "123456789012")
pub fn aws_account_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_bare_type(&[], "AccountId")),
        AttributeType::string(),
        Some("^\\d{12}$".to_string()),
        Some((Some(12), Some(12))),
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_aws_account_id(s)
                    .map_err(|reason| format!("Invalid AWS Account ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
