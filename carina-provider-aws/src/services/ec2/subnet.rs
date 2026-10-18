use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    optional_bool_attr, optional_bool_struct_field, optional_enum_attr, optional_enum_struct_field,
    require_string_attr,
};
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
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        use aws_sdk_ec2::types::Filter;

        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

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
            let identifier_value = Self::extract_ec2_subnet_attributes(subnet, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(subnet.tags()) {
                attributes.insert("tags".to_string(), tags_value);
            }

            let state = State::existing(id.clone(), attributes);
            Ok(if let Some(id_val) = identifier_value {
                state.with_identifier(id_val)
            } else {
                state
            })
        } else {
            Ok(State::not_found(id.clone()))
        }
    }

    /// Create an EC2 Subnet
    pub(crate) async fn create_ec2_subnet(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let cidr_block = require_string_attr(resource, "cidr_block")?;
        let vpc_id = require_string_attr(resource, "vpc_id")?;
        let modify_enum_attrs = resolve_modify_subnet_enum_attrs(resource, schema)?;

        let mut req = self
            .ec2_client
            .create_subnet()
            .vpc_id(&vpc_id)
            .cidr_block(&cidr_block);

        if let Some(az) = optional_enum_attr(resource, schema, "availability_zone")? {
            req = req.availability_zone(az);
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create subnet", "ec2.CreateSubnet", e)
                .for_resource(resource.id.clone())
        })?;

        let subnet_id = result.subnet().and_then(|s| s.subnet_id()).ok_or_else(|| {
            ProviderError::api_error("Subnet created but no ID returned")
                .for_resource(resource.id.clone())
        })?;

        // Apply tags
        let attrs = resource.resolved_attributes();
        self.apply_ec2_tags(&resource.id, subnet_id, &attrs, None)
            .await?;

        // Apply subnet attributes that require ModifySubnetAttribute
        self.modify_subnet_attributes(&resource.id, subnet_id, resource, &modify_enum_attrs)
            .await?;

        // Read back using subnet ID (reliable identifier)
        self.read_ec2_subnet(&resource.id, Some(subnet_id)).await
    }

    /// Update an EC2 Subnet
    pub(crate) async fn update_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let modify_enum_attrs = resolve_modify_subnet_enum_attrs(&to, schema)?;

        // Apply subnet attributes that require ModifySubnetAttribute
        let attrs = to.resolved_attributes();
        self.modify_subnet_attributes(&id, identifier, &to, &modify_enum_attrs)
            .await?;

        // Update tags
        self.apply_ec2_tags(&id, identifier, &attrs, Some(&from.attributes))
            .await?;

        self.read_ec2_subnet(&id, Some(identifier)).await
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
