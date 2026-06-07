use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{arn, provider_type, validate_iam_arn};

/// IAM Role ARN type (e.g., "arn:aws:iam::123456789012:role/MyRole").
pub fn iam_role_arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("iam", "Role", "Arn")),
        arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_arn(s, "role/")
                    .map_err(|reason| format!("Invalid IAM Role ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// IAM Policy ARN type (e.g., "arn:aws:iam::123456789012:policy/MyPolicy").
pub fn iam_policy_arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("iam", "Policy", "Arn")),
        arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_arn(s, "policy/")
                    .map_err(|reason| format!("Invalid IAM Policy ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// IAM OIDC Provider ARN type (e.g., "arn:aws:iam::123456789012:oidc-provider/token.actions.githubusercontent.com").
pub fn iam_oidc_provider_arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("iam", "OidcProvider", "Arn")),
        arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:oidc-provider/.+$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_arn(s, "oidc-provider/")
                    .map_err(|reason| format!("Invalid IAM OIDC Provider ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
