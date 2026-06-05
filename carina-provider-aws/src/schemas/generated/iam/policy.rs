//! Auto-generated helper schema module for AWS IAM Policy identifiers
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

pub fn arn() -> AttributeType {
    AttributeType::custom(
        Some(super::provider_type("iam", "Policy", "Arn")),
        super::arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                super::validate_iam_arn(s, "policy/")
                    .map_err(|reason| format!("Invalid IAM Policy ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
