use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{provider_type, validate_iam_arn};

/// IAM Role ARN type (e.g., "arn:aws:iam::123456789012:role/MyRole").
pub fn iam_role_arn() -> AttributeType {
    AttributeType::refined_string_with_validator(
        provider_type("iam", "Role", "Arn"),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_iam_arn(value.expose_plaintext(), "role/")
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// IAM Policy ARN type (e.g., "arn:aws:iam::123456789012:policy/MyPolicy").
pub fn iam_policy_arn() -> AttributeType {
    AttributeType::refined_string_with_validator(
        provider_type("iam", "Policy", "Arn"),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_iam_arn(value.expose_plaintext(), "policy/")
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// IAM OIDC Provider ARN type (e.g., "arn:aws:iam::123456789012:oidc-provider/token.actions.githubusercontent.com").
pub fn iam_oidc_provider_arn() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_type("iam", "OidcProvider", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:oidc-provider/.+$".to_string()),
        None,
        None,
    )
}
