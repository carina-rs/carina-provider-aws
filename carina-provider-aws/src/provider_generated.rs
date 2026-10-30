//! Auto-generated provider boilerplate
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-provider.sh

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use carina_core::provider::{
    AppliedIdentifier, BoxFuture, ProviderError, ProviderResult, ReadOutcome,
};
use carina_core::resource::{ConcreteValue, DataSource, Resource, ResourceId, State, Value};
use indexmap::IndexMap;

// ===== Generated Methods on AwsProvider =====

/// Typed values extracted from the AWS read shape for `ec2.Vpc`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcReadFields {
    cidr_block: Option<Value>,
    instance_tenancy: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2VpcReadFields {
    pub(crate) fn cidr_block(&self) -> Option<&Value> {
        self.cidr_block.as_ref()
    }

    pub(crate) fn map_cidr_block(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.cidr_block = map(self.cidr_block);
        self
    }

    pub(crate) fn instance_tenancy(&self) -> Option<&Value> {
        self.instance_tenancy.as_ref()
    }

    pub(crate) fn map_instance_tenancy(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.instance_tenancy = map(self.instance_tenancy);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2VpcReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2VpcReadModel,
    )> {
        let Self {
            cidr_block,
            instance_tenancy,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2VpcReadModelSupplement {
            enable_dns_hostnames,
            enable_dns_support,
            ipv4_netmask_length,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2VpcReadModel {
            cidr_block: cidr_block.clone(),
            enable_dns_hostnames,
            enable_dns_support,
            instance_tenancy: instance_tenancy.clone(),
            ipv4_netmask_length,
            vpc_id: vpc_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.Subnet`.
#[allow(dead_code)]
pub(crate) struct Ec2SubnetReadFields {
    assign_ipv6_address_on_creation: Option<Value>,
    availability_zone: Option<Value>,
    availability_zone_id: Option<Value>,
    cidr_block: Option<Value>,
    enable_dns64: Option<Value>,
    enable_lni_at_device_index: Option<Value>,
    ipv6_native: Option<Value>,
    map_public_ip_on_launch: Option<Value>,
    outpost_arn: Option<Value>,
    private_dns_name_options_on_launch: Option<Value>,
    subnet_id: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2SubnetReadFields {
    pub(crate) fn assign_ipv6_address_on_creation(&self) -> Option<&Value> {
        self.assign_ipv6_address_on_creation.as_ref()
    }

    pub(crate) fn map_assign_ipv6_address_on_creation(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.assign_ipv6_address_on_creation = map(self.assign_ipv6_address_on_creation);
        self
    }

    pub(crate) fn availability_zone(&self) -> Option<&Value> {
        self.availability_zone.as_ref()
    }

    pub(crate) fn map_availability_zone(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.availability_zone = map(self.availability_zone);
        self
    }

    pub(crate) fn availability_zone_id(&self) -> Option<&Value> {
        self.availability_zone_id.as_ref()
    }

    pub(crate) fn map_availability_zone_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.availability_zone_id = map(self.availability_zone_id);
        self
    }

    pub(crate) fn cidr_block(&self) -> Option<&Value> {
        self.cidr_block.as_ref()
    }

    pub(crate) fn map_cidr_block(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.cidr_block = map(self.cidr_block);
        self
    }

    pub(crate) fn enable_dns64(&self) -> Option<&Value> {
        self.enable_dns64.as_ref()
    }

    pub(crate) fn map_enable_dns64(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.enable_dns64 = map(self.enable_dns64);
        self
    }

    pub(crate) fn enable_lni_at_device_index(&self) -> Option<&Value> {
        self.enable_lni_at_device_index.as_ref()
    }

    pub(crate) fn map_enable_lni_at_device_index(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.enable_lni_at_device_index = map(self.enable_lni_at_device_index);
        self
    }

    pub(crate) fn ipv6_native(&self) -> Option<&Value> {
        self.ipv6_native.as_ref()
    }

    pub(crate) fn map_ipv6_native(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.ipv6_native = map(self.ipv6_native);
        self
    }

    pub(crate) fn map_public_ip_on_launch(&self) -> Option<&Value> {
        self.map_public_ip_on_launch.as_ref()
    }

    pub(crate) fn map_map_public_ip_on_launch(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.map_public_ip_on_launch = map(self.map_public_ip_on_launch);
        self
    }

    pub(crate) fn outpost_arn(&self) -> Option<&Value> {
        self.outpost_arn.as_ref()
    }

    pub(crate) fn map_outpost_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.outpost_arn = map(self.outpost_arn);
        self
    }

    pub(crate) fn private_dns_name_options_on_launch(&self) -> Option<&Value> {
        self.private_dns_name_options_on_launch.as_ref()
    }

    pub(crate) fn map_private_dns_name_options_on_launch(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.private_dns_name_options_on_launch = map(self.private_dns_name_options_on_launch);
        self
    }

    pub(crate) fn subnet_id(&self) -> Option<&Value> {
        self.subnet_id.as_ref()
    }

    pub(crate) fn map_subnet_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subnet_id = map(self.subnet_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2SubnetReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2SubnetReadModel,
    )> {
        let Self {
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
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2SubnetReadModelSupplement {
            ipv4_netmask_length,
            ipv6_cidr_block,
            ipv6_netmask_length,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2SubnetReadModel {
            assign_ipv6_address_on_creation: assign_ipv6_address_on_creation.clone(),
            availability_zone: availability_zone.clone(),
            availability_zone_id: availability_zone_id.clone(),
            cidr_block: cidr_block.clone(),
            enable_dns64: enable_dns64.clone(),
            enable_lni_at_device_index: enable_lni_at_device_index.clone(),
            ipv4_netmask_length,
            ipv6_cidr_block,
            ipv6_native: ipv6_native.clone(),
            ipv6_netmask_length,
            map_public_ip_on_launch: map_public_ip_on_launch.clone(),
            outpost_arn: outpost_arn.clone(),
            private_dns_name_options_on_launch: private_dns_name_options_on_launch.clone(),
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            subnet_id: subnet_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.InternetGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2InternetGatewayReadFields {
    internet_gateway_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2InternetGatewayReadFields {
    pub(crate) fn internet_gateway_id(&self) -> Option<&Value> {
        self.internet_gateway_id.as_ref()
    }

    pub(crate) fn map_internet_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.internet_gateway_id = map(self.internet_gateway_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2InternetGatewayReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2InternetGatewayReadModel,
    )> {
        let Self {
            internet_gateway_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2InternetGatewayReadModelSupplement { vpc_id, tags } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2InternetGatewayReadModel {
            vpc_id,
            internet_gateway_id: internet_gateway_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.RouteTable`.
#[allow(dead_code)]
pub(crate) struct Ec2RouteTableReadFields {
    route_table_id: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2RouteTableReadFields {
    pub(crate) fn route_table_id(&self) -> Option<&Value> {
        self.route_table_id.as_ref()
    }

    pub(crate) fn map_route_table_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.route_table_id = map(self.route_table_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2RouteTableReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2RouteTableReadModel,
    )> {
        let Self {
            route_table_id,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2RouteTableReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2RouteTableReadModel {
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            route_table_id: route_table_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.Route`.
#[allow(dead_code)]
pub(crate) struct Ec2RouteReadFields {
    destination_cidr_block: Option<Value>,
    gateway_id: Option<Value>,
    nat_gateway_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2RouteReadFields {
    pub(crate) fn destination_cidr_block(&self) -> Option<&Value> {
        self.destination_cidr_block.as_ref()
    }

    pub(crate) fn map_destination_cidr_block(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.destination_cidr_block = map(self.destination_cidr_block);
        self
    }

    pub(crate) fn gateway_id(&self) -> Option<&Value> {
        self.gateway_id.as_ref()
    }

    pub(crate) fn map_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.gateway_id = map(self.gateway_id);
        self
    }

    pub(crate) fn nat_gateway_id(&self) -> Option<&Value> {
        self.nat_gateway_id.as_ref()
    }

    pub(crate) fn map_nat_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.nat_gateway_id = map(self.nat_gateway_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2RouteReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2RouteReadModel,
    )> {
        let Self {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2RouteReadModelSupplement { route_table_id } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2RouteReadModel {
            destination_cidr_block: destination_cidr_block.clone(),
            gateway_id: gateway_id.clone(),
            nat_gateway_id: nat_gateway_id.clone(),
            route_table_id,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroup`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupReadFields {
    description: Option<Value>,
    group_id: Option<Value>,
    group_name: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2SecurityGroupReadFields {
    pub(crate) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }

    pub(crate) fn map_description(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.description = map(self.description);
        self
    }

    pub(crate) fn group_id(&self) -> Option<&Value> {
        self.group_id.as_ref()
    }

    pub(crate) fn map_group_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.group_id = map(self.group_id);
        self
    }

    pub(crate) fn group_name(&self) -> Option<&Value> {
        self.group_name.as_ref()
    }

    pub(crate) fn map_group_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.group_name = map(self.group_name);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2SecurityGroupReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2SecurityGroupReadModel,
    )> {
        let Self {
            description,
            group_id,
            group_name,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2SecurityGroupReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2SecurityGroupReadModel {
            description: crate::read_model::require_read_value(
                resource_id,
                description.clone(),
                "description",
            )?,
            group_name: crate::read_model::require_read_value(
                resource_id,
                group_name.clone(),
                "group_name",
            )?,
            vpc_id: vpc_id.clone(),
            group_id: group_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroupIngress`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupIngressReadFields {
    cidr_ipv6: Option<Value>,
    description: Option<Value>,
    from_port: Option<Value>,
    group_id: Option<Value>,
    ip_protocol: Option<Value>,
    security_group_rule_id: Option<Value>,
    source_prefix_list_id: Option<Value>,
    to_port: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2SecurityGroupIngressReadFields {
    pub(crate) fn cidr_ipv6(&self) -> Option<&Value> {
        self.cidr_ipv6.as_ref()
    }

    pub(crate) fn map_cidr_ipv6(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.cidr_ipv6 = map(self.cidr_ipv6);
        self
    }

    pub(crate) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }

    pub(crate) fn map_description(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.description = map(self.description);
        self
    }

    pub(crate) fn from_port(&self) -> Option<&Value> {
        self.from_port.as_ref()
    }

    pub(crate) fn map_from_port(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.from_port = map(self.from_port);
        self
    }

    pub(crate) fn group_id(&self) -> Option<&Value> {
        self.group_id.as_ref()
    }

    pub(crate) fn map_group_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.group_id = map(self.group_id);
        self
    }

    pub(crate) fn ip_protocol(&self) -> Option<&Value> {
        self.ip_protocol.as_ref()
    }

    pub(crate) fn map_ip_protocol(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.ip_protocol = map(self.ip_protocol);
        self
    }

    pub(crate) fn security_group_rule_id(&self) -> Option<&Value> {
        self.security_group_rule_id.as_ref()
    }

    pub(crate) fn map_security_group_rule_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.security_group_rule_id = map(self.security_group_rule_id);
        self
    }

    pub(crate) fn source_prefix_list_id(&self) -> Option<&Value> {
        self.source_prefix_list_id.as_ref()
    }

    pub(crate) fn map_source_prefix_list_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.source_prefix_list_id = map(self.source_prefix_list_id);
        self
    }

    pub(crate) fn to_port(&self) -> Option<&Value> {
        self.to_port.as_ref()
    }

    pub(crate) fn map_to_port(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.to_port = map(self.to_port);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2SecurityGroupIngressReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2SecurityGroupIngressReadModel,
    )> {
        let Self {
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            source_prefix_list_id,
            to_port,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2SecurityGroupIngressReadModelSupplement {
            cidr_ip,
            source_security_group_owner_id,
            source_security_group_id,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2SecurityGroupIngressReadModel {
            cidr_ip,
            cidr_ipv6: cidr_ipv6.clone(),
            description: description.clone(),
            from_port: from_port.clone(),
            group_id: group_id.clone(),
            ip_protocol: crate::read_model::require_read_value(
                resource_id,
                ip_protocol.clone(),
                "ip_protocol",
            )?,
            source_prefix_list_id: source_prefix_list_id.clone(),
            source_security_group_owner_id,
            to_port: to_port.clone(),
            source_security_group_id,
            security_group_rule_id: security_group_rule_id.clone(),
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroupEgress`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupEgressReadFields {
    cidr_ipv6: Option<Value>,
    description: Option<Value>,
    destination_prefix_list_id: Option<Value>,
    from_port: Option<Value>,
    group_id: Option<Value>,
    ip_protocol: Option<Value>,
    security_group_rule_id: Option<Value>,
    to_port: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2SecurityGroupEgressReadFields {
    pub(crate) fn cidr_ipv6(&self) -> Option<&Value> {
        self.cidr_ipv6.as_ref()
    }

    pub(crate) fn map_cidr_ipv6(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.cidr_ipv6 = map(self.cidr_ipv6);
        self
    }

    pub(crate) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }

    pub(crate) fn map_description(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.description = map(self.description);
        self
    }

    pub(crate) fn destination_prefix_list_id(&self) -> Option<&Value> {
        self.destination_prefix_list_id.as_ref()
    }

    pub(crate) fn map_destination_prefix_list_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.destination_prefix_list_id = map(self.destination_prefix_list_id);
        self
    }

    pub(crate) fn from_port(&self) -> Option<&Value> {
        self.from_port.as_ref()
    }

    pub(crate) fn map_from_port(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.from_port = map(self.from_port);
        self
    }

    pub(crate) fn group_id(&self) -> Option<&Value> {
        self.group_id.as_ref()
    }

    pub(crate) fn map_group_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.group_id = map(self.group_id);
        self
    }

    pub(crate) fn ip_protocol(&self) -> Option<&Value> {
        self.ip_protocol.as_ref()
    }

    pub(crate) fn map_ip_protocol(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.ip_protocol = map(self.ip_protocol);
        self
    }

    pub(crate) fn security_group_rule_id(&self) -> Option<&Value> {
        self.security_group_rule_id.as_ref()
    }

    pub(crate) fn map_security_group_rule_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.security_group_rule_id = map(self.security_group_rule_id);
        self
    }

    pub(crate) fn to_port(&self) -> Option<&Value> {
        self.to_port.as_ref()
    }

    pub(crate) fn map_to_port(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.to_port = map(self.to_port);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2SecurityGroupEgressReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2SecurityGroupEgressReadModel,
    )> {
        let Self {
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            to_port,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2SecurityGroupEgressReadModelSupplement {
            cidr_ip,
            source_security_group_owner_id,
            destination_security_group_id,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2SecurityGroupEgressReadModel {
            cidr_ip,
            cidr_ipv6: cidr_ipv6.clone(),
            description: description.clone(),
            destination_prefix_list_id: destination_prefix_list_id.clone(),
            from_port: from_port.clone(),
            group_id: crate::read_model::require_read_value(
                resource_id,
                group_id.clone(),
                "group_id",
            )?,
            ip_protocol: crate::read_model::require_read_value(
                resource_id,
                ip_protocol.clone(),
                "ip_protocol",
            )?,
            source_security_group_owner_id,
            to_port: to_port.clone(),
            destination_security_group_id,
            security_group_rule_id: security_group_rule_id.clone(),
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.EgressOnlyInternetGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2EgressOnlyInternetGatewayReadFields {
    egress_only_internet_gateway_id: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2EgressOnlyInternetGatewayReadFields {
    pub(crate) fn egress_only_internet_gateway_id(&self) -> Option<&Value> {
        self.egress_only_internet_gateway_id.as_ref()
    }

    pub(crate) fn map_egress_only_internet_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.egress_only_internet_gateway_id = map(self.egress_only_internet_gateway_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2EgressOnlyInternetGatewayReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2EgressOnlyInternetGatewayReadModel,
    )> {
        let Self {
            egress_only_internet_gateway_id,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2EgressOnlyInternetGatewayReadModelSupplement { tags } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2EgressOnlyInternetGatewayReadModel {
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            egress_only_internet_gateway_id: egress_only_internet_gateway_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.Eip`.
#[allow(dead_code)]
pub(crate) struct Ec2EipReadFields {
    allocation_id: Option<Value>,
    domain: Option<Value>,
    public_ip: Option<Value>,
    public_ipv4_pool: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2EipReadFields {
    pub(crate) fn allocation_id(&self) -> Option<&Value> {
        self.allocation_id.as_ref()
    }

    pub(crate) fn map_allocation_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.allocation_id = map(self.allocation_id);
        self
    }

    pub(crate) fn domain(&self) -> Option<&Value> {
        self.domain.as_ref()
    }

    pub(crate) fn map_domain(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.domain = map(self.domain);
        self
    }

    pub(crate) fn public_ip(&self) -> Option<&Value> {
        self.public_ip.as_ref()
    }

    pub(crate) fn map_public_ip(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.public_ip = map(self.public_ip);
        self
    }

    pub(crate) fn public_ipv4_pool(&self) -> Option<&Value> {
        self.public_ipv4_pool.as_ref()
    }

    pub(crate) fn map_public_ipv4_pool(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.public_ipv4_pool = map(self.public_ipv4_pool);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2EipReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2EipReadModel,
    )> {
        let Self {
            allocation_id,
            domain,
            public_ip,
            public_ipv4_pool,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2EipReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2EipReadModel {
            address: public_ip.clone(),
            domain: domain.clone(),
            public_ipv4_pool: public_ipv4_pool.clone(),
            allocation_id: allocation_id.clone(),
            public_ip: public_ip.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.FlowLog`.
#[allow(dead_code)]
pub(crate) struct Ec2FlowLogReadFields {
    deliver_logs_permission_arn: Option<Value>,
    flow_log_id: Option<Value>,
    log_destination: Option<Value>,
    log_destination_type: Option<Value>,
    log_format: Option<Value>,
    log_group_name: Option<Value>,
    max_aggregation_interval: Option<Value>,
    traffic_type: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2FlowLogReadFields {
    pub(crate) fn deliver_logs_permission_arn(&self) -> Option<&Value> {
        self.deliver_logs_permission_arn.as_ref()
    }

    pub(crate) fn map_deliver_logs_permission_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.deliver_logs_permission_arn = map(self.deliver_logs_permission_arn);
        self
    }

    pub(crate) fn flow_log_id(&self) -> Option<&Value> {
        self.flow_log_id.as_ref()
    }

    pub(crate) fn map_flow_log_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.flow_log_id = map(self.flow_log_id);
        self
    }

    pub(crate) fn log_destination(&self) -> Option<&Value> {
        self.log_destination.as_ref()
    }

    pub(crate) fn map_log_destination(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_destination = map(self.log_destination);
        self
    }

    pub(crate) fn log_destination_type(&self) -> Option<&Value> {
        self.log_destination_type.as_ref()
    }

    pub(crate) fn map_log_destination_type(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_destination_type = map(self.log_destination_type);
        self
    }

    pub(crate) fn log_format(&self) -> Option<&Value> {
        self.log_format.as_ref()
    }

    pub(crate) fn map_log_format(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_format = map(self.log_format);
        self
    }

    pub(crate) fn log_group_name(&self) -> Option<&Value> {
        self.log_group_name.as_ref()
    }

    pub(crate) fn map_log_group_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_group_name = map(self.log_group_name);
        self
    }

    pub(crate) fn max_aggregation_interval(&self) -> Option<&Value> {
        self.max_aggregation_interval.as_ref()
    }

    pub(crate) fn map_max_aggregation_interval(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.max_aggregation_interval = map(self.max_aggregation_interval);
        self
    }

    pub(crate) fn traffic_type(&self) -> Option<&Value> {
        self.traffic_type.as_ref()
    }

    pub(crate) fn map_traffic_type(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.traffic_type = map(self.traffic_type);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2FlowLogReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2FlowLogReadModel,
    )> {
        let Self {
            deliver_logs_permission_arn,
            flow_log_id,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            traffic_type,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2FlowLogReadModelSupplement {
            resource_ids,
            resource_type,
            tag_field_specifications,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2FlowLogReadModel {
            deliver_logs_permission_arn: deliver_logs_permission_arn.clone(),
            log_destination: log_destination.clone(),
            log_destination_type: log_destination_type.clone(),
            log_format: log_format.clone(),
            log_group_name: log_group_name.clone(),
            max_aggregation_interval: max_aggregation_interval.clone(),
            resource_ids,
            resource_type,
            tag_field_specifications,
            traffic_type: traffic_type.clone(),
            flow_log_id: flow_log_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.NatGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2NatGatewayReadFields {
    allocation_id: Option<Value>,
    availability_mode: Option<Value>,
    connectivity_type: Option<Value>,
    nat_gateway_id: Option<Value>,
    subnet_id: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2NatGatewayReadFields {
    pub(crate) fn allocation_id(&self) -> Option<&Value> {
        self.allocation_id.as_ref()
    }

    pub(crate) fn map_allocation_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.allocation_id = map(self.allocation_id);
        self
    }

    pub(crate) fn availability_mode(&self) -> Option<&Value> {
        self.availability_mode.as_ref()
    }

    pub(crate) fn map_availability_mode(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.availability_mode = map(self.availability_mode);
        self
    }

    pub(crate) fn connectivity_type(&self) -> Option<&Value> {
        self.connectivity_type.as_ref()
    }

    pub(crate) fn map_connectivity_type(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.connectivity_type = map(self.connectivity_type);
        self
    }

    pub(crate) fn nat_gateway_id(&self) -> Option<&Value> {
        self.nat_gateway_id.as_ref()
    }

    pub(crate) fn map_nat_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.nat_gateway_id = map(self.nat_gateway_id);
        self
    }

    pub(crate) fn subnet_id(&self) -> Option<&Value> {
        self.subnet_id.as_ref()
    }

    pub(crate) fn map_subnet_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subnet_id = map(self.subnet_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2NatGatewayReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2NatGatewayReadModel,
    )> {
        let Self {
            allocation_id,
            availability_mode,
            connectivity_type,
            nat_gateway_id,
            subnet_id,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2NatGatewayReadModelSupplement {
            availability_zone_addresses,
            private_ip_address,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2NatGatewayReadModel {
            allocation_id: allocation_id.clone(),
            availability_mode: availability_mode.clone(),
            availability_zone_addresses,
            connectivity_type: connectivity_type.clone(),
            private_ip_address,
            subnet_id: subnet_id.clone(),
            vpc_id: vpc_id.clone(),
            nat_gateway_id: nat_gateway_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.SubnetRouteTableAssociation`.
#[allow(dead_code)]
pub(crate) struct Ec2SubnetRouteTableAssociationReadFields {
    route_table_id: Option<Value>,
    subnet_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2SubnetRouteTableAssociationReadFields {
    pub(crate) fn route_table_id(&self) -> Option<&Value> {
        self.route_table_id.as_ref()
    }

    pub(crate) fn map_route_table_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.route_table_id = map(self.route_table_id);
        self
    }

    pub(crate) fn subnet_id(&self) -> Option<&Value> {
        self.subnet_id.as_ref()
    }

    pub(crate) fn map_subnet_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subnet_id = map(self.subnet_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2SubnetRouteTableAssociationReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2SubnetRouteTableAssociationReadModel,
    )> {
        let Self {
            route_table_id,
            subnet_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2SubnetRouteTableAssociationReadModelSupplement {} =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2SubnetRouteTableAssociationReadModel {
            route_table_id: crate::read_model::require_read_value(
                resource_id,
                route_table_id.clone(),
                "route_table_id",
            )?,
            subnet_id: crate::read_model::require_read_value(
                resource_id,
                subnet_id.clone(),
                "subnet_id",
            )?,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.TransitGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2TransitGatewayReadFields {
    description: Option<Value>,
    options: Option<Value>,
    transit_gateway_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2TransitGatewayReadFields {
    pub(crate) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }

    pub(crate) fn map_description(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.description = map(self.description);
        self
    }

    pub(crate) fn options(&self) -> Option<&Value> {
        self.options.as_ref()
    }

    pub(crate) fn map_options(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.options = map(self.options);
        self
    }

    pub(crate) fn transit_gateway_id(&self) -> Option<&Value> {
        self.transit_gateway_id.as_ref()
    }

    pub(crate) fn map_transit_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.transit_gateway_id = map(self.transit_gateway_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2TransitGatewayReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2TransitGatewayReadModel,
    )> {
        let Self {
            description,
            options,
            transit_gateway_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2TransitGatewayReadModelSupplement { tags } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2TransitGatewayReadModel {
            description: description.clone(),
            options: options.clone(),
            transit_gateway_id: transit_gateway_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.TransitGatewayAttachment`.
#[allow(dead_code)]
pub(crate) struct Ec2TransitGatewayAttachmentReadFields {
    options: Option<Value>,
    subnet_ids: Option<Value>,
    transit_gateway_attachment_id: Option<Value>,
    transit_gateway_id: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2TransitGatewayAttachmentReadFields {
    pub(crate) fn options(&self) -> Option<&Value> {
        self.options.as_ref()
    }

    pub(crate) fn map_options(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.options = map(self.options);
        self
    }

    pub(crate) fn subnet_ids(&self) -> Option<&Value> {
        self.subnet_ids.as_ref()
    }

    pub(crate) fn map_subnet_ids(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subnet_ids = map(self.subnet_ids);
        self
    }

    pub(crate) fn transit_gateway_attachment_id(&self) -> Option<&Value> {
        self.transit_gateway_attachment_id.as_ref()
    }

    pub(crate) fn map_transit_gateway_attachment_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.transit_gateway_attachment_id = map(self.transit_gateway_attachment_id);
        self
    }

    pub(crate) fn transit_gateway_id(&self) -> Option<&Value> {
        self.transit_gateway_id.as_ref()
    }

    pub(crate) fn map_transit_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.transit_gateway_id = map(self.transit_gateway_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2TransitGatewayAttachmentReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2TransitGatewayAttachmentReadModel,
    )> {
        let Self {
            options,
            subnet_ids,
            transit_gateway_attachment_id,
            transit_gateway_id,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2TransitGatewayAttachmentReadModelSupplement { tags } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2TransitGatewayAttachmentReadModel {
            options: options.clone(),
            subnet_ids: crate::read_model::require_read_value(
                resource_id,
                subnet_ids.clone(),
                "subnet_ids",
            )?,
            transit_gateway_id: crate::read_model::require_read_value(
                resource_id,
                transit_gateway_id.clone(),
                "transit_gateway_id",
            )?,
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            transit_gateway_attachment_id: transit_gateway_attachment_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.VpcEndpoint`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcEndpointReadFields {
    policy_document: Option<Value>,
    private_dns_enabled: Option<Value>,
    resource_configuration_arn: Option<Value>,
    route_table_ids: Option<Value>,
    security_group_ids: Option<Value>,
    service_name: Option<Value>,
    service_network_arn: Option<Value>,
    service_region: Option<Value>,
    subnet_ids: Option<Value>,
    vpc_endpoint_id: Option<Value>,
    vpc_endpoint_type: Option<Value>,
    vpc_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2VpcEndpointReadFields {
    pub(crate) fn policy_document(&self) -> Option<&Value> {
        self.policy_document.as_ref()
    }

    pub(crate) fn map_policy_document(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.policy_document = map(self.policy_document);
        self
    }

    pub(crate) fn private_dns_enabled(&self) -> Option<&Value> {
        self.private_dns_enabled.as_ref()
    }

    pub(crate) fn map_private_dns_enabled(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.private_dns_enabled = map(self.private_dns_enabled);
        self
    }

    pub(crate) fn resource_configuration_arn(&self) -> Option<&Value> {
        self.resource_configuration_arn.as_ref()
    }

    pub(crate) fn map_resource_configuration_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.resource_configuration_arn = map(self.resource_configuration_arn);
        self
    }

    pub(crate) fn route_table_ids(&self) -> Option<&Value> {
        self.route_table_ids.as_ref()
    }

    pub(crate) fn map_route_table_ids(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.route_table_ids = map(self.route_table_ids);
        self
    }

    pub(crate) fn security_group_ids(&self) -> Option<&Value> {
        self.security_group_ids.as_ref()
    }

    pub(crate) fn map_security_group_ids(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.security_group_ids = map(self.security_group_ids);
        self
    }

    pub(crate) fn service_name(&self) -> Option<&Value> {
        self.service_name.as_ref()
    }

    pub(crate) fn map_service_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.service_name = map(self.service_name);
        self
    }

    pub(crate) fn service_network_arn(&self) -> Option<&Value> {
        self.service_network_arn.as_ref()
    }

    pub(crate) fn map_service_network_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.service_network_arn = map(self.service_network_arn);
        self
    }

    pub(crate) fn service_region(&self) -> Option<&Value> {
        self.service_region.as_ref()
    }

    pub(crate) fn map_service_region(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.service_region = map(self.service_region);
        self
    }

    pub(crate) fn subnet_ids(&self) -> Option<&Value> {
        self.subnet_ids.as_ref()
    }

    pub(crate) fn map_subnet_ids(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subnet_ids = map(self.subnet_ids);
        self
    }

    pub(crate) fn vpc_endpoint_id(&self) -> Option<&Value> {
        self.vpc_endpoint_id.as_ref()
    }

    pub(crate) fn map_vpc_endpoint_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.vpc_endpoint_id = map(self.vpc_endpoint_id);
        self
    }

    pub(crate) fn vpc_endpoint_type(&self) -> Option<&Value> {
        self.vpc_endpoint_type.as_ref()
    }

    pub(crate) fn map_vpc_endpoint_type(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.vpc_endpoint_type = map(self.vpc_endpoint_type);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2VpcEndpointReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2VpcEndpointReadModel,
    )> {
        let Self {
            policy_document,
            private_dns_enabled,
            resource_configuration_arn,
            route_table_ids,
            security_group_ids,
            service_name,
            service_network_arn,
            service_region,
            subnet_ids,
            vpc_endpoint_id,
            vpc_endpoint_type,
            vpc_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2VpcEndpointReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2VpcEndpointReadModel {
            policy_document: policy_document.clone(),
            private_dns_enabled: private_dns_enabled.clone(),
            resource_configuration_arn: resource_configuration_arn.clone(),
            route_table_ids: crate::read_model::require_read_value(
                resource_id,
                route_table_ids.clone(),
                "route_table_ids",
            )?,
            security_group_ids: crate::read_model::require_read_value(
                resource_id,
                security_group_ids.clone(),
                "security_group_ids",
            )?,
            service_name: crate::read_model::require_read_value(
                resource_id,
                service_name.clone(),
                "service_name",
            )?,
            service_network_arn: service_network_arn.clone(),
            service_region: service_region.clone(),
            subnet_ids: crate::read_model::require_read_value(
                resource_id,
                subnet_ids.clone(),
                "subnet_ids",
            )?,
            vpc_endpoint_type: vpc_endpoint_type.clone(),
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            vpc_endpoint_id: vpc_endpoint_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.VpcPeeringConnection`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcPeeringConnectionReadFields {
    peer_owner_id: Option<Value>,
    peer_vpc_id: Option<Value>,
    vpc_id: Option<Value>,
    vpc_peering_connection_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2VpcPeeringConnectionReadFields {
    pub(crate) fn peer_owner_id(&self) -> Option<&Value> {
        self.peer_owner_id.as_ref()
    }

    pub(crate) fn map_peer_owner_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.peer_owner_id = map(self.peer_owner_id);
        self
    }

    pub(crate) fn peer_vpc_id(&self) -> Option<&Value> {
        self.peer_vpc_id.as_ref()
    }

    pub(crate) fn map_peer_vpc_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.peer_vpc_id = map(self.peer_vpc_id);
        self
    }

    pub(crate) fn vpc_id(&self) -> Option<&Value> {
        self.vpc_id.as_ref()
    }

    pub(crate) fn map_vpc_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.vpc_id = map(self.vpc_id);
        self
    }

    pub(crate) fn vpc_peering_connection_id(&self) -> Option<&Value> {
        self.vpc_peering_connection_id.as_ref()
    }

    pub(crate) fn map_vpc_peering_connection_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.vpc_peering_connection_id = map(self.vpc_peering_connection_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2VpcPeeringConnectionReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2VpcPeeringConnectionReadModel,
    )> {
        let Self {
            peer_owner_id,
            peer_vpc_id,
            vpc_id,
            vpc_peering_connection_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2VpcPeeringConnectionReadModelSupplement { tags } =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2VpcPeeringConnectionReadModel {
            peer_owner_id: peer_owner_id.clone(),
            peer_vpc_id: crate::read_model::require_read_value(
                resource_id,
                peer_vpc_id.clone(),
                "peer_vpc_id",
            )?,
            vpc_id: crate::read_model::require_read_value(resource_id, vpc_id.clone(), "vpc_id")?,
            vpc_peering_connection_id: vpc_peering_connection_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `ec2.VpnGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2VpnGatewayReadFields {
    amazon_side_asn: Option<Value>,
    availability_zone: Option<Value>,
    r#type: Option<Value>,
    vpn_gateway_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Ec2VpnGatewayReadFields {
    pub(crate) fn amazon_side_asn(&self) -> Option<&Value> {
        self.amazon_side_asn.as_ref()
    }

    pub(crate) fn map_amazon_side_asn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.amazon_side_asn = map(self.amazon_side_asn);
        self
    }

    pub(crate) fn availability_zone(&self) -> Option<&Value> {
        self.availability_zone.as_ref()
    }

    pub(crate) fn map_availability_zone(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.availability_zone = map(self.availability_zone);
        self
    }

    pub(crate) fn r#type(&self) -> Option<&Value> {
        self.r#type.as_ref()
    }

    pub(crate) fn map_type(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.r#type = map(self.r#type);
        self
    }

    pub(crate) fn vpn_gateway_id(&self) -> Option<&Value> {
        self.vpn_gateway_id.as_ref()
    }

    pub(crate) fn map_vpn_gateway_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.vpn_gateway_id = map(self.vpn_gateway_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Ec2VpnGatewayReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Ec2VpnGatewayReadModel,
    )> {
        let Self {
            amazon_side_asn,
            availability_zone,
            r#type,
            vpn_gateway_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Ec2VpnGatewayReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Ec2VpnGatewayReadModel {
            amazon_side_asn: amazon_side_asn.clone(),
            availability_zone: availability_zone.clone(),
            r#type: crate::read_model::require_read_value(resource_id, r#type.clone(), "type")?,
            vpn_gateway_id: vpn_gateway_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `organizations.Organization`.
#[allow(dead_code)]
pub(crate) struct OrganizationsOrganizationReadFields {
    arn: Option<Value>,
    feature_set: Option<Value>,
    id: Option<Value>,
    master_account_arn: Option<Value>,
    master_account_email: Option<Value>,
    master_account_id: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl OrganizationsOrganizationReadFields {
    pub(crate) fn arn(&self) -> Option<&Value> {
        self.arn.as_ref()
    }

    pub(crate) fn map_arn(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.arn = map(self.arn);
        self
    }

    pub(crate) fn feature_set(&self) -> Option<&Value> {
        self.feature_set.as_ref()
    }

    pub(crate) fn map_feature_set(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.feature_set = map(self.feature_set);
        self
    }

    pub(crate) fn id(&self) -> Option<&Value> {
        self.id.as_ref()
    }

    pub(crate) fn map_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.id = map(self.id);
        self
    }

    pub(crate) fn master_account_arn(&self) -> Option<&Value> {
        self.master_account_arn.as_ref()
    }

    pub(crate) fn map_master_account_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.master_account_arn = map(self.master_account_arn);
        self
    }

    pub(crate) fn master_account_email(&self) -> Option<&Value> {
        self.master_account_email.as_ref()
    }

    pub(crate) fn map_master_account_email(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.master_account_email = map(self.master_account_email);
        self
    }

    pub(crate) fn master_account_id(&self) -> Option<&Value> {
        self.master_account_id.as_ref()
    }

    pub(crate) fn map_master_account_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.master_account_id = map(self.master_account_id);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::OrganizationsOrganizationReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::OrganizationsOrganizationReadModel,
    )> {
        let Self {
            arn,
            feature_set,
            id,
            master_account_arn,
            master_account_email,
            master_account_id,
            applied_identifier,
        } = self;
        let crate::read_models_generated::OrganizationsOrganizationReadModelSupplement {} =
            supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::OrganizationsOrganizationReadModel {
            feature_set: feature_set.clone(),
            arn: arn.clone(),
            id: id.clone(),
            master_account_arn: master_account_arn.clone(),
            master_account_email: master_account_email.clone(),
            master_account_id: master_account_id.clone(),
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `organizations.Account`.
#[allow(dead_code)]
pub(crate) struct OrganizationsAccountReadFields {
    arn: Option<Value>,
    email: Option<Value>,
    id: Option<Value>,
    joined_method: Option<Value>,
    name: Option<Value>,
    status: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl OrganizationsAccountReadFields {
    pub(crate) fn arn(&self) -> Option<&Value> {
        self.arn.as_ref()
    }

    pub(crate) fn map_arn(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.arn = map(self.arn);
        self
    }

    pub(crate) fn email(&self) -> Option<&Value> {
        self.email.as_ref()
    }

    pub(crate) fn map_email(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.email = map(self.email);
        self
    }

    pub(crate) fn id(&self) -> Option<&Value> {
        self.id.as_ref()
    }

    pub(crate) fn map_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.id = map(self.id);
        self
    }

    pub(crate) fn joined_method(&self) -> Option<&Value> {
        self.joined_method.as_ref()
    }

    pub(crate) fn map_joined_method(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.joined_method = map(self.joined_method);
        self
    }

    pub(crate) fn name(&self) -> Option<&Value> {
        self.name.as_ref()
    }

    pub(crate) fn map_name(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.name = map(self.name);
        self
    }

    pub(crate) fn status(&self) -> Option<&Value> {
        self.status.as_ref()
    }

    pub(crate) fn map_status(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.status = map(self.status);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::OrganizationsAccountReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::OrganizationsAccountReadModel,
    )> {
        let Self {
            arn,
            email,
            id,
            joined_method,
            name,
            status,
            applied_identifier,
        } = self;
        let crate::read_models_generated::OrganizationsAccountReadModelSupplement {
            joined_timestamp,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::OrganizationsAccountReadModel {
            account_name: crate::read_model::require_read_value(
                resource_id,
                name.clone(),
                "account_name",
            )?,
            email: crate::read_model::require_read_value(resource_id, email.clone(), "email")?,
            arn: arn.clone(),
            id: id.clone(),
            joined_method: joined_method.clone(),
            joined_timestamp,
            status: status.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `route53.RecordSet`.
#[allow(dead_code)]
pub(crate) struct Route53RecordSetReadFields {
    name: Option<Value>,
    ttl: Option<Value>,
    r#type: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl Route53RecordSetReadFields {
    pub(crate) fn name(&self) -> Option<&Value> {
        self.name.as_ref()
    }

    pub(crate) fn map_name(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.name = map(self.name);
        self
    }

    pub(crate) fn ttl(&self) -> Option<&Value> {
        self.ttl.as_ref()
    }

    pub(crate) fn map_ttl(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.ttl = map(self.ttl);
        self
    }

    pub(crate) fn r#type(&self) -> Option<&Value> {
        self.r#type.as_ref()
    }

    pub(crate) fn map_type(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.r#type = map(self.r#type);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::Route53RecordSetReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::Route53RecordSetReadModel,
    )> {
        let Self {
            name,
            ttl,
            r#type,
            applied_identifier,
        } = self;
        let crate::read_models_generated::Route53RecordSetReadModelSupplement {
            alias_target,
            resource_records,
            hosted_zone_id,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::Route53RecordSetReadModel {
            alias_target,
            name: crate::read_model::require_read_value(resource_id, name.clone(), "name")?,
            resource_records,
            ttl: ttl.clone(),
            r#type: crate::read_model::require_read_value(resource_id, r#type.clone(), "type")?,
            hosted_zone_id,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `iam.Role`.
#[allow(dead_code)]
pub(crate) struct IamRoleReadFields {
    arn: Option<Value>,
    assume_role_policy_document: Option<Value>,
    description: Option<Value>,
    max_session_duration: Option<Value>,
    path: Option<Value>,
    role_id: Option<Value>,
    role_name: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl IamRoleReadFields {
    pub(crate) fn arn(&self) -> Option<&Value> {
        self.arn.as_ref()
    }

    pub(crate) fn map_arn(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.arn = map(self.arn);
        self
    }

    pub(crate) fn assume_role_policy_document(&self) -> Option<&Value> {
        self.assume_role_policy_document.as_ref()
    }

    pub(crate) fn map_assume_role_policy_document(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.assume_role_policy_document = map(self.assume_role_policy_document);
        self
    }

    pub(crate) fn description(&self) -> Option<&Value> {
        self.description.as_ref()
    }

    pub(crate) fn map_description(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.description = map(self.description);
        self
    }

    pub(crate) fn max_session_duration(&self) -> Option<&Value> {
        self.max_session_duration.as_ref()
    }

    pub(crate) fn map_max_session_duration(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.max_session_duration = map(self.max_session_duration);
        self
    }

    pub(crate) fn path(&self) -> Option<&Value> {
        self.path.as_ref()
    }

    pub(crate) fn map_path(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.path = map(self.path);
        self
    }

    pub(crate) fn role_id(&self) -> Option<&Value> {
        self.role_id.as_ref()
    }

    pub(crate) fn map_role_id(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.role_id = map(self.role_id);
        self
    }

    pub(crate) fn role_name(&self) -> Option<&Value> {
        self.role_name.as_ref()
    }

    pub(crate) fn map_role_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.role_name = map(self.role_name);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::IamRoleReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::IamRoleReadModel,
    )> {
        let Self {
            arn,
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_id,
            role_name,
            applied_identifier,
        } = self;
        let crate::read_models_generated::IamRoleReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::IamRoleReadModel {
            assume_role_policy_document: crate::read_model::require_read_value(
                resource_id,
                assume_role_policy_document.clone(),
                "assume_role_policy_document",
            )?,
            description: description.clone(),
            max_session_duration: max_session_duration.clone(),
            path: path.clone(),
            role_name: crate::read_model::require_read_value(
                resource_id,
                role_name.clone(),
                "role_name",
            )?,
            arn: arn.clone(),
            role_id: role_id.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `logs.LogGroup`.
#[allow(dead_code)]
pub(crate) struct LogsLogGroupReadFields {
    deletion_protection_enabled: Option<Value>,
    kms_key_id: Option<Value>,
    log_group_class: Option<Value>,
    log_group_name: Option<Value>,
    retention_in_days: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl LogsLogGroupReadFields {
    pub(crate) fn deletion_protection_enabled(&self) -> Option<&Value> {
        self.deletion_protection_enabled.as_ref()
    }

    pub(crate) fn map_deletion_protection_enabled(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.deletion_protection_enabled = map(self.deletion_protection_enabled);
        self
    }

    pub(crate) fn kms_key_id(&self) -> Option<&Value> {
        self.kms_key_id.as_ref()
    }

    pub(crate) fn map_kms_key_id(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.kms_key_id = map(self.kms_key_id);
        self
    }

    pub(crate) fn log_group_class(&self) -> Option<&Value> {
        self.log_group_class.as_ref()
    }

    pub(crate) fn map_log_group_class(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_group_class = map(self.log_group_class);
        self
    }

    pub(crate) fn log_group_name(&self) -> Option<&Value> {
        self.log_group_name.as_ref()
    }

    pub(crate) fn map_log_group_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.log_group_name = map(self.log_group_name);
        self
    }

    pub(crate) fn retention_in_days(&self) -> Option<&Value> {
        self.retention_in_days.as_ref()
    }

    pub(crate) fn map_retention_in_days(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.retention_in_days = map(self.retention_in_days);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::LogsLogGroupReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::LogsLogGroupReadModel,
    )> {
        let Self {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            applied_identifier,
        } = self;
        let crate::read_models_generated::LogsLogGroupReadModelSupplement { tags } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::LogsLogGroupReadModel {
            deletion_protection_enabled: crate::read_model::require_read_value(
                resource_id,
                deletion_protection_enabled.clone(),
                "deletion_protection_enabled",
            )?,
            kms_key_id: kms_key_id.clone(),
            log_group_class: log_group_class.clone(),
            log_group_name: crate::read_model::require_read_value(
                resource_id,
                log_group_name.clone(),
                "log_group_name",
            )?,
            retention_in_days: retention_in_days.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

/// Typed values extracted from the AWS read shape for `acm.Certificate`.
#[allow(dead_code)]
pub(crate) struct AcmCertificateReadFields {
    certificate_arn: Option<Value>,
    domain_name: Option<Value>,
    key_algorithm: Option<Value>,
    renewal_eligibility: Option<Value>,
    status: Option<Value>,
    subject_alternative_names: Option<Value>,
    r#type: Option<Value>,
    applied_identifier: Option<String>,
}

#[allow(dead_code, clippy::wrong_self_convention)]
impl AcmCertificateReadFields {
    pub(crate) fn certificate_arn(&self) -> Option<&Value> {
        self.certificate_arn.as_ref()
    }

    pub(crate) fn map_certificate_arn(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.certificate_arn = map(self.certificate_arn);
        self
    }

    pub(crate) fn domain_name(&self) -> Option<&Value> {
        self.domain_name.as_ref()
    }

    pub(crate) fn map_domain_name(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.domain_name = map(self.domain_name);
        self
    }

    pub(crate) fn key_algorithm(&self) -> Option<&Value> {
        self.key_algorithm.as_ref()
    }

    pub(crate) fn map_key_algorithm(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.key_algorithm = map(self.key_algorithm);
        self
    }

    pub(crate) fn renewal_eligibility(&self) -> Option<&Value> {
        self.renewal_eligibility.as_ref()
    }

    pub(crate) fn map_renewal_eligibility(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.renewal_eligibility = map(self.renewal_eligibility);
        self
    }

    pub(crate) fn status(&self) -> Option<&Value> {
        self.status.as_ref()
    }

    pub(crate) fn map_status(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.status = map(self.status);
        self
    }

    pub(crate) fn subject_alternative_names(&self) -> Option<&Value> {
        self.subject_alternative_names.as_ref()
    }

    pub(crate) fn map_subject_alternative_names(
        mut self,
        map: impl FnOnce(Option<Value>) -> Option<Value>,
    ) -> Self {
        self.subject_alternative_names = map(self.subject_alternative_names);
        self
    }

    pub(crate) fn r#type(&self) -> Option<&Value> {
        self.r#type.as_ref()
    }

    pub(crate) fn map_type(mut self, map: impl FnOnce(Option<Value>) -> Option<Value>) -> Self {
        self.r#type = map(self.r#type);
        self
    }

    pub(crate) fn applied_identifier(&self) -> &Option<String> {
        &self.applied_identifier
    }

    pub(crate) fn into_model(
        self,
        resource_id: &ResourceId,
        supplement: crate::read_models_generated::AcmCertificateReadModelSupplement,
    ) -> ProviderResult<(
        Option<String>,
        crate::read_models_generated::AcmCertificateReadModel,
    )> {
        let Self {
            certificate_arn,
            domain_name,
            key_algorithm,
            renewal_eligibility,
            status,
            subject_alternative_names,
            r#type,
            applied_identifier,
        } = self;
        let crate::read_models_generated::AcmCertificateReadModelSupplement {
            options,
            validation_method,
            domain_validation_options,
            renewal_summary,
            tags,
        } = supplement;
        let _ = resource_id;
        let model = crate::read_models_generated::AcmCertificateReadModel {
            domain_name: crate::read_model::require_read_value(
                resource_id,
                domain_name.clone(),
                "domain_name",
            )?,
            key_algorithm: key_algorithm.clone(),
            options,
            subject_alternative_names: crate::read_model::require_read_value(
                resource_id,
                subject_alternative_names.clone(),
                "subject_alternative_names",
            )?,
            validation_method,
            certificate_arn: certificate_arn.clone(),
            domain_validation_options,
            renewal_eligibility: renewal_eligibility.clone(),
            renewal_summary,
            status: status.clone(),
            r#type: r#type.clone(),
            tags,
        };
        Ok((applied_identifier, model))
    }
}

#[allow(dead_code)]
impl AwsProvider {
    /// Delete ec2.Vpc (generated)
    pub(crate) async fn delete_ec2_vpc(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_vpc()
            .vpc_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| api_error_with_meta("Failed to delete vpc", e).for_resource(id.clone()))?;
        Ok(())
    }

    /// Delete ec2.Subnet (generated)
    pub(crate) async fn delete_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_subnet()
            .subnet_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete subnet", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.RouteTable (generated)
    pub(crate) async fn delete_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_route_table()
            .route_table_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete route table", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.SecurityGroup (generated)
    pub(crate) async fn delete_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_security_group()
            .group_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete security group", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.NatGateway (generated)
    pub(crate) async fn delete_ec2_nat_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_nat_gateway()
            .nat_gateway_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete nat gateway", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.TransitGateway (generated)
    pub(crate) async fn delete_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway()
            .transit_gateway_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete transit gateway", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.TransitGatewayAttachment (generated)
    pub(crate) async fn delete_ec2_transit_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway_vpc_attachment()
            .transit_gateway_attachment_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete transit gateway attachment", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Update ec2.InternetGateway: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = crate::send_models_generated::Ec2InternetGatewayUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2InternetGatewayUpdateParts { tags } =
            input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_internet_gateway(&id, identifier).await)
    }

    /// Update ec2.RouteTable: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = crate::send_models_generated::Ec2RouteTableUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2RouteTableUpdateParts { tags } = input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_route_table(&id, identifier).await)
    }

    /// Update ec2.SecurityGroup: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = crate::send_models_generated::Ec2SecurityGroupUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2SecurityGroupUpdateParts { tags } = input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_security_group(&id, identifier).await)
    }

    /// Update organizations.Organization (no-op, just read back current state) (generated)
    pub(crate) async fn update_organizations_organization(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input =
            crate::send_models_generated::OrganizationsOrganizationUpdateInput::decode(&to)?;
        let crate::send_models_generated::OrganizationsOrganizationUpdateParts = input.into_parts();
        Ok(self.read_organizations_organization(&id, identifier).await)
    }

    /// Extract ec2.Vpc attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_attributes(obj: &aws_sdk_ec2::types::Vpc) -> Ec2VpcReadFields {
        let cidr_block = obj
            .cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let instance_tenancy = obj
            .instance_tenancy()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpc_id().map(String::from);
        Ec2VpcReadFields {
            cidr_block,
            instance_tenancy,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Subnet attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_attributes(
        obj: &aws_sdk_ec2::types::Subnet,
    ) -> Ec2SubnetReadFields {
        let assign_ipv6_address_on_creation = obj
            .assign_ipv6_address_on_creation()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let availability_zone = obj
            .availability_zone()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let availability_zone_id = obj
            .availability_zone_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let cidr_block = obj
            .cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let enable_dns64 = obj
            .enable_dns64()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let enable_lni_at_device_index = obj
            .enable_lni_at_device_index()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let ipv6_native = obj
            .ipv6_native()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let map_public_ip_on_launch = obj
            .map_public_ip_on_launch()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let outpost_arn = obj
            .outpost_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let private_dns_name_options_on_launch =
            obj.private_dns_name_options_on_launch()
                .and_then(|dns_opts| {
                    let mut fields = IndexMap::new();
                    if let Some(v) = dns_opts.hostname_type() {
                        fields.insert(
                            "hostname_type".to_string(),
                            Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                        );
                    }
                    if let Some(v) = dns_opts.enable_resource_name_dns_a_record() {
                        fields.insert(
                            "enable_resource_name_dns_a_record".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    if let Some(v) = dns_opts.enable_resource_name_dns_aaaa_record() {
                        fields.insert(
                            "enable_resource_name_dns_aaaa_record".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
                });
        let applied_identifier = obj.subnet_id().map(String::from);
        Ec2SubnetReadFields {
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
            applied_identifier,
        }
    }

    /// Extract ec2.InternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::InternetGateway,
    ) -> Ec2InternetGatewayReadFields {
        let internet_gateway_id = obj
            .internet_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.internet_gateway_id().map(String::from);
        Ec2InternetGatewayReadFields {
            internet_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.RouteTable attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_table_attributes(
        obj: &aws_sdk_ec2::types::RouteTable,
    ) -> Ec2RouteTableReadFields {
        let route_table_id = obj
            .route_table_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.route_table_id().map(String::from);
        Ec2RouteTableReadFields {
            route_table_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Route attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_attributes(
        obj: &aws_sdk_ec2::types::Route,
    ) -> Ec2RouteReadFields {
        let destination_cidr_block = obj
            .destination_cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let gateway_id = obj
            .gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let nat_gateway_id = obj
            .nat_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = None;
        Ec2RouteReadFields {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroup attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroup,
    ) -> Ec2SecurityGroupReadFields {
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let group_name = obj
            .group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.group_id().map(String::from);
        Ec2SecurityGroupReadFields {
            description,
            group_id,
            group_name,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroupIngress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_ingress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
    ) -> Ec2SecurityGroupIngressReadFields {
        let cidr_ipv6 = obj
            .cidr_ipv6()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let from_port = obj
            .from_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ip_protocol = obj
            .ip_protocol()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let security_group_rule_id = obj
            .security_group_rule_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let source_prefix_list_id = obj
            .prefix_list_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let to_port = obj
            .to_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = obj.security_group_rule_id().map(String::from);
        Ec2SecurityGroupIngressReadFields {
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            source_prefix_list_id,
            to_port,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroupEgress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_egress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
    ) -> Ec2SecurityGroupEgressReadFields {
        let cidr_ipv6 = obj
            .cidr_ipv6()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let destination_prefix_list_id = obj
            .prefix_list_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let from_port = obj
            .from_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ip_protocol = obj
            .ip_protocol()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let security_group_rule_id = obj
            .security_group_rule_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let to_port = obj
            .to_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = obj.security_group_rule_id().map(String::from);
        Ec2SecurityGroupEgressReadFields {
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            to_port,
            applied_identifier,
        }
    }

    /// Extract ec2.EgressOnlyInternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_egress_only_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::EgressOnlyInternetGateway,
    ) -> Ec2EgressOnlyInternetGatewayReadFields {
        let egress_only_internet_gateway_id = obj
            .egress_only_internet_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .attachments()
            .first()
            .and_then(|item| item.vpc_id())
            .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
        let applied_identifier = obj.egress_only_internet_gateway_id().map(String::from);
        Ec2EgressOnlyInternetGatewayReadFields {
            egress_only_internet_gateway_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Eip attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_eip_attributes(
        obj: &aws_sdk_ec2::types::Address,
    ) -> Ec2EipReadFields {
        let allocation_id = obj
            .allocation_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let domain = obj
            .domain()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let public_ip = obj
            .public_ip()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let public_ipv4_pool = obj
            .public_ipv4_pool()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.allocation_id().map(String::from);
        Ec2EipReadFields {
            allocation_id,
            domain,
            public_ip,
            public_ipv4_pool,
            applied_identifier,
        }
    }

    /// Extract ec2.FlowLog attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_flow_log_attributes(
        obj: &aws_sdk_ec2::types::FlowLog,
    ) -> Ec2FlowLogReadFields {
        let deliver_logs_permission_arn = obj
            .deliver_logs_permission_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let flow_log_id = obj
            .flow_log_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_destination = obj
            .log_destination()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_destination_type = obj
            .log_destination_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let log_format = obj
            .log_format()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_group_name = obj
            .log_group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let max_aggregation_interval = obj
            .max_aggregation_interval()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let traffic_type = obj
            .traffic_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.flow_log_id().map(String::from);
        Ec2FlowLogReadFields {
            deliver_logs_permission_arn,
            flow_log_id,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            traffic_type,
            applied_identifier,
        }
    }

    /// Extract ec2.NatGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_nat_gateway_attributes(
        obj: &aws_sdk_ec2::types::NatGateway,
    ) -> Ec2NatGatewayReadFields {
        let availability_mode = obj
            .availability_mode()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let connectivity_type = obj
            .connectivity_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let nat_gateway_id = obj
            .nat_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let allocation_id = obj
            .nat_gateway_addresses()
            .first()
            .and_then(|item| item.allocation_id())
            .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
        let applied_identifier = obj.nat_gateway_id().map(String::from);
        Ec2NatGatewayReadFields {
            allocation_id,
            availability_mode,
            connectivity_type,
            nat_gateway_id,
            subnet_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SubnetRouteTableAssociation attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_route_table_association_attributes(
        obj: &aws_sdk_ec2::types::RouteTableAssociation,
    ) -> Ec2SubnetRouteTableAssociationReadFields {
        let route_table_id = obj
            .route_table_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = None;
        Ec2SubnetRouteTableAssociationReadFields {
            route_table_id,
            subnet_id,
            applied_identifier,
        }
    }

    /// Extract ec2.TransitGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attributes(
        obj: &aws_sdk_ec2::types::TransitGateway,
    ) -> Ec2TransitGatewayReadFields {
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let transit_gateway_id = obj
            .transit_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let options = obj.options().and_then(|dns_opts| {
            let mut fields = IndexMap::new();
            if let Some(v) = dns_opts.amazon_side_asn() {
                fields.insert(
                    "amazon_side_asn".to_string(),
                    Value::Concrete(ConcreteValue::Int(v)),
                );
            }
            if let Some(v) = dns_opts.auto_accept_shared_attachments() {
                fields.insert(
                    "auto_accept_shared_attachments".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.default_route_table_association() {
                fields.insert(
                    "default_route_table_association".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.default_route_table_propagation() {
                fields.insert(
                    "default_route_table_propagation".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.dns_support() {
                fields.insert(
                    "dns_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.multicast_support() {
                fields.insert(
                    "multicast_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.security_group_referencing_support() {
                fields.insert(
                    "security_group_referencing_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            {
                let v = dns_opts.transit_gateway_cidr_blocks();
                if !v.is_empty() {
                    fields.insert(
                        "transit_gateway_cidr_blocks".to_string(),
                        Value::Concrete(ConcreteValue::List(
                            v.iter()
                                .map(|s| {
                                    Value::Concrete(ConcreteValue::String(s.to_string().into()))
                                })
                                .collect(),
                        )),
                    );
                }
            }
            if let Some(v) = dns_opts.vpn_ecmp_support() {
                fields.insert(
                    "vpn_ecmp_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
        });
        let applied_identifier = obj.transit_gateway_id().map(String::from);
        Ec2TransitGatewayReadFields {
            description,
            options,
            transit_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.TransitGatewayAttachment attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attachment_attributes(
        obj: &aws_sdk_ec2::types::TransitGatewayVpcAttachment,
    ) -> Ec2TransitGatewayAttachmentReadFields {
        let ids = obj.subnet_ids();
        let subnet_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let transit_gateway_attachment_id = obj
            .transit_gateway_attachment_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let transit_gateway_id = obj
            .transit_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let options = obj.options().and_then(|dns_opts| {
            let mut fields = IndexMap::new();
            if let Some(v) = dns_opts.appliance_mode_support() {
                fields.insert(
                    "appliance_mode_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.dns_support() {
                fields.insert(
                    "dns_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.ipv6_support() {
                fields.insert(
                    "ipv6_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.security_group_referencing_support() {
                fields.insert(
                    "security_group_referencing_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
        });
        let applied_identifier = obj.transit_gateway_attachment_id().map(String::from);
        Ec2TransitGatewayAttachmentReadFields {
            options,
            subnet_ids,
            transit_gateway_attachment_id,
            transit_gateway_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpcEndpoint attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_endpoint_attributes(
        obj: &aws_sdk_ec2::types::VpcEndpoint,
    ) -> Ec2VpcEndpointReadFields {
        let policy_document = obj
            .policy_document()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let private_dns_enabled = obj
            .private_dns_enabled()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let resource_configuration_arn = obj
            .resource_configuration_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ids = obj.route_table_ids();
        let route_table_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let service_name = obj
            .service_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let service_network_arn = obj
            .service_network_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let service_region = obj
            .service_region()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ids = obj.subnet_ids();
        let subnet_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let vpc_endpoint_id = obj
            .vpc_endpoint_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_endpoint_type = obj
            .vpc_endpoint_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let list: Vec<Value> = obj
            .groups()
            .iter()
            .filter_map(|item| {
                item.group_id()
                    .map(|id| Value::Concrete(ConcreteValue::String(id.to_string().into())))
            })
            .collect();
        let security_group_ids =
            (!list.is_empty()).then(|| Value::Concrete(ConcreteValue::List(list)));
        let applied_identifier = obj.vpc_endpoint_id().map(String::from);
        Ec2VpcEndpointReadFields {
            policy_document,
            private_dns_enabled,
            resource_configuration_arn,
            route_table_ids,
            security_group_ids,
            service_name,
            service_network_arn,
            service_region,
            subnet_ids,
            vpc_endpoint_id,
            vpc_endpoint_type,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpcPeeringConnection attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_peering_connection_attributes(
        obj: &aws_sdk_ec2::types::VpcPeeringConnection,
    ) -> Ec2VpcPeeringConnectionReadFields {
        let vpc_peering_connection_id = obj
            .vpc_peering_connection_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .requester_vpc_info()
            .and_then(|opts| opts.vpc_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let peer_vpc_id = obj
            .accepter_vpc_info()
            .and_then(|opts| opts.vpc_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let peer_owner_id = obj
            .accepter_vpc_info()
            .and_then(|opts| opts.owner_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpc_peering_connection_id().map(String::from);
        Ec2VpcPeeringConnectionReadFields {
            peer_owner_id,
            peer_vpc_id,
            vpc_id,
            vpc_peering_connection_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpnGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpn_gateway_attributes(
        obj: &aws_sdk_ec2::types::VpnGateway,
    ) -> Ec2VpnGatewayReadFields {
        let amazon_side_asn = obj
            .amazon_side_asn()
            .map(|v| Value::Concrete(ConcreteValue::Int(v)));
        let availability_zone = obj
            .availability_zone()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let r#type = obj
            .r#type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpn_gateway_id = obj
            .vpn_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpn_gateway_id().map(String::from);
        Ec2VpnGatewayReadFields {
            amazon_side_asn,
            availability_zone,
            r#type,
            vpn_gateway_id,
            applied_identifier,
        }
    }

    /// Extract organizations.Organization attributes from SDK response type (generated)
    pub(crate) fn extract_organizations_organization_attributes(
        obj: &aws_sdk_organizations::types::Organization,
    ) -> OrganizationsOrganizationReadFields {
        let arn = obj
            .arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let feature_set = obj
            .feature_set()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let id = obj
            .id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_arn = obj
            .master_account_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_email = obj
            .master_account_email()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_id = obj
            .master_account_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.id().map(String::from);
        OrganizationsOrganizationReadFields {
            arn,
            feature_set,
            id,
            master_account_arn,
            master_account_email,
            master_account_id,
            applied_identifier,
        }
    }

    /// Extract organizations.Account attributes from SDK response type (generated)
    pub(crate) fn extract_organizations_account_attributes(
        obj: &aws_sdk_organizations::types::Account,
    ) -> OrganizationsAccountReadFields {
        let arn = obj
            .arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let email = obj
            .email()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let id = obj
            .id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let joined_method = obj
            .joined_method()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let name = obj
            .name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let status = obj
            .status()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.id().map(String::from);
        OrganizationsAccountReadFields {
            arn,
            email,
            id,
            joined_method,
            name,
            status,
            applied_identifier,
        }
    }

    /// Extract route53.RecordSet attributes from SDK response type (generated)
    pub(crate) fn extract_route53_record_set_attributes(
        obj: &aws_sdk_route53::types::ResourceRecordSet,
    ) -> Route53RecordSetReadFields {
        let v = obj.name();
        let name =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ttl = obj.ttl().map(|v| Value::Concrete(ConcreteValue::Int(v)));
        let r#type = Some(Value::Concrete(ConcreteValue::String(
            obj.r#type().as_str().to_string().into(),
        )));
        let applied_identifier = Some(obj.name().to_string());
        Route53RecordSetReadFields {
            name,
            ttl,
            r#type,
            applied_identifier,
        }
    }

    /// Extract iam.Role attributes from SDK response type (generated)
    pub(crate) fn extract_iam_role_attributes(obj: &aws_sdk_iam::types::Role) -> IamRoleReadFields {
        let v = obj.arn();
        let arn =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let assume_role_policy_document = obj
            .assume_role_policy_document()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let max_session_duration = obj
            .max_session_duration()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let v = obj.path();
        let path =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let v = obj.role_id();
        let role_id =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let v = obj.role_name();
        let role_name =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = Some(obj.role_name().to_string());
        IamRoleReadFields {
            arn,
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_id,
            role_name,
            applied_identifier,
        }
    }

    /// Extract logs.LogGroup attributes from SDK response type (generated)
    pub(crate) fn extract_logs_log_group_attributes(
        obj: &aws_sdk_cloudwatchlogs::types::LogGroup,
    ) -> LogsLogGroupReadFields {
        let deletion_protection_enabled = obj
            .deletion_protection_enabled()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let kms_key_id = obj
            .kms_key_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_group_class = obj
            .log_group_class()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let log_group_name = obj
            .log_group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let retention_in_days = obj
            .retention_in_days()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = None;
        LogsLogGroupReadFields {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            applied_identifier,
        }
    }

    /// Extract acm.Certificate attributes from SDK response type (generated)
    pub(crate) fn extract_acm_certificate_attributes(
        obj: &aws_sdk_acm::types::CertificateDetail,
    ) -> AcmCertificateReadFields {
        let certificate_arn = obj
            .certificate_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let domain_name = obj
            .domain_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let key_algorithm = obj
            .key_algorithm()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let renewal_eligibility = obj
            .renewal_eligibility()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let status = obj
            .status()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let ids = obj.subject_alternative_names();
        let subject_alternative_names = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let r#type = obj
            .r#type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.certificate_arn().map(String::from);
        AcmCertificateReadFields {
            certificate_arn,
            domain_name,
            key_algorithm,
            renewal_eligibility,
            status,
            subject_alternative_names,
            r#type,
            applied_identifier,
        }
    }
}

// ===== Generated DataSourceLookups Trait =====

/// One method per `DataSourceDef`. AwsProvider must implement all of
/// them; the codegen-emitted dispatcher below routes by
/// `resource.id.resource_type`.
pub trait DataSourceLookups {
    fn read_sts_caller_identity_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_identitystore_user_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_s3_bucket_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_iam_roles_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;
}

// ===== Generated read_data_source dispatcher =====

/// Routes `Provider::read_data_source` calls to the matching
/// `DataSourceLookups` trait method. The default arm refuses
/// to drop user-supplied inputs silently.
pub(crate) fn dispatch_read_data_source<'a>(
    provider: &'a AwsProvider,
    resource: &'a DataSource<impl Clone + Send + Sync + 'static>,
) -> BoxFuture<'a, ProviderResult<State>> {
    match resource.id.resource_type.as_str() {
        "sts.CallerIdentity" => provider.read_sts_caller_identity_data_source(resource),
        "identitystore.User" => provider.read_identitystore_user_data_source(resource),
        "s3.Bucket" => provider.read_s3_bucket_data_source(resource),
        "iam.Roles" => provider.read_iam_roles_data_source(resource),
        _ => {
            let id = resource.id.clone();
            let resource_type = resource.id.resource_type.clone();
            Box::pin(async move {
                Err(ProviderError::internal(format!(
                    "aws provider does not implement read_data_source for '{}'",
                    resource_type
                ))
                .for_resource(id))
            })
        }
    }
}
