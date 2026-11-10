//! organizations.Organization schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_FEATURE_SET: &[&str] = &["ALL", "CONSOLIDATED_BILLING"];

pub fn arn() -> AttributeType {
    carina_aws_types::with_member_identity(
        AttributeType::string_member(
            carina_core::schema::StringIdentityLayer::new(None, None, None, None),
            carina_core::schema::StringMemberConstraints::new(
                Some(carina_core::schema::PatternSpec::new(
                    "^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:organization\\/o-[a-z0-9]{10,32}$",
                    carina_core::schema::PatternMatch::Full,
                )),
                None,
            ),
        ),
        carina_aws_types::AwsType::OrganizationsOrganizationArn.definition(),
    )
}

/// Returns the schema config for organizations.Organization (Smithy: com.amazonaws.organizations)
pub fn organizations_organization_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Organization",
        resource_type_name: "organizations.Organization",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("organizations.Organization")
        .with_description("Contains details about an organization. An organization is a collection of accounts that are centrally managed together using consolidated billing, organized hierarchically with organizational units (...")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("feature_set", AttributeType::enum_(carina_core::schema::enum_identity("FeatureSet", Some("aws.organizations.Organization")), Some(vec!["ALL".to_string(), "CONSOLIDATED_BILLING".to_string()]), vec![("ALL".to_string(), "all".to_string()), ("CONSOLIDATED_BILLING".to_string(), "consolidated_billing".to_string())], None, None))
                .create_only()
                .with_description("Specifies the feature set supported by the new organization. Each feature set supports different levels of functionality. CONSOLIDATED_BILLING: All me...")
                .with_provider_name("FeatureSet")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:organization\\/o-[a-z0-9]{10,32}$", carina_core::schema::PatternMatch::Full)), None)), carina_aws_types::AwsType::OrganizationsOrganizationArn.definition()))
                .with_description("The Amazon Resource Name (ARN) of an organization. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the... (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("id", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^o-[a-z0-9]{10,32}$", carina_core::schema::PatternMatch::Full)), None)))
                .with_description("The unique identifier (ID) of an organization. The regex pattern for an organization ID string requires \"o-\" followed by from 10 to 32 lowercase lette... (read-only)")
                .with_provider_name("Id")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("master_account_arn", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:account\\/o-[a-z0-9]{10,32}\\/[0-9]{12}$", carina_core::schema::PatternMatch::Full)), None)), carina_aws_types::AwsType::OrganizationsAccountArn.definition()))
                .with_description("The Amazon Resource Name (ARN) of the account that is designated as the management account for the organization. For more information about ARNs in Or... (read-only)")
                .with_provider_name("MasterAccountArn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("master_account_email", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+@[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+\\.[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+$", carina_core::schema::PatternMatch::Full)), Some((Some(6), Some(64))))), carina_aws_types::AwsType::OrganizationsAccountEmail.definition()))
                .with_description("The email address that is associated with the Amazon Web Services account that is designated as the management account for the organization. (read-only)")
                .with_provider_name("MasterAccountEmail")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("master_account_id", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[0-9]{12}$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(12))))), carina_aws_types::AwsType::AccountId.definition()))
                .with_description("The unique identifier (ID) of the management account of an organization. The regex pattern for an account ID string requires exactly 12 digits. (read-only)")
                .with_provider_name("MasterAccountId")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "organizations.Organization",
        &[("feature_set", VALID_FEATURE_SET)],
    )
}
