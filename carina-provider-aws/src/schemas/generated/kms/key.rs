//! Auto-generated helper schema module for AWS KMS Key identifiers
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

pub fn arn() -> AttributeType {
    AttributeType::custom(
        Some(super::provider_type("kms", "Key", "Arn")),
        super::arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                super::validate_service_arn(s, "kms", Some("key/"))
                    .map_err(|reason| format!("Invalid KMS Key ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

pub fn id() -> AttributeType {
    AttributeType::custom(
        Some(super::provider_type("kms", "Key", "Id")),
        super::aws_resource_id(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                super::validate_kms_key_id(s)
                    .map_err(|reason| format!("Invalid KMS key identifier '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
