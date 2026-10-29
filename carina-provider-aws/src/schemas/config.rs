//! AWS schema configuration.

pub use carina_aws_types::*;

use carina_core::schema::ResourceSchema;

/// Generated schema plus AWS-specific resource metadata.
pub struct AwsSchemaConfig {
    /// AWS CloudFormation type name (for example, `AWS::EC2::VPC`).
    pub aws_type_name: &'static str,
    /// Resource type name used in the DSL (for example, `ec2.Vpc`).
    pub resource_type_name: &'static str,
    /// Whether this resource type uses tags.
    pub has_tags: bool,
    /// Resource schema draft; admission happens in [`crate::schemas::all_schemas`].
    pub schema: ResourceSchema,
}
