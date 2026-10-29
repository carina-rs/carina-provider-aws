//! Auto-generated helper schema module for AWS KMS Key identifiers
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

use carina_core::schema::AttributeType;

pub fn arn() -> AttributeType {
    super::kms_key_arn()
}

pub fn id() -> AttributeType {
    super::kms_key_id()
}
