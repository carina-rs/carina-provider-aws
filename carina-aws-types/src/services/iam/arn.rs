use carina_core::schema::AttributeType;

use crate::provider_type;

/// IAM Role ARN type (e.g., "arn:aws:iam::123456789012:role/MyRole").
pub fn iam_role_arn() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_type("iam", "Role", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$".to_string()),
        None,
        None,
    )
}

/// IAM Policy ARN type (e.g., "arn:aws:iam::123456789012:policy/MyPolicy").
pub fn iam_policy_arn() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_type("iam", "Policy", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$".to_string()),
        None,
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
