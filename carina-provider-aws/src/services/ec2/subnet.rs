use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, optional_bool_attr, optional_bool_struct_field,
    optional_enum_attr, optional_enum_struct_field, optional_int_attr, optional_string_attr,
    require_string_attr,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::provider_generated::Ec2SubnetReadFields;
use crate::read_model::{empty_map_value, found_managed_read, require_read_value};
use crate::read_models_generated::Ec2SubnetReadModel;
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
    enable_lni_at_device_index: Option<i32>,
}

fn resolve_modify_subnet_enum_attrs(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<ModifySubnetEnumAttrs> {
    let enable_lni_at_device_index = optional_int_attr(resource, "enable_lni_at_device_index")?
        .map(|value| {
            i32::try_from(value).map_err(|_| {
                ProviderError::invalid_input(
                    "enable_lni_at_device_index is outside the EC2 integer range",
                )
                .for_resource(resource.id.clone())
            })
        })
        .transpose()?;
    Ok(ModifySubnetEnumAttrs {
        hostname_type: optional_enum_struct_field(
            resource,
            schema,
            "private_dns_name_options_on_launch",
            "hostname_type",
        )?
        .map(HostnameType::from),
        enable_lni_at_device_index,
    })
}

fn validate_subnet_cidr_source(
    id: &ResourceId,
    family: &str,
    cidr: Option<&str>,
    pool_id: Option<&str>,
    netmask_length: Option<i64>,
    required: bool,
) -> ProviderResult<()> {
    match (cidr, pool_id, netmask_length) {
        (Some(_), None, None) | (None, Some(_), Some(_)) | (None, None, None) if !required => {
            Ok(())
        }
        (Some(_), None, None) | (None, Some(_), Some(_)) => Ok(()),
        (None, None, None) => Err(ProviderError::invalid_input(format!(
            "an {family} CIDR block or {family} IPAM pool with netmask length is required"
        ))
        .for_resource(id.clone())),
        (Some(_), Some(_), _) => Err(ProviderError::invalid_input(format!(
            "{family} CIDR block and {family} IPAM pool ID are mutually exclusive"
        ))
        .for_resource(id.clone())),
        (Some(_), None, Some(_)) => Err(ProviderError::invalid_input(format!(
            "{family} netmask length may only be used with an {family} IPAM pool ID"
        ))
        .for_resource(id.clone())),
        (None, Some(_), None) => Err(ProviderError::invalid_input(format!(
            "{family} IPAM pool ID requires an {family} netmask length"
        ))
        .for_resource(id.clone())),
        (None, None, Some(_)) => Err(ProviderError::invalid_input(format!(
            "{family} netmask length requires an {family} IPAM pool ID"
        ))
        .for_resource(id.clone())),
    }
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
                api_error_with_meta("Failed to describe subnets", e).for_resource(id.clone())
            })?;

        if let Some(subnet) = result.subnets().first() {
            let Ec2SubnetReadFields {
                assign_ipv6_address_on_creation,
                availability_zone,
                availability_zone_id,
                cidr_block,
                enable_dns64,
                enable_lni_at_device_index,
                ipv6_native,
                map_public_ip_on_launch,
                outpost_arn,
                private_dns_name_options_on_launch,
                subnet_id,
                vpc_id,
                applied_identifier: _,
            } = Self::extract_ec2_subnet_attributes(subnet);

            let tags = Self::ec2_tags_to_value(subnet.tags()).unwrap_or_else(empty_map_value);
            let prefix_value = |cidr: &str| {
                cidr.rsplit_once('/').and_then(|(_, prefix)| {
                    prefix
                        .parse::<i64>()
                        .ok()
                        .map(|prefix| Value::Concrete(ConcreteValue::Int(prefix)))
                })
            };
            let ipv6_cidr_block = subnet
                .ipv6_cidr_block_association_set()
                .iter()
                .find_map(|association| association.ipv6_cidr_block())
                .map(|cidr| Value::Concrete(ConcreteValue::String(cidr.to_string().into())));
            let ipv6_netmask_length = subnet
                .ipv6_cidr_block_association_set()
                .iter()
                .find_map(|association| association.ipv6_cidr_block())
                .and_then(prefix_value);
            let model = Ec2SubnetReadModel {
                assign_ipv6_address_on_creation,
                availability_zone,
                availability_zone_id,
                cidr_block,
                enable_dns64,
                enable_lni_at_device_index,
                ipv4_netmask_length: subnet.cidr_block().and_then(prefix_value),
                ipv6_cidr_block,
                ipv6_native,
                ipv6_netmask_length,
                map_public_ip_on_launch,
                outpost_arn,
                private_dns_name_options_on_launch,
                vpc_id: require_read_value(id, vpc_id, "vpc_id")?,
                subnet_id,
                tags,
            };

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Subnet
    pub(crate) async fn create_ec2_subnet<A: AwsCreateAttempt>(
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
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let _modify_attrs = match resolve_modify_subnet_enum_attrs(resource, schema) {
            Ok(attrs) => attrs,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let availability_zone_id = match optional_string_attr(resource, "availability_zone_id") {
            Ok(value) => value,
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
        let ipv6_cidr_block = match optional_string_attr(resource, "ipv6_cidr_block") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let ipv6_ipam_pool_id = match optional_string_attr(resource, "ipv6_ipam_pool_id") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let ipv6_netmask_length = match optional_int_attr(resource, "ipv6_netmask_length") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let outpost_arn = match optional_string_attr(resource, "outpost_arn") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let ipv6_native = optional_bool_attr(resource, "ipv6_native").unwrap_or(false);

        if let Err(error) = validate_subnet_cidr_source(
            &resource.id,
            "IPv4",
            cidr_block,
            ipv4_ipam_pool_id,
            ipv4_netmask_length,
            !ipv6_native,
        ) {
            return Err(attempt.untracked(error));
        }
        if let Err(error) = validate_subnet_cidr_source(
            &resource.id,
            "IPv6",
            ipv6_cidr_block,
            ipv6_ipam_pool_id,
            ipv6_netmask_length,
            ipv6_native,
        ) {
            return Err(attempt.untracked(error));
        }

        let mut req = self
            .ec2_client
            .create_subnet()
            .vpc_id(&vpc_id)
            .tag_specifications(build_tag_specification_with_create_token(
                resource,
                aws_sdk_ec2::types::ResourceType::Subnet,
                client_token,
            ));
        if let Some(cidr_block) = cidr_block {
            req = req.cidr_block(cidr_block);
        }
        if let Some(availability_zone_id) = availability_zone_id {
            req = req.availability_zone_id(availability_zone_id);
        }
        if let Some(ipv4_ipam_pool_id) = ipv4_ipam_pool_id {
            req = req.ipv4_ipam_pool_id(ipv4_ipam_pool_id);
        }
        if let Some(ipv4_netmask_length) = ipv4_netmask_length {
            req = req.ipv4_netmask_length(ipv4_netmask_length as i32);
        }
        if let Some(ipv6_cidr_block) = ipv6_cidr_block {
            req = req.ipv6_cidr_block(ipv6_cidr_block);
        }
        if let Some(ipv6_ipam_pool_id) = ipv6_ipam_pool_id {
            req = req.ipv6_ipam_pool_id(ipv6_ipam_pool_id);
        }
        if let Some(ipv6_netmask_length) = ipv6_netmask_length {
            req = req.ipv6_netmask_length(ipv6_netmask_length as i32);
        }
        if resource.get_attr("ipv6_native").is_some() {
            req = req.ipv6_native(ipv6_native);
        }
        if let Some(outpost_arn) = outpost_arn {
            req = req.outpost_arn(outpost_arn);
        }

        let availability_zone = match optional_enum_attr(resource, schema, "availability_zone") {
            Ok(availability_zone) => availability_zone,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(az) = availability_zone {
            if availability_zone_id.is_some() {
                return Err(attempt.untracked(
                    ProviderError::invalid_input(
                        "availability_zone and availability_zone_id are mutually exclusive",
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
            req = req.availability_zone(az);
        }

        let sent = Self::sent_only(
            resource,
            &[
                "vpc_id",
                "cidr_block",
                "availability_zone",
                "availability_zone_id",
                "ipv4_ipam_pool_id",
                "ipv4_netmask_length",
                "ipv6_cidr_block",
                "ipv6_ipam_pool_id",
                "ipv6_native",
                "ipv6_netmask_length",
                "outpost_arn",
                "tags",
            ],
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
        resource: &Resource<impl Sized>,
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
        resource: &Resource<impl Sized>,
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
                    api_error_with_meta("Failed to set map_public_ip_on_launch", e)
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
                    api_error_with_meta("Failed to set assign_ipv6_address_on_creation", e)
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
                    api_error_with_meta("Failed to set enable_dns64", e).for_resource(id.clone())
                })?;
        }

        if let Some(device_index) = enum_attrs.enable_lni_at_device_index {
            self.ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id)
                .enable_lni_at_device_index(device_index)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to set enable_lni_at_device_index", e)
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
