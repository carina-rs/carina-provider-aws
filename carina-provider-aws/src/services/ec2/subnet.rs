use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId, State};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, optional_bool_attr, optional_bool_struct_field,
    optional_enum_attr, optional_enum_struct_field, require_string_attr,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use aws_sdk_ec2::types::{AttributeBooleanValue, HostnameType};

// `read_ec2_subnet` must store `availability_zone` as the AWS canonical
// form (`"ap-northeast-1a"`). Rewriting it to a DSL-prefixed identifier
// (`"aws.AvailabilityZone.ZoneName.ap_northeast_1a"`) yields a phantom
// `forces replacement` diff on every post-apply plan. The host lifts API
// spelling at the schema position; provider read paths must not re-derive
// an authored DSL spelling.

#[derive(Debug)]
struct ModifySubnetEnumAttrs {
    hostname_type: Option<HostnameType>,
}

fn resolve_modify_subnet_enum_attrs(
    resource: &Resource,
    schema: &ResourceSchema,
) -> ProviderResult<ModifySubnetEnumAttrs> {
    Ok(ModifySubnetEnumAttrs {
        hostname_type: optional_enum_struct_field(
            resource,
            schema,
            "private_dns_name_options_on_launch",
            "hostname_type",
        )?
        .map(HostnameType::from),
    })
}

impl AwsProvider {
    /// Read an EC2 Subnet
    pub(crate) async fn read_ec2_subnet(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("subnet-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_subnets()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe subnets", "ec2.DescribeSubnets", e)
                    .for_resource(id.clone())
            })?;

        if let Some(subnet) = result.subnets().first() {
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value = Self::extract_ec2_subnet_attributes(subnet, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(subnet.tags()) {
                attributes.insert("tags".to_string(), tags_value);
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

    /// Create an EC2 Subnet
    pub(crate) async fn create_ec2_subnet<A: AwsCreateAttempt>(
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
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };
        match resolve_modify_subnet_enum_attrs(resource, schema) {
            Ok(_) => {}
            Err(error) => return Err(attempt.untracked(error)),
        }

        let mut req = self
            .ec2_client
            .create_subnet()
            .vpc_id(&vpc_id)
            .cidr_block(&cidr_block)
            .tag_specifications(build_tag_specification_with_create_token(
                resource,
                aws_sdk_ec2::types::ResourceType::Subnet,
                client_token,
            ));

        let availability_zone = match optional_enum_attr(resource, schema, "availability_zone") {
            Ok(availability_zone) => availability_zone,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(az) = availability_zone {
            req = req.availability_zone(az);
        }

        let sent = Self::sent_only(
            resource,
            &["vpc_id", "cidr_block", "availability_zone", "tags"],
        );
        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::Subnet.create_context(client_token, sent),
            result,
            |output| {
                output
                    .subnet()
                    .and_then(|subnet| subnet.subnet_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    pub(crate) async fn continue_ec2_subnet_create(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        subnet_id: &str,
    ) -> ProviderResult<()> {
        let modify_enum_attrs = resolve_modify_subnet_enum_attrs(resource, schema)?;
        // Apply subnet attributes that require ModifySubnetAttribute
        self.modify_subnet_attributes(&resource.id, subnet_id, resource, &modify_enum_attrs)
            .await
    }

    /// Update an EC2 Subnet
    pub(crate) async fn update_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let modify_enum_attrs = resolve_modify_subnet_enum_attrs(&to, schema)?;

        // Apply subnet attributes that require ModifySubnetAttribute
        let attrs = to.resolved_attributes();
        self.modify_subnet_attributes(&id, identifier.as_str(), &to, &modify_enum_attrs)
            .await?;

        // Update tags
        self.apply_ec2_tags(&id, identifier.as_str(), &attrs, Some(&from.attributes))
            .await?;

        Ok(self.read_ec2_subnet(&id, identifier).await)
    }

    /// Apply boolean subnet attributes via ModifySubnetAttribute API.
    /// Used by both create (post-creation) and update paths.
    async fn modify_subnet_attributes(
        &self,
        id: &ResourceId,
        subnet_id: &str,
        resource: &Resource,
        enum_attrs: &ModifySubnetEnumAttrs,
    ) -> ProviderResult<()> {
        if let Some(enabled) = optional_bool_attr(resource, "map_public_ip_on_launch") {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .map_public_ip_on_launch(AttributeBooleanValue::builder().value(enabled).build())
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set map_public_ip_on_launch",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        if let Some(enabled) = optional_bool_attr(resource, "assign_ipv6_address_on_creation") {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .assign_ipv6_address_on_creation(
                    AttributeBooleanValue::builder().value(enabled).build(),
                )
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set assign_ipv6_address_on_creation",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        if let Some(enabled) = optional_bool_attr(resource, "enable_dns64") {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .enable_dns64(AttributeBooleanValue::builder().value(enabled).build())
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set enable_dns64",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        // The ModifySubnetAttribute API only allows modifying one attribute at a time.
        // Each private_dns_name_options_on_launch field must be a separate API call.
        if let Some(ht) = enum_attrs.hostname_type.as_ref() {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .private_dns_hostname_type_on_launch(ht.clone())
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set private_dns_name_options_on_launch.hostname_type",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        if let Some(v) = optional_bool_struct_field(
            resource,
            "private_dns_name_options_on_launch",
            "enable_resource_name_dns_a_record",
        ) {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .enable_resource_name_dns_a_record_on_launch(
                    AttributeBooleanValue::builder().value(v).build(),
                )
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set private_dns_name_options_on_launch.enable_resource_name_dns_a_record",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        if let Some(v) = optional_bool_struct_field(
            resource,
            "private_dns_name_options_on_launch",
            "enable_resource_name_dns_aaaa_record",
        ) {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .enable_resource_name_dns_aaaa_record_on_launch(
                    AttributeBooleanValue::builder().value(v).build(),
                )
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set private_dns_name_options_on_launch.enable_resource_name_dns_aaaa_record",
                        "ec2.ModifySubnetAttribute",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::{ConcreteValue, Value};
    use indexmap::IndexMap;

    #[test]
    fn invalid_hostname_type_is_rejected_during_pre_send_resolution() {
        let schema = crate::schemas::generated::ec2::subnet::ec2_subnet_config().schema;
        let mut resource = Resource::with_provider("aws", "ec2.Subnet", "test", None);
        let mut options = IndexMap::new();
        options.insert(
            "hostname_type".to_string(),
            Value::Concrete(ConcreteValue::String("IP-NAME".into())),
        );
        resource.set_attr(
            "private_dns_name_options_on_launch".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        resolve_modify_subnet_enum_attrs(&resource, &schema)
            .expect_err("invalid hostname_type must fail before any EC2 mutation");
    }
}
