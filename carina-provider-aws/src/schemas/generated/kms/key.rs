//! Auto-generated helper schema module for AWS KMS Key identifiers
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

use carina_core::schema::AttributeType;

pub fn arn() -> AttributeType {
    AttributeType::refined_string(
        Some(super::provider_type("kms", "Key", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$".to_string()),
        None,
        None,
    )
}

pub fn id() -> AttributeType {
    AttributeType::refined_string(
        Some(super::provider_type("kms", "Key", "Id")),
        None,
        None,
        None,
    )
}
