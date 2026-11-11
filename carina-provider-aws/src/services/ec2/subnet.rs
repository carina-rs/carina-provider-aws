use crate::helpers::AttributeChange;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, SnapshotState,
    Value,
};
use carina_core::schema::CompiledResourceSchema;
use carina_plugin_sdk::{DesiredScalar, DesiredValueMut, DesiredView};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, optional_bool_struct_field,
    optional_enum_struct_field,
};
#[cfg(test)]
use crate::helpers::{optional_bool_attr, optional_int_attr};
use crate::provider::{AwsCreateAttempt, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2SubnetReadModelSupplement;
use crate::send_models_generated::{Ec2SubnetCreateInput, Ec2SubnetUpdateInput};
use carina_aws_types::sdk::ec2::types::{AttributeBooleanValue, HostnameType};
use std::net::Ipv6Addr;

// `read_ec2_subnet` must store `availability_zone` as the AWS canonical
// form (`"ap-northeast-1a"`). Rewriting it to a DSL-prefixed identifier
// (`"aws.AvailabilityZone.ZoneName.ap_northeast_1a"`) yields a phantom
// `forces replacement` diff on every post-apply plan. The host lifts API
// spelling at the schema position; provider read paths must not re-derive
// an authored DSL spelling.

#[derive(Debug)]
enum LniChange {
    Enable(i32),
    Disable,
}

#[derive(Debug)]
struct ModifySubnetAttrs {
    assign_ipv6_address_on_creation: Option<bool>,
    enable_dns64: Option<bool>,
    hostname_type: Option<HostnameType>,
    lni: Option<LniChange>,
    map_public_ip_on_launch: Option<bool>,
    enable_resource_name_dns_a_record: Option<bool>,
    enable_resource_name_dns_aaaa_record: Option<bool>,
}

#[cfg(test)]
fn resolve_modify_subnet_enum_attrs(
    resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
    schema: &CompiledResourceSchema,
) -> ProviderResult<ModifySubnetAttrs> {
    resolve_modify_subnet_attrs(
        &resource.id,
        schema,
        optional_bool_attr(resource, "assign_ipv6_address_on_creation"),
        optional_bool_attr(resource, "enable_dns64"),
        optional_int_attr(resource, "enable_lni_at_device_index")?,
        optional_bool_attr(resource, "map_public_ip_on_launch"),
        resource.get_attr("private_dns_name_options_on_launch"),
    )
}

fn resolve_modify_subnet_attrs(
    id: &ResourceId,
    schema: &CompiledResourceSchema,
    assign_ipv6_address_on_creation: Option<bool>,
    enable_dns64: Option<bool>,
    enable_lni_at_device_index: Option<i64>,
    map_public_ip_on_launch: Option<bool>,
    private_dns_name_options_on_launch: Option<&Value>,
) -> ProviderResult<ModifySubnetAttrs> {
    let enable_lni_at_device_index = enable_lni_at_device_index
        .map(|value| {
            i32::try_from(value).map_err(|_| {
                ProviderError::invalid_input(
                    "enable_lni_at_device_index is outside the EC2 integer range",
                )
                .for_resource(id.clone())
            })
        })
        .transpose()?;
    let mut projected = Resource::from_id(id.clone());
    if let Some(value) = private_dns_name_options_on_launch {
        projected.attributes.insert(
            "private_dns_name_options_on_launch".to_string(),
            value.clone(),
        );
    }
    Ok(ModifySubnetAttrs {
        assign_ipv6_address_on_creation,
        enable_dns64,
        hostname_type: optional_enum_struct_field(
            &projected,
            schema,
            "private_dns_name_options_on_launch",
            "hostname_type",
        )?
        .map(HostnameType::from),
        lni: enable_lni_at_device_index.map(LniChange::Enable),
        map_public_ip_on_launch,
        enable_resource_name_dns_a_record: optional_bool_struct_field(
            &projected,
            "private_dns_name_options_on_launch",
            "enable_resource_name_dns_a_record",
        ),
        enable_resource_name_dns_aaaa_record: optional_bool_struct_field(
            &projected,
            "private_dns_name_options_on_launch",
            "enable_resource_name_dns_aaaa_record",
        ),
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
        (Some(_), None | Some(_), None) | (None, Some(_), Some(_)) | (None, None, None)
            if !required =>
        {
            Ok(())
        }
        (Some(_), None | Some(_), None) | (None, Some(_), Some(_)) => Ok(()),
        (None, None, None) => Err(ProviderError::invalid_input(format!(
            "an {family} CIDR block or {family} IPAM pool with netmask length is required"
        ))
        .for_resource(id.clone())),
        (Some(_), _, Some(_)) => Err(ProviderError::invalid_input(format!(
            "{family} CIDR block and {family} netmask length are mutually exclusive"
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

fn canonical_ipv6_cidr(value: &str) -> Result<String, String> {
    let (address, prefix) = value
        .rsplit_once('/')
        .ok_or_else(|| "IPv6 CIDR must include a prefix length".to_string())?;
    let address = address
        .parse::<Ipv6Addr>()
        .map_err(|error| format!("invalid IPv6 address: {error}"))?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|error| format!("invalid IPv6 prefix length: {error}"))?;
    if prefix > 128 {
        return Err("IPv6 prefix length must be between 0 and 128".to_string());
    }

    // Ipv6Addr's Display uses dotted decimal for IPv4-mapped addresses,
    // but the slot's validator admits only hex groups.
    let segments = address.segments();
    let mut longest = 0..0;
    let mut start = 0;
    for end in 0..=segments.len() {
        if end == segments.len() || segments[end] != 0 {
            // Keep the first longest run; never compress a single zero group.
            if end - start > longest.len() {
                longest = start..end;
            }
            start = end + 1;
        }
    }
    let hex_groups = |groups: &[u16]| {
        groups
            .iter()
            .map(|group| format!("{group:x}"))
            .collect::<Vec<_>>()
            .join(":")
    };
    let address = if longest.len() >= 2 {
        format!(
            "{}::{}",
            hex_groups(&segments[..longest.start]),
            hex_groups(&segments[longest.end..])
        )
    } else {
        hex_groups(&segments)
    };
    Ok(format!("{address}/{prefix}"))
}

fn associated_ipv6_cidr(
    associations: &[carina_aws_types::sdk::ec2::types::SubnetIpv6CidrBlockAssociation],
) -> Option<&str> {
    associations
        .iter()
        .find(|association| {
            association
                .ipv6_cidr_block_state()
                .and_then(|state| state.state())
                == Some(&carina_aws_types::sdk::ec2::types::SubnetCidrBlockStateCode::Associated)
        })
        .and_then(|association| association.ipv6_cidr_block())
}

pub(crate) fn normalize_subnet_ipv6_cidr_blocks(input: &mut DesiredView<'_>) -> ProviderResult<()> {
    let id = input.id().clone();
    if let Some(DesiredValueMut::Known(DesiredScalar::String(value))) =
        input.attribute_mut("ipv6_cidr_block")
    {
        let canonical = canonical_ipv6_cidr(value.expose_plaintext()).map_err(|error| {
            ProviderError::invalid_input(format!("invalid ipv6_cidr_block: {error}"))
                .for_resource(id)
        })?;
        *value = canonical.into();
    }
    Ok(())
}

impl AwsProvider {
    /// Read an EC2 Subnet
    pub(crate) async fn read_ec2_subnet(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use carina_aws_types::sdk::ec2::types::Filter;

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
            let fields = Self::extract_ec2_subnet_attributes(subnet);

            let tags = Self::ec2_tags_to_value(subnet.tags()).unwrap_or_else(empty_map_value);
            let prefix_value = |cidr: &str| {
                cidr.rsplit_once('/').and_then(|(_, prefix)| {
                    prefix
                        .parse::<i64>()
                        .ok()
                        .map(|prefix| Value::Concrete(ConcreteValue::Int(prefix)))
                })
            };
            let associated_ipv6_cidr =
                associated_ipv6_cidr(subnet.ipv6_cidr_block_association_set());
            let ipv6_cidr_block = associated_ipv6_cidr.map(|cidr| {
                let cidr = canonical_ipv6_cidr(cidr).unwrap_or_else(|_| cidr.to_string());
                Value::Concrete(ConcreteValue::String(cidr.into()))
            });
            let ipv6_netmask_length = associated_ipv6_cidr.and_then(prefix_value);
            let (_, model) = fields.into_model(
                id,
                Ec2SubnetReadModelSupplement {
                    ipv4_netmask_length: subnet.cidr_block().and_then(prefix_value),
                    ipv6_cidr_block,
                    ipv6_netmask_length,
                    tags,
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Subnet
    pub(crate) async fn create_ec2_subnet<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
        schema: &CompiledResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2SubnetCreateInput::decode(resource, schema) {
            Ok(input) => input,
            Err(error) => return Err(attempt.not_created(error)),
        };
        let sent = input.sent();
        let crate::send_models_generated::Ec2SubnetCreateParts {
            assign_ipv6_address_on_creation,
            availability_zone,
            availability_zone_id,
            cidr_block,
            enable_dns64,
            enable_lni_at_device_index,
            ipv4_ipam_pool_id,
            ipv4_netmask_length,
            ipv6_cidr_block,
            ipv6_ipam_pool_id,
            ipv6_native,
            ipv6_netmask_length,
            map_public_ip_on_launch,
            outpost_arn,
            private_dns_name_options_on_launch,
            tags,
            vpc_id,
        } = input.into_parts();
        let assign_ipv6_address_on_creation = assign_ipv6_address_on_creation.into_inner();
        let availability_zone = availability_zone.into_inner();
        let availability_zone_id = availability_zone_id.into_inner();
        let cidr_block = cidr_block.into_inner();
        let enable_dns64 = enable_dns64.into_inner();
        let enable_lni_at_device_index = enable_lni_at_device_index.into_inner();
        let ipv4_ipam_pool_id = ipv4_ipam_pool_id.into_inner();
        let ipv4_netmask_length = ipv4_netmask_length.into_inner();
        let ipv6_cidr_block = ipv6_cidr_block.into_inner();
        let ipv6_ipam_pool_id = ipv6_ipam_pool_id.into_inner();
        let ipv6_native = ipv6_native.into_inner();
        let ipv6_netmask_length = ipv6_netmask_length.into_inner();
        let map_public_ip_on_launch = map_public_ip_on_launch.into_inner();
        let outpost_arn = outpost_arn.into_inner();
        let private_dns_name_options_on_launch = private_dns_name_options_on_launch.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let vpc_id = vpc_id.into_inner();
        if availability_zone.is_some() && availability_zone_id.is_some() {
            return Err(attempt.not_created(
                ProviderError::invalid_input(
                    "availability_zone and availability_zone_id are mutually exclusive",
                )
                .for_resource(resource.id.clone()),
            ));
        }
        if let Err(error) = resolve_modify_subnet_attrs(
            &resource.id,
            schema,
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index.map(i64::from),
            map_public_ip_on_launch,
            private_dns_name_options_on_launch.as_ref(),
        ) {
            return Err(attempt.not_created(error));
        }
        let ipv6_native_value = ipv6_native.unwrap_or(false);

        if let Err(error) = validate_subnet_cidr_source(
            &resource.id,
            "IPv4",
            cidr_block.as_deref(),
            ipv4_ipam_pool_id.as_deref(),
            ipv4_netmask_length.map(i64::from),
            !ipv6_native_value,
        ) {
            return Err(attempt.not_created(error));
        }
        if let Err(error) = validate_subnet_cidr_source(
            &resource.id,
            "IPv6",
            ipv6_cidr_block.as_deref(),
            ipv6_ipam_pool_id.as_deref(),
            ipv6_netmask_length.map(i64::from),
            ipv6_native_value,
        ) {
            return Err(attempt.not_created(error));
        }

        let mut req = self
            .ec2_client
            .create_subnet()
            .vpc_id(&vpc_id)
            .tag_specifications(build_tag_specification_with_create_token(
                tags.as_ref(),
                carina_aws_types::sdk::ec2::types::ResourceType::Subnet,
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
            req = req.ipv4_netmask_length(ipv4_netmask_length);
        }
        if let Some(ipv6_cidr_block) = ipv6_cidr_block {
            req = req.ipv6_cidr_block(ipv6_cidr_block);
        }
        if let Some(ipv6_ipam_pool_id) = ipv6_ipam_pool_id {
            req = req.ipv6_ipam_pool_id(ipv6_ipam_pool_id);
        }
        if let Some(ipv6_netmask_length) = ipv6_netmask_length {
            req = req.ipv6_netmask_length(ipv6_netmask_length);
        }
        if ipv6_native.is_some() {
            req = req.ipv6_native(ipv6_native_value);
        }
        if let Some(outpost_arn) = outpost_arn {
            req = req.outpost_arn(outpost_arn);
        }

        if let Some(az) = availability_zone {
            req = req.availability_zone(az);
        }

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            TaggedEc2Create::Subnet {
                assign_ipv6_address_on_creation,
                enable_dns64,
                enable_lni_at_device_index: enable_lni_at_device_index.map(i64::from),
                map_public_ip_on_launch,
                private_dns_name_options_on_launch,
            }
            .create_context(client_token, sent),
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn continue_ec2_subnet_create_with(
        &self,
        id: &ResourceId,
        schema: &CompiledResourceSchema,
        subnet_id: &str,
        assign_ipv6_address_on_creation: Option<bool>,
        enable_dns64: Option<bool>,
        enable_lni_at_device_index: Option<i64>,
        map_public_ip_on_launch: Option<bool>,
        private_dns_name_options_on_launch: Option<&Value>,
    ) -> ProviderResult<()> {
        let modify_attrs = resolve_modify_subnet_attrs(
            id,
            schema,
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index,
            map_public_ip_on_launch,
            private_dns_name_options_on_launch,
        )?;
        self.modify_subnet_attributes(id, subnet_id, &modify_attrs)
            .await
    }

    /// Update an EC2 Subnet
    pub(crate) async fn update_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &SnapshotState,
        to: &crate::helpers::UpdateResource,
        schema: &CompiledResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2SubnetUpdateInput::decode(to)?;
        let crate::send_models_generated::Ec2SubnetUpdateParts {
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index,
            map_public_ip_on_launch,
            private_dns_name_options_on_launch,
            tags,
        } = input.into_parts();
        let assign_ipv6_address_on_creation = assign_ipv6_address_on_creation.into_inner();
        let enable_dns64 = enable_dns64.into_inner();
        let enable_lni_at_device_index = enable_lni_at_device_index.into_inner();
        let map_public_ip_on_launch = map_public_ip_on_launch.into_inner();
        let private_dns_name_options_on_launch = private_dns_name_options_on_launch.into_inner();
        let tags = tags.into_inner();
        let ipv6_native = |removed_attribute: &str| -> ProviderResult<bool> {
            let unknown_mode = || {
                crate::helpers::removal_error(
                    &id,
                    removed_attribute,
                    "Its reset depends on ipv6_native, which is not known in state",
                )
            };
            crate::send_model::decode_change(to, "ipv6_native", |resource| {
                crate::send_model::optional_bool(resource, "ipv6_native")
            })?
            .resolve(|| Err(unknown_mode()))?
            .ok_or_else(unknown_mode)
        };
        let mut modify_attrs = ModifySubnetAttrs {
            assign_ipv6_address_on_creation: assign_ipv6_address_on_creation
                .resolve(|| Ok(Some(ipv6_native("assign_ipv6_address_on_creation")?)))?,
            enable_dns64: enable_dns64.resolve(|| Ok(Some(false)))?,
            lni: match enable_lni_at_device_index {
                AttributeChange::Set(value) | AttributeChange::Unchanged(value) => {
                    value.map(LniChange::Enable)
                }
                AttributeChange::Removed => Some(LniChange::Disable),
            },
            map_public_ip_on_launch: map_public_ip_on_launch.resolve(|| Ok(Some(false)))?,
            hostname_type: None,
            enable_resource_name_dns_a_record: None,
            enable_resource_name_dns_aaaa_record: None,
        };
        // ModifySubnetAttribute requires one call per nested option. Each
        // removed field explicitly restores its documented DNS mode.
        let parent = "private_dns_name_options_on_launch";
        let current = from.attributes.get(parent);
        let decode_dns_bool = |value: Option<Value>| -> ProviderResult<Option<bool>> {
            Ok(value.and_then(|value| {
                if let Value::Concrete(ConcreteValue::Bool(value)) = value {
                    Some(value)
                } else {
                    None
                }
            }))
        };
        use crate::send_models_generated::Ec2SubnetDnsField;
        for field in Ec2SubnetDnsField::ALL {
            let change = crate::send_model::nested_update_field(
                &id,
                parent,
                &private_dns_name_options_on_launch,
                current,
                field.name(),
            )?;
            match field {
                Ec2SubnetDnsField::HostnameType => {
                    let removed_attribute =
                        if matches!(private_dns_name_options_on_launch, AttributeChange::Removed) {
                            parent
                        } else {
                            "private_dns_name_options_on_launch.hostname_type"
                        };
                    modify_attrs.hostname_type = change
                        .try_map(|value| {
                            carina_core::resource::enum_attr_at_schema_path(
                                schema,
                                parent,
                                &["hostname_type"],
                                value.as_ref(),
                            )
                            .map(|value| value.map(|value| HostnameType::from(value.api_value())))
                            .map_err(|error| crate::helpers::enum_attr_error(&id, error))
                        })?
                        .resolve(|| {
                            // Consume historical mode only for hostname removal.
                            Ok(Some(if ipv6_native(removed_attribute)? {
                                HostnameType::ResourceName
                            } else {
                                HostnameType::IpName
                            }))
                        })?;
                }
                Ec2SubnetDnsField::EnableResourceNameDnsARecord => {
                    modify_attrs.enable_resource_name_dns_a_record = change
                        .try_map(decode_dns_bool)?
                        .resolve(|| Ok(Some(false)))?;
                }
                Ec2SubnetDnsField::EnableResourceNameDnsAaaaRecord => {
                    modify_attrs.enable_resource_name_dns_aaaa_record = change
                        .try_map(decode_dns_bool)?
                        .resolve(|| Ok(Some(false)))?;
                }
            }
        }

        self.modify_subnet_attributes(&id, identifier.as_str(), &modify_attrs)
            .await?;

        self.apply_ec2_tags(&id, identifier.as_str(), tags, Some(&from.attributes))
            .await?;

        Ok(self.read_ec2_subnet(&id, identifier).await)
    }

    /// Apply boolean subnet attributes via ModifySubnetAttribute API.
    /// Used by both create (post-creation) and update paths.
    async fn modify_subnet_attributes(
        &self,
        id: &ResourceId,
        subnet_id: &str,
        attrs: &ModifySubnetAttrs,
    ) -> ProviderResult<()> {
        if let Some(enabled) = attrs.map_public_ip_on_launch {
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

        if let Some(enabled) = attrs.assign_ipv6_address_on_creation {
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

        if let Some(enabled) = attrs.enable_dns64 {
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

        if let Some(change) = &attrs.lni {
            let req = self
                .ec2_client
                .modify_subnet_attribute()
                .subnet_id(subnet_id);
            let (req, context) = match change {
                LniChange::Enable(device_index) => (
                    req.enable_lni_at_device_index(*device_index),
                    "Failed to set enable_lni_at_device_index",
                ),
                // DisableLniAtDeviceIndex.Value=true disables local network
                // interfaces at the current position.
                LniChange::Disable => (
                    req.disable_lni_at_device_index(
                        AttributeBooleanValue::builder().value(true).build(),
                    ),
                    "Failed to disable enable_lni_at_device_index",
                ),
            };
            req.send()
                .await
                .map_err(|error| api_error_with_meta(context, error).for_resource(id.clone()))?;
        }

        // The ModifySubnetAttribute API only allows modifying one attribute at a time.
        // Each private_dns_name_options_on_launch field must be a separate API call.
        if let Some(ht) = attrs.hostname_type.as_ref() {
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

        if let Some(v) = attrs.enable_resource_name_dns_a_record {
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

        if let Some(v) = attrs.enable_resource_name_dns_aaaa_record {
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
        let schema = crate::test_resource_schema("ec2.Subnet");
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

        resolve_modify_subnet_enum_attrs(&resource, schema)
            .expect_err("invalid hostname_type must fail before any EC2 mutation");
    }

    #[test]
    fn explicit_cidrs_from_ipam_pools_are_allowed_but_netmasks_conflict() {
        let id = ResourceId::with_provider_identity("aws", "ec2.Subnet", "test", None);
        validate_subnet_cidr_source(
            &id,
            "IPv4",
            Some("10.0.1.0/24"),
            Some("ipam-pool-12345678"),
            None,
            true,
        )
        .expect("AWS accepts an explicit IPv4 CIDR from an IPAM pool");
        validate_subnet_cidr_source(
            &id,
            "IPv6",
            Some("2001:db8::/64"),
            Some("ipam-pool-12345678"),
            None,
            false,
        )
        .expect("AWS accepts an explicit IPv6 CIDR from an IPAM pool");
        assert!(
            validate_subnet_cidr_source(
                &id,
                "IPv4",
                Some("10.0.1.0/24"),
                Some("ipam-pool-12345678"),
                Some(24),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn ipv6_cidr_normalization_preserves_hex_groups_and_slot_validity() {
        let schema = crate::schemas::generated::ec2::subnet::ec2_subnet_config()
            .schema
            .validate()
            .unwrap();
        let cidr_type = schema
            .effective_node_at_path(&["ipv6_cidr_block".to_string()])
            .unwrap()
            .value_type();

        for (authored, expected) in [
            ("::ffff:102:304/64", "::ffff:102:304/64"),
            ("::102:304/64", "::102:304/64"),
            (
                "2001:0DB8:0000:0000:0000:0000:0000:0000/64",
                "2001:db8::/64",
            ),
            ("2001:0:0:1:0:0:1:1/64", "2001::1:0:0:1:1/64"),
            ("2001:db8:0:1:2:3:4:5/64", "2001:db8:0:1:2:3:4:5/64"),
            ("::/64", "::/64"),
            ("::1/64", "::1/64"),
        ] {
            let mut resource = Resource::with_provider("aws", "ec2.Subnet", "test", None);
            let authored = Value::Concrete(ConcreteValue::String(authored.into()));
            cidr_type.validate(&authored).unwrap();
            resource.set_attr("ipv6_cidr_block", authored);

            let mut desired =
                carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource).unwrap();
            normalize_subnet_ipv6_cidr_blocks(&mut desired.view()).unwrap();
            desired.finish().unwrap();

            let normalized = resource.get_attr("ipv6_cidr_block").unwrap();
            // The slot includes validate_ec2_subnet_ipv6_cidr's custom validator.
            cidr_type.validate(normalized).unwrap();
            assert_eq!(
                normalized,
                &Value::Concrete(ConcreteValue::String(expected.into()))
            );
            assert_eq!(canonical_ipv6_cidr(expected).unwrap(), expected);
        }
    }

    #[test]
    fn ipv6_cidr_normalization_matches_the_associated_read_value() {
        assert_eq!(
            canonical_ipv6_cidr("2001:0DB8:0000:0000:0000:0000:0000:0000/64").unwrap(),
            "2001:db8::/64"
        );
        let mut resource = Resource::with_provider("aws", "ec2.Subnet", "test", None);
        resource.set_attr(
            "ipv6_cidr_block",
            Value::Concrete(ConcreteValue::String(
                "2001:0DB8:0000:0000:0000:0000:0000:0000/64".into(),
            )),
        );
        let mut desired =
            carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource).unwrap();
        normalize_subnet_ipv6_cidr_blocks(&mut desired.view()).unwrap();
        desired.finish().unwrap();
        assert_eq!(
            resource.get_attr("ipv6_cidr_block"),
            Some(&Value::Concrete(ConcreteValue::String(
                "2001:db8::/64".into()
            )))
        );

        let associating = carina_aws_types::sdk::ec2::types::SubnetIpv6CidrBlockAssociation::builder()
            .ipv6_cidr_block("2001:db8:1::/64")
            .ipv6_cidr_block_state(
                carina_aws_types::sdk::ec2::types::SubnetCidrBlockState::builder()
                    .state(carina_aws_types::sdk::ec2::types::SubnetCidrBlockStateCode::Associating)
                    .build(),
            )
            .build();
        let associated =
            carina_aws_types::sdk::ec2::types::SubnetIpv6CidrBlockAssociation::builder()
                .ipv6_cidr_block("2001:db8::/64")
                .ipv6_cidr_block_state(
                    carina_aws_types::sdk::ec2::types::SubnetCidrBlockState::builder()
                        .state(
                            carina_aws_types::sdk::ec2::types::SubnetCidrBlockStateCode::Associated,
                        )
                        .build(),
                )
                .build();
        assert_eq!(
            associated_ipv6_cidr(&[associating, associated]),
            Some("2001:db8::/64")
        );
    }
}
