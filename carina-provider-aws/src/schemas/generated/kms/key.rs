//! Auto-generated helper schema module for AWS KMS Key identifiers
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

pub fn arn() -> AttributeType {
    AttributeType::refined_string_with_validator(
        super::provider_type("kms", "Key", "Arn"),
        Some("^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                super::validate_service_arn(value.expose_plaintext(), "kms", Some("key/"))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

pub fn id() -> AttributeType {
    super::kms_key_id()
}
