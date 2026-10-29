//! SecurityGroup schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

/// Returns the schema config for ec2.SecurityGroup (Smithy: com.amazonaws.ec2)
pub fn ec2_security_group_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::SecurityGroup",
        resource_type_name: "ec2.SecurityGroup",
        has_tags: true,
        schema: ResourceSchema::new("ec2.SecurityGroup")
        .with_description("Describes a security group.")
        .with_unique_name_attribute("group_name")
        .attribute(
            AttributeSchema::required("description", AttributeType::string())
                .create_only()
                .with_description("A description for the security group. Constraints: Up to 255 characters in length Valid characters: a-z, A-Z, 0-9, spaces, and ._-:/()#,@[]+=&;{}!$*")
                .with_provider_name("Description")
                .build(),
        )
        .attribute(
            AttributeSchema::required("group_name", AttributeType::string())
                .create_only()
                .with_description("The name of the security group. Names are case-insensitive and must be unique within the VPC. Constraints: Up to 255 characters in length. Can't start...")
                .with_provider_name("GroupName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("vpc_id", super::vpc_id())
                .create_only()
                .with_description("The ID of the VPC. Required for a nondefault VPC.")
                .with_provider_name("VpcId")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("group_id", super::security_group_id())
                .with_description("The ID of the security group. (read-only)")
                .with_provider_name("GroupId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.SecurityGroup", &[])
}
