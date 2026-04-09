//! organization schema definition for AWS
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for organizations.organization (Smithy: com.amazonaws.organizations)
pub fn organizations_organization_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Organization",
        resource_type_name: "organizations.organization",
        has_tags: false,
        schema: ResourceSchema::new("aws.organizations.organization")
            .attribute(
                AttributeSchema::new("id", AttributeType::String)
                    .read_only()
                    .with_description("The unique identifier (ID) of the organization. (read-only)")
                    .with_provider_name("Id"),
            )
            .attribute(
                AttributeSchema::new("arn", super::arn())
                    .read_only()
                    .with_description(
                        "The Amazon Resource Name (ARN) of the organization. (read-only)",
                    )
                    .with_provider_name("Arn"),
            )
            .attribute(
                AttributeSchema::new(
                    "feature_set",
                    AttributeType::StringEnum {
                        name: "FeatureSet".to_string(),
                        values: vec!["ALL".to_string(), "CONSOLIDATED_BILLING".to_string()],
                        namespace: Some("aws.organizations.organization".to_string()),
                        to_dsl: None,
                    },
                )
                .with_description("Specifies the feature set supported by the new organization.")
                .with_provider_name("FeatureSet"),
            )
            .attribute(
                AttributeSchema::new("master_account_id", AttributeType::String)
                    .read_only()
                    .with_description(
                        "The unique identifier (ID) of the management account. (read-only)",
                    )
                    .with_provider_name("MasterAccountId"),
            )
            .attribute(
                AttributeSchema::new("master_account_arn", super::arn())
                    .read_only()
                    .with_description(
                        "The Amazon Resource Name (ARN) of the management account. (read-only)",
                    )
                    .with_provider_name("MasterAccountArn"),
            )
            .attribute(
                AttributeSchema::new("master_account_email", AttributeType::String)
                    .read_only()
                    .with_description(
                        "The email address associated with the management account. (read-only)",
                    )
                    .with_provider_name("MasterAccountEmail"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "organizations.organization",
        &[("feature_set", &["ALL", "CONSOLIDATED_BILLING"])],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
