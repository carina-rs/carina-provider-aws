use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, optional_bool_attr, optional_enum_attr,
    require_string_attr, retry_aws_operation,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

impl AwsProvider {
    /// Read an EC2 VPC
    pub(crate) async fn read_ec2_vpc(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder().name("vpc-id").values(identifier).build();

        let result = self
            .ec2_client
            .describe_vpcs()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe VPCs", "ec2.DescribeVpcs", e)
                    .for_resource(id.clone())
            })?;

        if let Some(vpc) = result.vpcs().first() {
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value = Self::extract_ec2_vpc_attributes(vpc, &mut attributes);

            // Extract user-defined tags (excluding Name)
            if let Some(tags_value) = Self::ec2_tags_to_value(vpc.tags()) {
                attributes.insert("tags".to_string(), tags_value);
            }

            // Get VPC attributes for DNS settings (not in Vpc struct)
            if let Some(vpc_id) = vpc.vpc_id() {
                if let Ok(dns_support) = self
                    .ec2_client
                    .describe_vpc_attribute()
                    .vpc_id(vpc_id)
                    .attribute(aws_sdk_ec2::types::VpcAttributeName::EnableDnsSupport)
                    .send()
                    .await
                    && let Some(attr) = dns_support.enable_dns_support()
                {
                    attributes.insert(
                        "enable_dns_support".to_string(),
                        Value::Concrete(ConcreteValue::Bool(attr.value.unwrap_or(false))),
                    );
                }

                if let Ok(dns_hostnames) = self
                    .ec2_client
                    .describe_vpc_attribute()
                    .vpc_id(vpc_id)
                    .attribute(aws_sdk_ec2::types::VpcAttributeName::EnableDnsHostnames)
                    .send()
                    .await
                    && let Some(attr) = dns_hostnames.enable_dns_hostnames()
                {
                    attributes.insert(
                        "enable_dns_hostnames".to_string(),
                        Value::Concrete(ConcreteValue::Bool(attr.value.unwrap_or(false))),
                    );
                }
            }

            Ok(crate::helpers::found_read(
                id,
                applied_identifier,
                attributes,
            ))
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPC
    pub(crate) async fn create_ec2_vpc<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let cidr_block = match require_string_attr(resource, "cidr_block") {
            Ok(cidr_block) => cidr_block,
            Err(error) => return Err(attempt.untracked(error)),
        };

        // Create VPC with optional instance_tenancy
        let mut create_vpc_builder = self
            .ec2_client
            .create_vpc()
            .cidr_block(&cidr_block)
            .tag_specifications(build_tag_specification_with_create_token(
                resource,
                aws_sdk_ec2::types::ResourceType::Vpc,
                client_token,
            ));

        // Handle instance_tenancy if specified
        let tenancy = match optional_enum_attr(resource, schema, "instance_tenancy") {
            Ok(tenancy) => tenancy,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(tenancy) = tenancy {
            let tenancy_enum = match tenancy {
                "dedicated" => aws_sdk_ec2::types::Tenancy::Dedicated,
                "host" => aws_sdk_ec2::types::Tenancy::Host,
                _ => aws_sdk_ec2::types::Tenancy::Default,
            };
            create_vpc_builder = create_vpc_builder.instance_tenancy(tenancy_enum);
        }

        let sent = Self::sent_only(resource, &["cidr_block", "instance_tenancy", "tags"]);
        let result =
            retry_aws_operation(RetryPolicy::default(), || create_vpc_builder.clone()).await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::Vpc.create_context(client_token, sent),
            result,
            |output| output.vpc().and_then(|vpc| vpc.vpc_id()).map(str::to_owned),
        )
        .await
    }

    pub(crate) async fn continue_ec2_vpc_create(
        &self,
        resource: &Resource,
        vpc_id: &str,
    ) -> ProviderResult<()> {
        // Configure DNS support
        if let Some(enabled) = optional_bool_attr(resource, "enable_dns_support") {
            let result = self
                .ec2_client
                .modify_vpc_attribute()
                .vpc_id(vpc_id)
                .enable_dns_support(
                    aws_sdk_ec2::types::AttributeBooleanValue::builder()
                        .value(enabled)
                        .build(),
                )
                .send()
                .await;
            if let Err(error) = result {
                let error = api_error_with_meta(
                    "Failed to set DNS support",
                    "ec2.ModifyVpcAttribute",
                    error,
                )
                .for_resource(resource.id.clone());
                return Err(error);
            }
        }

        // Configure DNS hostnames
        if let Some(enabled) = optional_bool_attr(resource, "enable_dns_hostnames") {
            let result = self
                .ec2_client
                .modify_vpc_attribute()
                .vpc_id(vpc_id)
                .enable_dns_hostnames(
                    aws_sdk_ec2::types::AttributeBooleanValue::builder()
                        .value(enabled)
                        .build(),
                )
                .send()
                .await;
            if let Err(error) = result {
                let error = api_error_with_meta(
                    "Failed to set DNS hostnames",
                    "ec2.ModifyVpcAttribute",
                    error,
                )
                .for_resource(resource.id.clone());
                return Err(error);
            }
        }
        Ok(())
    }

    /// Update an EC2 VPC
    pub(crate) async fn update_ec2_vpc(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // identifier is the VPC ID (e.g., vpc-12345678)
        let vpc_id = identifier.to_string();

        // Update DNS support
        if let Some(enabled) = optional_bool_attr(&to, "enable_dns_support") {
            self.ec2_client
                .modify_vpc_attribute()
                .vpc_id(&vpc_id)
                .enable_dns_support(
                    aws_sdk_ec2::types::AttributeBooleanValue::builder()
                        .value(enabled)
                        .build(),
                )
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to update DNS support", "ec2.ModifyVpcAttribute", e)
                        .for_resource(id.clone())
                })?;
        }

        // Update DNS hostnames
        if let Some(enabled) = optional_bool_attr(&to, "enable_dns_hostnames") {
            self.ec2_client
                .modify_vpc_attribute()
                .vpc_id(&vpc_id)
                .enable_dns_hostnames(
                    aws_sdk_ec2::types::AttributeBooleanValue::builder()
                        .value(enabled)
                        .build(),
                )
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to update DNS hostnames",
                        "ec2.ModifyVpcAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        // Update tags
        self.apply_ec2_tags(
            &id,
            &vpc_id,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        Ok(self.read_ec2_vpc(&id, identifier).await)
    }
}
