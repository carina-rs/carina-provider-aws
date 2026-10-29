use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, optional_bool_attr, optional_enum_attr,
    optional_int_attr, optional_string_attr, retry_aws_operation,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read, take_optional};
use crate::read_models_generated::Ec2VpcReadModel;

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
                api_error_with_meta("Failed to describe VPCs", e).for_resource(id.clone())
            })?;

        if let Some(vpc) = result.vpcs().first() {
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value = Self::extract_ec2_vpc_attributes(vpc, &mut attributes);

            let tags = Self::ec2_tags_to_value(vpc.tags()).unwrap_or_else(empty_map_value);

            // Get VPC attributes for DNS settings (not in Vpc struct)
            let vpc_id = vpc.vpc_id().ok_or_else(|| {
                carina_core::provider::ProviderError::api_error(
                    "DescribeVpcs returned a VPC without VpcId",
                )
                .for_resource(id.clone())
            })?;
            let dns_support = self
                .ec2_client
                .describe_vpc_attribute()
                .vpc_id(vpc_id)
                .attribute(aws_sdk_ec2::types::VpcAttributeName::EnableDnsSupport)
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta("Failed to read VPC DNS support", error)
                        .for_resource(id.clone())
                })?;
            let dns_hostnames = self
                .ec2_client
                .describe_vpc_attribute()
                .vpc_id(vpc_id)
                .attribute(aws_sdk_ec2::types::VpcAttributeName::EnableDnsHostnames)
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta("Failed to read VPC DNS hostnames", error)
                        .for_resource(id.clone())
                })?;

            let ipv4_netmask_length = vpc.cidr_block().and_then(|cidr| {
                cidr.rsplit_once('/').and_then(|(_, prefix)| {
                    prefix
                        .parse::<i64>()
                        .ok()
                        .map(|prefix| Value::Concrete(ConcreteValue::Int(prefix)))
                })
            });
            let model = Ec2VpcReadModel {
                cidr_block: take_optional(&mut attributes, "cidr_block"),
                enable_dns_hostnames: Value::Concrete(ConcreteValue::Bool(
                    dns_hostnames
                        .enable_dns_hostnames()
                        .and_then(|attribute| attribute.value)
                        .unwrap_or(false),
                )),
                enable_dns_support: Value::Concrete(ConcreteValue::Bool(
                    dns_support
                        .enable_dns_support()
                        .and_then(|attribute| attribute.value)
                        .unwrap_or(false),
                )),
                instance_tenancy: take_optional(&mut attributes, "instance_tenancy"),
                ipv4_netmask_length,
                vpc_id: take_optional(&mut attributes, "vpc_id"),
                tags,
            };

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPC
    pub(crate) async fn create_ec2_vpc<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let cidr_block = match optional_string_attr(resource, "cidr_block") {
            Ok(cidr_block) => cidr_block,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let ipv4_ipam_pool_id = match optional_string_attr(resource, "ipv4_ipam_pool_id") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let ipv4_netmask_length = match optional_int_attr(resource, "ipv4_netmask_length") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };

        match (cidr_block, ipv4_ipam_pool_id, ipv4_netmask_length) {
            (Some(_), None, None) | (None, Some(_), Some(_)) => {}
            (None, None, None) => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input(
                        "either cidr_block or ipv4_ipam_pool_id with ipv4_netmask_length is required",
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
            (Some(_), Some(_), _) => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input(
                        "cidr_block and ipv4_ipam_pool_id are mutually exclusive",
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
            (Some(_), None, Some(_)) => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input(
                        "ipv4_netmask_length requires ipv4_ipam_pool_id when cidr_block is set",
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
            (None, Some(_), None) => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input("ipv4_ipam_pool_id requires ipv4_netmask_length")
                        .for_resource(resource.id.clone()),
                ));
            }
            (None, None, Some(_)) => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input("ipv4_netmask_length requires ipv4_ipam_pool_id")
                        .for_resource(resource.id.clone()),
                ));
            }
        }

        // Create VPC with optional instance_tenancy
        let mut create_vpc_builder = self.ec2_client.create_vpc().tag_specifications(
            build_tag_specification_with_create_token(
                resource,
                aws_sdk_ec2::types::ResourceType::Vpc,
                client_token,
            ),
        );
        if let Some(cidr_block) = cidr_block {
            create_vpc_builder = create_vpc_builder.cidr_block(cidr_block);
        }
        if let Some(ipv4_ipam_pool_id) = ipv4_ipam_pool_id {
            create_vpc_builder = create_vpc_builder.ipv4_ipam_pool_id(ipv4_ipam_pool_id);
        }
        if let Some(ipv4_netmask_length) = ipv4_netmask_length {
            let ipv4_netmask_length = match i32::try_from(ipv4_netmask_length) {
                Ok(value) => value,
                Err(_) => {
                    return Err(attempt.untracked(
                        ProviderError::invalid_input(
                            "ipv4_netmask_length is outside the EC2 integer range",
                        )
                        .for_resource(resource.id.clone()),
                    ));
                }
            };
            create_vpc_builder = create_vpc_builder.ipv4_netmask_length(ipv4_netmask_length);
        }

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

        let sent = Self::sent_only(
            resource,
            &[
                "cidr_block",
                "ipv4_ipam_pool_id",
                "ipv4_netmask_length",
                "instance_tenancy",
                "tags",
            ],
        );
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
        resource: &Resource<impl Sized>,
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
                let error = api_error_with_meta("Failed to set DNS support", error)
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
                let error = api_error_with_meta("Failed to set DNS hostnames", error)
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
                    api_error_with_meta("Failed to update DNS support", e).for_resource(id.clone())
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
                    api_error_with_meta("Failed to update DNS hostnames", e)
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
