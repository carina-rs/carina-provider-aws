//! Auto-generated managed-resource read models.
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./scripts/generate-provider.sh

use std::collections::HashMap;

use std::sync::OnceLock;

use carina_core::provider::ProviderResult;
use carina_core::resource::Value;

use carina_core::schema::ValidatedResourceSchema;

use crate::read_model::{
    ManagedReadModel, cached_validated_read_schema, sealed, validate_read_attributes,
};

/// Exhaustive canonical read state for `ec2.Vpc`.
pub(crate) struct Ec2VpcReadModel {
    pub(crate) cidr_block: Option<Value>,
    pub(crate) enable_dns_hostnames: Value,
    pub(crate) enable_dns_support: Value,
    pub(crate) instance_tenancy: Option<Value>,
    pub(crate) ipv4_netmask_length: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.Vpc` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2VpcReadModelSupplement {
    pub(crate) enable_dns_hostnames: Value,
    pub(crate) enable_dns_support: Value,
    pub(crate) ipv4_netmask_length: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2VpcReadModel {}

impl ManagedReadModel for Ec2VpcReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            cidr_block,
            enable_dns_hostnames,
            enable_dns_support,
            instance_tenancy,
            ipv4_netmask_length,
            vpc_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(7);
        if let Some(value) = cidr_block {
            attributes.insert("cidr_block".to_string(), value);
        }
        attributes.insert("enable_dns_hostnames".to_string(), enable_dns_hostnames);
        attributes.insert("enable_dns_support".to_string(), enable_dns_support);
        if let Some(value) = instance_tenancy {
            attributes.insert("instance_tenancy".to_string(), value);
        }
        if let Some(value) = ipv4_netmask_length {
            attributes.insert("ipv4_netmask_length".to_string(), value);
        }
        if let Some(value) = vpc_id {
            attributes.insert("vpc_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.Vpc", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::vpc::ec2_vpc_config().schema
        })?;
        validate_read_attributes("ec2.Vpc", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.Subnet`.
pub(crate) struct Ec2SubnetReadModel {
    pub(crate) assign_ipv6_address_on_creation: Option<Value>,
    pub(crate) availability_zone: Option<Value>,
    pub(crate) availability_zone_id: Option<Value>,
    pub(crate) cidr_block: Option<Value>,
    pub(crate) enable_dns64: Option<Value>,
    pub(crate) enable_lni_at_device_index: Option<Value>,
    pub(crate) ipv4_netmask_length: Option<Value>,
    pub(crate) ipv6_cidr_block: Option<Value>,
    pub(crate) ipv6_native: Option<Value>,
    pub(crate) ipv6_netmask_length: Option<Value>,
    pub(crate) map_public_ip_on_launch: Option<Value>,
    pub(crate) outpost_arn: Option<Value>,
    pub(crate) private_dns_name_options_on_launch: Option<Value>,
    pub(crate) vpc_id: Value,
    pub(crate) subnet_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.Subnet` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2SubnetReadModelSupplement {
    pub(crate) ipv4_netmask_length: Option<Value>,
    pub(crate) ipv6_cidr_block: Option<Value>,
    pub(crate) ipv6_netmask_length: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2SubnetReadModel {}

impl ManagedReadModel for Ec2SubnetReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            assign_ipv6_address_on_creation,
            availability_zone,
            availability_zone_id,
            cidr_block,
            enable_dns64,
            enable_lni_at_device_index,
            ipv4_netmask_length,
            ipv6_cidr_block,
            ipv6_native,
            ipv6_netmask_length,
            map_public_ip_on_launch,
            outpost_arn,
            private_dns_name_options_on_launch,
            vpc_id,
            subnet_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(16);
        if let Some(value) = assign_ipv6_address_on_creation {
            attributes.insert("assign_ipv6_address_on_creation".to_string(), value);
        }
        if let Some(value) = availability_zone {
            attributes.insert("availability_zone".to_string(), value);
        }
        if let Some(value) = availability_zone_id {
            attributes.insert("availability_zone_id".to_string(), value);
        }
        if let Some(value) = cidr_block {
            attributes.insert("cidr_block".to_string(), value);
        }
        if let Some(value) = enable_dns64 {
            attributes.insert("enable_dns64".to_string(), value);
        }
        if let Some(value) = enable_lni_at_device_index {
            attributes.insert("enable_lni_at_device_index".to_string(), value);
        }
        if let Some(value) = ipv4_netmask_length {
            attributes.insert("ipv4_netmask_length".to_string(), value);
        }
        if let Some(value) = ipv6_cidr_block {
            attributes.insert("ipv6_cidr_block".to_string(), value);
        }
        if let Some(value) = ipv6_native {
            attributes.insert("ipv6_native".to_string(), value);
        }
        if let Some(value) = ipv6_netmask_length {
            attributes.insert("ipv6_netmask_length".to_string(), value);
        }
        if let Some(value) = map_public_ip_on_launch {
            attributes.insert("map_public_ip_on_launch".to_string(), value);
        }
        if let Some(value) = outpost_arn {
            attributes.insert("outpost_arn".to_string(), value);
        }
        if let Some(value) = private_dns_name_options_on_launch {
            attributes.insert("private_dns_name_options_on_launch".to_string(), value);
        }
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = subnet_id {
            attributes.insert("subnet_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.Subnet", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::subnet::ec2_subnet_config().schema
        })?;
        validate_read_attributes("ec2.Subnet", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.InternetGateway`.
pub(crate) struct Ec2InternetGatewayReadModel {
    pub(crate) vpc_id: Value,
    pub(crate) internet_gateway_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.InternetGateway` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2InternetGatewayReadModelSupplement {
    pub(crate) vpc_id: Value,
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2InternetGatewayReadModel {}

impl ManagedReadModel for Ec2InternetGatewayReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            vpc_id,
            internet_gateway_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = internet_gateway_id {
            attributes.insert("internet_gateway_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema =
            cached_validated_read_schema("ec2.InternetGateway", &VALIDATED_SCHEMA, || {
                crate::schemas::generated::ec2::internet_gateway::ec2_internet_gateway_config()
                    .schema
            })?;
        validate_read_attributes("ec2.InternetGateway", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.RouteTable`.
pub(crate) struct Ec2RouteTableReadModel {
    pub(crate) vpc_id: Value,
    pub(crate) route_table_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.RouteTable` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2RouteTableReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2RouteTableReadModel {}

impl ManagedReadModel for Ec2RouteTableReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            vpc_id,
            route_table_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = route_table_id {
            attributes.insert("route_table_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.RouteTable", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::route_table::ec2_route_table_config().schema
        })?;
        validate_read_attributes("ec2.RouteTable", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.Route`.
pub(crate) struct Ec2RouteReadModel {
    pub(crate) destination_cidr_block: Option<Value>,
    pub(crate) gateway_id: Option<Value>,
    pub(crate) nat_gateway_id: Option<Value>,
    pub(crate) route_table_id: Value,
}

/// Canonical fields for `ec2.Route` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2RouteReadModelSupplement {
    pub(crate) route_table_id: Value,
}

impl sealed::Sealed for Ec2RouteReadModel {}

impl ManagedReadModel for Ec2RouteReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            route_table_id,
        } = self;
        let mut attributes = HashMap::with_capacity(4);
        if let Some(value) = destination_cidr_block {
            attributes.insert("destination_cidr_block".to_string(), value);
        }
        if let Some(value) = gateway_id {
            attributes.insert("gateway_id".to_string(), value);
        }
        if let Some(value) = nat_gateway_id {
            attributes.insert("nat_gateway_id".to_string(), value);
        }
        attributes.insert("route_table_id".to_string(), route_table_id);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.Route", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::route::ec2_route_config().schema
        })?;
        validate_read_attributes("ec2.Route", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.SecurityGroup`.
pub(crate) struct Ec2SecurityGroupReadModel {
    pub(crate) description: Value,
    pub(crate) group_name: Value,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) group_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.SecurityGroup` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2SecurityGroupReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2SecurityGroupReadModel {}

impl ManagedReadModel for Ec2SecurityGroupReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            description,
            group_name,
            vpc_id,
            group_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(5);
        attributes.insert("description".to_string(), description);
        attributes.insert("group_name".to_string(), group_name);
        if let Some(value) = vpc_id {
            attributes.insert("vpc_id".to_string(), value);
        }
        if let Some(value) = group_id {
            attributes.insert("group_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.SecurityGroup", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::security_group::ec2_security_group_config().schema
        })?;
        validate_read_attributes("ec2.SecurityGroup", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.SecurityGroupIngress`.
pub(crate) struct Ec2SecurityGroupIngressReadModel {
    pub(crate) cidr_ip: Option<Value>,
    pub(crate) cidr_ipv6: Option<Value>,
    pub(crate) description: Option<Value>,
    pub(crate) from_port: Option<Value>,
    pub(crate) group_id: Option<Value>,
    pub(crate) ip_protocol: Value,
    pub(crate) source_prefix_list_id: Option<Value>,
    pub(crate) source_security_group_owner_id: Option<Value>,
    pub(crate) to_port: Option<Value>,
    pub(crate) source_security_group_id: Option<Value>,
    pub(crate) security_group_rule_id: Option<Value>,
}

/// Canonical fields for `ec2.SecurityGroupIngress` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2SecurityGroupIngressReadModelSupplement {
    pub(crate) cidr_ip: Option<Value>,
    pub(crate) source_security_group_owner_id: Option<Value>,
    pub(crate) source_security_group_id: Option<Value>,
}

impl sealed::Sealed for Ec2SecurityGroupIngressReadModel {}

impl ManagedReadModel for Ec2SecurityGroupIngressReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            cidr_ip,
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            source_prefix_list_id,
            source_security_group_owner_id,
            to_port,
            source_security_group_id,
            security_group_rule_id,
        } = self;
        let mut attributes = HashMap::with_capacity(11);
        if let Some(value) = cidr_ip {
            attributes.insert("cidr_ip".to_string(), value);
        }
        if let Some(value) = cidr_ipv6 {
            attributes.insert("cidr_ipv6".to_string(), value);
        }
        if let Some(value) = description {
            attributes.insert("description".to_string(), value);
        }
        if let Some(value) = from_port {
            attributes.insert("from_port".to_string(), value);
        }
        if let Some(value) = group_id {
            attributes.insert("group_id".to_string(), value);
        }
        attributes.insert("ip_protocol".to_string(), ip_protocol);
        if let Some(value) = source_prefix_list_id {
            attributes.insert("source_prefix_list_id".to_string(), value);
        }
        if let Some(value) = source_security_group_owner_id {
            attributes.insert("source_security_group_owner_id".to_string(), value);
        }
        if let Some(value) = to_port {
            attributes.insert("to_port".to_string(), value);
        }
        if let Some(value) = source_security_group_id {
            attributes.insert("source_security_group_id".to_string(), value);
        }
        if let Some(value) = security_group_rule_id {
            attributes.insert("security_group_rule_id".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.SecurityGroupIngress",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config().schema
            },
        )?;
        validate_read_attributes("ec2.SecurityGroupIngress", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.SecurityGroupEgress`.
pub(crate) struct Ec2SecurityGroupEgressReadModel {
    pub(crate) cidr_ip: Option<Value>,
    pub(crate) cidr_ipv6: Option<Value>,
    pub(crate) description: Option<Value>,
    pub(crate) destination_prefix_list_id: Option<Value>,
    pub(crate) from_port: Option<Value>,
    pub(crate) group_id: Value,
    pub(crate) ip_protocol: Value,
    pub(crate) source_security_group_owner_id: Option<Value>,
    pub(crate) to_port: Option<Value>,
    pub(crate) destination_security_group_id: Option<Value>,
    pub(crate) security_group_rule_id: Option<Value>,
}

/// Canonical fields for `ec2.SecurityGroupEgress` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2SecurityGroupEgressReadModelSupplement {
    pub(crate) cidr_ip: Option<Value>,
    pub(crate) source_security_group_owner_id: Option<Value>,
    pub(crate) destination_security_group_id: Option<Value>,
}

impl sealed::Sealed for Ec2SecurityGroupEgressReadModel {}

impl ManagedReadModel for Ec2SecurityGroupEgressReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            cidr_ip,
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            from_port,
            group_id,
            ip_protocol,
            source_security_group_owner_id,
            to_port,
            destination_security_group_id,
            security_group_rule_id,
        } = self;
        let mut attributes = HashMap::with_capacity(11);
        if let Some(value) = cidr_ip {
            attributes.insert("cidr_ip".to_string(), value);
        }
        if let Some(value) = cidr_ipv6 {
            attributes.insert("cidr_ipv6".to_string(), value);
        }
        if let Some(value) = description {
            attributes.insert("description".to_string(), value);
        }
        if let Some(value) = destination_prefix_list_id {
            attributes.insert("destination_prefix_list_id".to_string(), value);
        }
        if let Some(value) = from_port {
            attributes.insert("from_port".to_string(), value);
        }
        attributes.insert("group_id".to_string(), group_id);
        attributes.insert("ip_protocol".to_string(), ip_protocol);
        if let Some(value) = source_security_group_owner_id {
            attributes.insert("source_security_group_owner_id".to_string(), value);
        }
        if let Some(value) = to_port {
            attributes.insert("to_port".to_string(), value);
        }
        if let Some(value) = destination_security_group_id {
            attributes.insert("destination_security_group_id".to_string(), value);
        }
        if let Some(value) = security_group_rule_id {
            attributes.insert("security_group_rule_id".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.SecurityGroupEgress",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::security_group_egress::ec2_security_group_egress_config().schema
            },
        )?;
        validate_read_attributes("ec2.SecurityGroupEgress", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.EgressOnlyInternetGateway`.
pub(crate) struct Ec2EgressOnlyInternetGatewayReadModel {
    pub(crate) vpc_id: Value,
    pub(crate) egress_only_internet_gateway_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.EgressOnlyInternetGateway` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2EgressOnlyInternetGatewayReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2EgressOnlyInternetGatewayReadModel {}

impl ManagedReadModel for Ec2EgressOnlyInternetGatewayReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            vpc_id,
            egress_only_internet_gateway_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = egress_only_internet_gateway_id {
            attributes.insert("egress_only_internet_gateway_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.EgressOnlyInternetGateway",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config().schema
            },
        )?;
        validate_read_attributes("ec2.EgressOnlyInternetGateway", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.Eip`.
pub(crate) struct Ec2EipReadModel {
    pub(crate) address: Option<Value>,
    pub(crate) domain: Option<Value>,
    pub(crate) public_ipv4_pool: Option<Value>,
    pub(crate) allocation_id: Option<Value>,
    pub(crate) public_ip: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.Eip` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2EipReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2EipReadModel {}

impl ManagedReadModel for Ec2EipReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            address,
            domain,
            public_ipv4_pool,
            allocation_id,
            public_ip,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(6);
        if let Some(value) = address {
            attributes.insert("address".to_string(), value);
        }
        if let Some(value) = domain {
            attributes.insert("domain".to_string(), value);
        }
        if let Some(value) = public_ipv4_pool {
            attributes.insert("public_ipv4_pool".to_string(), value);
        }
        if let Some(value) = allocation_id {
            attributes.insert("allocation_id".to_string(), value);
        }
        if let Some(value) = public_ip {
            attributes.insert("public_ip".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.Eip", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::eip::ec2_eip_config().schema
        })?;
        validate_read_attributes("ec2.Eip", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.FlowLog`.
pub(crate) struct Ec2FlowLogReadModel {
    pub(crate) deliver_logs_permission_arn: Option<Value>,
    pub(crate) log_destination: Option<Value>,
    pub(crate) log_destination_type: Option<Value>,
    pub(crate) log_format: Option<Value>,
    pub(crate) log_group_name: Option<Value>,
    pub(crate) max_aggregation_interval: Option<Value>,
    pub(crate) resource_ids: Value,
    pub(crate) resource_type: Value,
    pub(crate) traffic_type: Option<Value>,
    pub(crate) flow_log_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.FlowLog` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2FlowLogReadModelSupplement {
    pub(crate) resource_ids: Value,
    pub(crate) resource_type: Value,
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2FlowLogReadModel {}

impl ManagedReadModel for Ec2FlowLogReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            deliver_logs_permission_arn,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            resource_ids,
            resource_type,
            traffic_type,
            flow_log_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(11);
        if let Some(value) = deliver_logs_permission_arn {
            attributes.insert("deliver_logs_permission_arn".to_string(), value);
        }
        if let Some(value) = log_destination {
            attributes.insert("log_destination".to_string(), value);
        }
        if let Some(value) = log_destination_type {
            attributes.insert("log_destination_type".to_string(), value);
        }
        if let Some(value) = log_format {
            attributes.insert("log_format".to_string(), value);
        }
        if let Some(value) = log_group_name {
            attributes.insert("log_group_name".to_string(), value);
        }
        if let Some(value) = max_aggregation_interval {
            attributes.insert("max_aggregation_interval".to_string(), value);
        }
        attributes.insert("resource_ids".to_string(), resource_ids);
        attributes.insert("resource_type".to_string(), resource_type);
        if let Some(value) = traffic_type {
            attributes.insert("traffic_type".to_string(), value);
        }
        if let Some(value) = flow_log_id {
            attributes.insert("flow_log_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.FlowLog", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::flow_log::ec2_flow_log_config().schema
        })?;
        validate_read_attributes("ec2.FlowLog", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.NatGateway`.
pub(crate) struct Ec2NatGatewayReadModel {
    pub(crate) allocation_id: Option<Value>,
    pub(crate) availability_mode: Option<Value>,
    pub(crate) availability_zone_addresses: Value,
    pub(crate) connectivity_type: Option<Value>,
    pub(crate) private_ip_address: Option<Value>,
    pub(crate) subnet_id: Value,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) nat_gateway_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.NatGateway` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2NatGatewayReadModelSupplement {
    pub(crate) availability_zone_addresses: Value,
    pub(crate) private_ip_address: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2NatGatewayReadModel {}

impl ManagedReadModel for Ec2NatGatewayReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            allocation_id,
            availability_mode,
            availability_zone_addresses,
            connectivity_type,
            private_ip_address,
            subnet_id,
            vpc_id,
            nat_gateway_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(9);
        if let Some(value) = allocation_id {
            attributes.insert("allocation_id".to_string(), value);
        }
        if let Some(value) = availability_mode {
            attributes.insert("availability_mode".to_string(), value);
        }
        attributes.insert(
            "availability_zone_addresses".to_string(),
            availability_zone_addresses,
        );
        if let Some(value) = connectivity_type {
            attributes.insert("connectivity_type".to_string(), value);
        }
        if let Some(value) = private_ip_address {
            attributes.insert("private_ip_address".to_string(), value);
        }
        attributes.insert("subnet_id".to_string(), subnet_id);
        if let Some(value) = vpc_id {
            attributes.insert("vpc_id".to_string(), value);
        }
        if let Some(value) = nat_gateway_id {
            attributes.insert("nat_gateway_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.NatGateway", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema
        })?;
        validate_read_attributes("ec2.NatGateway", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.SubnetRouteTableAssociation`.
pub(crate) struct Ec2SubnetRouteTableAssociationReadModel {
    pub(crate) route_table_id: Value,
    pub(crate) subnet_id: Value,
}

/// Canonical fields for `ec2.SubnetRouteTableAssociation` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2SubnetRouteTableAssociationReadModelSupplement {}

impl sealed::Sealed for Ec2SubnetRouteTableAssociationReadModel {}

impl ManagedReadModel for Ec2SubnetRouteTableAssociationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            route_table_id,
            subnet_id,
        } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("route_table_id".to_string(), route_table_id);
        attributes.insert("subnet_id".to_string(), subnet_id);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.SubnetRouteTableAssociation",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::subnet_route_table_association::ec2_subnet_route_table_association_config().schema
            },
        )?;
        validate_read_attributes("ec2.SubnetRouteTableAssociation", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.TransitGateway`.
pub(crate) struct Ec2TransitGatewayReadModel {
    pub(crate) description: Option<Value>,
    pub(crate) options: Option<Value>,
    pub(crate) transit_gateway_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.TransitGateway` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2TransitGatewayReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2TransitGatewayReadModel {}

impl ManagedReadModel for Ec2TransitGatewayReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            description,
            options,
            transit_gateway_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(4);
        if let Some(value) = description {
            attributes.insert("description".to_string(), value);
        }
        if let Some(value) = options {
            attributes.insert("options".to_string(), value);
        }
        if let Some(value) = transit_gateway_id {
            attributes.insert("transit_gateway_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.TransitGateway", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema
        })?;
        validate_read_attributes("ec2.TransitGateway", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.TransitGatewayAttachment`.
pub(crate) struct Ec2TransitGatewayAttachmentReadModel {
    pub(crate) options: Option<Value>,
    pub(crate) subnet_ids: Value,
    pub(crate) transit_gateway_id: Value,
    pub(crate) vpc_id: Value,
    pub(crate) transit_gateway_attachment_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.TransitGatewayAttachment` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2TransitGatewayAttachmentReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2TransitGatewayAttachmentReadModel {}

impl ManagedReadModel for Ec2TransitGatewayAttachmentReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            options,
            subnet_ids,
            transit_gateway_id,
            vpc_id,
            transit_gateway_attachment_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(6);
        if let Some(value) = options {
            attributes.insert("options".to_string(), value);
        }
        attributes.insert("subnet_ids".to_string(), subnet_ids);
        attributes.insert("transit_gateway_id".to_string(), transit_gateway_id);
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = transit_gateway_attachment_id {
            attributes.insert("transit_gateway_attachment_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.TransitGatewayAttachment",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema
            },
        )?;
        validate_read_attributes("ec2.TransitGatewayAttachment", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.VpcEndpoint`.
pub(crate) struct Ec2VpcEndpointReadModel {
    pub(crate) policy_document: Option<Value>,
    pub(crate) private_dns_enabled: Option<Value>,
    pub(crate) resource_configuration_arn: Option<Value>,
    pub(crate) route_table_ids: Value,
    pub(crate) security_group_ids: Value,
    pub(crate) service_name: Value,
    pub(crate) service_network_arn: Option<Value>,
    pub(crate) service_region: Option<Value>,
    pub(crate) subnet_ids: Value,
    pub(crate) vpc_endpoint_type: Option<Value>,
    pub(crate) vpc_id: Value,
    pub(crate) vpc_endpoint_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.VpcEndpoint` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2VpcEndpointReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2VpcEndpointReadModel {}

impl ManagedReadModel for Ec2VpcEndpointReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
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
            vpc_endpoint_type,
            vpc_id,
            vpc_endpoint_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(13);
        if let Some(value) = policy_document {
            attributes.insert("policy_document".to_string(), value);
        }
        if let Some(value) = private_dns_enabled {
            attributes.insert("private_dns_enabled".to_string(), value);
        }
        if let Some(value) = resource_configuration_arn {
            attributes.insert("resource_configuration_arn".to_string(), value);
        }
        attributes.insert("route_table_ids".to_string(), route_table_ids);
        attributes.insert("security_group_ids".to_string(), security_group_ids);
        attributes.insert("service_name".to_string(), service_name);
        if let Some(value) = service_network_arn {
            attributes.insert("service_network_arn".to_string(), value);
        }
        if let Some(value) = service_region {
            attributes.insert("service_region".to_string(), value);
        }
        attributes.insert("subnet_ids".to_string(), subnet_ids);
        if let Some(value) = vpc_endpoint_type {
            attributes.insert("vpc_endpoint_type".to_string(), value);
        }
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = vpc_endpoint_id {
            attributes.insert("vpc_endpoint_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.VpcEndpoint", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::vpc_endpoint::ec2_vpc_endpoint_config().schema
        })?;
        validate_read_attributes("ec2.VpcEndpoint", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.VpcGatewayAttachment`.
pub(crate) struct Ec2VpcGatewayAttachmentReadModel {
    pub(crate) internet_gateway_id: Option<Value>,
    pub(crate) vpc_id: Value,
    pub(crate) vpn_gateway_id: Option<Value>,
}

impl sealed::Sealed for Ec2VpcGatewayAttachmentReadModel {}

impl ManagedReadModel for Ec2VpcGatewayAttachmentReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            internet_gateway_id,
            vpc_id,
            vpn_gateway_id,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        if let Some(value) = internet_gateway_id {
            attributes.insert("internet_gateway_id".to_string(), value);
        }
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = vpn_gateway_id {
            attributes.insert("vpn_gateway_id".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.VpcGatewayAttachment",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config().schema
            },
        )?;
        validate_read_attributes("ec2.VpcGatewayAttachment", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.VpcPeeringConnection`.
pub(crate) struct Ec2VpcPeeringConnectionReadModel {
    pub(crate) peer_owner_id: Option<Value>,
    pub(crate) peer_vpc_id: Value,
    pub(crate) vpc_id: Value,
    pub(crate) vpc_peering_connection_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.VpcPeeringConnection` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2VpcPeeringConnectionReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2VpcPeeringConnectionReadModel {}

impl ManagedReadModel for Ec2VpcPeeringConnectionReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            peer_owner_id,
            peer_vpc_id,
            vpc_id,
            vpc_peering_connection_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(5);
        if let Some(value) = peer_owner_id {
            attributes.insert("peer_owner_id".to_string(), value);
        }
        attributes.insert("peer_vpc_id".to_string(), peer_vpc_id);
        attributes.insert("vpc_id".to_string(), vpc_id);
        if let Some(value) = vpc_peering_connection_id {
            attributes.insert("vpc_peering_connection_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "ec2.VpcPeeringConnection",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config().schema
            },
        )?;
        validate_read_attributes("ec2.VpcPeeringConnection", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `ec2.VpnGateway`.
pub(crate) struct Ec2VpnGatewayReadModel {
    pub(crate) amazon_side_asn: Option<Value>,
    pub(crate) availability_zone: Option<Value>,
    pub(crate) r#type: Value,
    pub(crate) vpn_gateway_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `ec2.VpnGateway` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Ec2VpnGatewayReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for Ec2VpnGatewayReadModel {}

impl ManagedReadModel for Ec2VpnGatewayReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            amazon_side_asn,
            availability_zone,
            r#type,
            vpn_gateway_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(5);
        if let Some(value) = amazon_side_asn {
            attributes.insert("amazon_side_asn".to_string(), value);
        }
        if let Some(value) = availability_zone {
            attributes.insert("availability_zone".to_string(), value);
        }
        attributes.insert("type".to_string(), r#type);
        if let Some(value) = vpn_gateway_id {
            attributes.insert("vpn_gateway_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("ec2.VpnGateway", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::ec2::vpn_gateway::ec2_vpn_gateway_config().schema
        })?;
        validate_read_attributes("ec2.VpnGateway", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.Bucket`.
pub(crate) struct S3BucketReadModel {
    pub(crate) bucket: Value,
    pub(crate) arn: Value,
    pub(crate) region: Value,
    pub(crate) bucket_domain_name: Value,
    pub(crate) bucket_regional_domain_name: Value,
    pub(crate) hosted_zone_id: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for S3BucketReadModel {}

impl ManagedReadModel for S3BucketReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            arn,
            region,
            bucket_domain_name,
            bucket_regional_domain_name,
            hosted_zone_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(7);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("arn".to_string(), arn);
        attributes.insert("region".to_string(), region);
        attributes.insert("bucket_domain_name".to_string(), bucket_domain_name);
        attributes.insert(
            "bucket_regional_domain_name".to_string(),
            bucket_regional_domain_name,
        );
        if let Some(value) = hosted_zone_id {
            attributes.insert("hosted_zone_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("s3.Bucket", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::s3::bucket::s3_bucket_config().schema
        })?;
        validate_read_attributes("s3.Bucket", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketPolicy`.
pub(crate) struct S3BucketPolicyReadModel {
    pub(crate) bucket: Value,
    pub(crate) policy: Value,
}

impl sealed::Sealed for S3BucketPolicyReadModel {}

impl ManagedReadModel for S3BucketPolicyReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { bucket, policy } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("policy".to_string(), policy);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("s3.BucketPolicy", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::s3::bucket_policy::s3_bucket_policy_config().schema
        })?;
        validate_read_attributes("s3.BucketPolicy", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketVersioning`.
pub(crate) struct S3BucketVersioningReadModel {
    pub(crate) bucket: Value,
    pub(crate) status: Value,
}

impl sealed::Sealed for S3BucketVersioningReadModel {}

impl ManagedReadModel for S3BucketVersioningReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { bucket, status } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("status".to_string(), status);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema =
            cached_validated_read_schema("s3.BucketVersioning", &VALIDATED_SCHEMA, || {
                crate::schemas::generated::s3::bucket_versioning::s3_bucket_versioning_config()
                    .schema
            })?;
        validate_read_attributes("s3.BucketVersioning", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketObjectLockConfiguration`.
pub(crate) struct S3BucketObjectLockConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) object_lock_enabled: Value,
    pub(crate) rule: Option<Value>,
}

impl sealed::Sealed for S3BucketObjectLockConfigurationReadModel {}

impl ManagedReadModel for S3BucketObjectLockConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            object_lock_enabled,
            rule,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("object_lock_enabled".to_string(), object_lock_enabled);
        if let Some(value) = rule {
            attributes.insert("rule".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketObjectLockConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_object_lock_configuration::s3_bucket_object_lock_configuration_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketObjectLockConfiguration", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketAcl`.
pub(crate) struct S3BucketAclReadModel {
    pub(crate) acl: Value,
    pub(crate) bucket: Value,
}

impl sealed::Sealed for S3BucketAclReadModel {}

impl ManagedReadModel for S3BucketAclReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { acl, bucket } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("acl".to_string(), acl);
        attributes.insert("bucket".to_string(), bucket);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("s3.BucketAcl", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema
        })?;
        validate_read_attributes("s3.BucketAcl", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketOwnershipControls`.
pub(crate) struct S3BucketOwnershipControlsReadModel {
    pub(crate) bucket: Value,
    pub(crate) object_ownership: Value,
}

impl sealed::Sealed for S3BucketOwnershipControlsReadModel {}

impl ManagedReadModel for S3BucketOwnershipControlsReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            object_ownership,
        } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("object_ownership".to_string(), object_ownership);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketOwnershipControls",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_ownership_controls::s3_bucket_ownership_controls_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketOwnershipControls", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketServerSideEncryptionConfiguration`.
pub(crate) struct S3BucketServerSideEncryptionConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) rules: Value,
}

impl sealed::Sealed for S3BucketServerSideEncryptionConfigurationReadModel {}

impl ManagedReadModel for S3BucketServerSideEncryptionConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { bucket, rules } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("rules".to_string(), rules);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketServerSideEncryptionConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema
            },
        )?;
        validate_read_attributes(
            "s3.BucketServerSideEncryptionConfiguration",
            schema,
            &mut attributes,
        )?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketReplicationConfiguration`.
pub(crate) struct S3BucketReplicationConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) role: Value,
    pub(crate) rules: Value,
}

impl sealed::Sealed for S3BucketReplicationConfigurationReadModel {}

impl ManagedReadModel for S3BucketReplicationConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            role,
            rules,
        } = self;
        let mut attributes = HashMap::with_capacity(3);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("role".to_string(), role);
        attributes.insert("rules".to_string(), rules);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketReplicationConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_replication_configuration::s3_bucket_replication_configuration_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketReplicationConfiguration", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketLifecycleConfiguration`.
pub(crate) struct S3BucketLifecycleConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) rules: Value,
}

impl sealed::Sealed for S3BucketLifecycleConfigurationReadModel {}

impl ManagedReadModel for S3BucketLifecycleConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { bucket, rules } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("rules".to_string(), rules);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketLifecycleConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketLifecycleConfiguration", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketWebsiteConfiguration`.
pub(crate) struct S3BucketWebsiteConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) index_document: Option<Value>,
    pub(crate) error_document: Option<Value>,
    pub(crate) redirect_all_requests_to: Option<Value>,
}

impl sealed::Sealed for S3BucketWebsiteConfigurationReadModel {}

impl ManagedReadModel for S3BucketWebsiteConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            index_document,
            error_document,
            redirect_all_requests_to,
        } = self;
        let mut attributes = HashMap::with_capacity(4);
        attributes.insert("bucket".to_string(), bucket);
        if let Some(value) = index_document {
            attributes.insert("index_document".to_string(), value);
        }
        if let Some(value) = error_document {
            attributes.insert("error_document".to_string(), value);
        }
        if let Some(value) = redirect_all_requests_to {
            attributes.insert("redirect_all_requests_to".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketWebsiteConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_website_configuration::s3_bucket_website_configuration_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketWebsiteConfiguration", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketCorsConfiguration`.
pub(crate) struct S3BucketCorsConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) cors_rules: Value,
}

impl sealed::Sealed for S3BucketCorsConfigurationReadModel {}

impl ManagedReadModel for S3BucketCorsConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self { bucket, cors_rules } = self;
        let mut attributes = HashMap::with_capacity(2);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("cors_rules".to_string(), cors_rules);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketCorsConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_cors_configuration::s3_bucket_cors_configuration_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketCorsConfiguration", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketNotificationConfiguration`.
pub(crate) struct S3BucketNotificationConfigurationReadModel {
    pub(crate) bucket: Value,
    pub(crate) topic_configurations: Value,
    pub(crate) queue_configurations: Value,
    pub(crate) lambda_function_configurations: Value,
    pub(crate) event_bridge_configuration: Option<Value>,
}

impl sealed::Sealed for S3BucketNotificationConfigurationReadModel {}

impl ManagedReadModel for S3BucketNotificationConfigurationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            topic_configurations,
            queue_configurations,
            lambda_function_configurations,
            event_bridge_configuration,
        } = self;
        let mut attributes = HashMap::with_capacity(5);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("topic_configurations".to_string(), topic_configurations);
        attributes.insert("queue_configurations".to_string(), queue_configurations);
        attributes.insert(
            "lambda_function_configurations".to_string(),
            lambda_function_configurations,
        );
        if let Some(value) = event_bridge_configuration {
            attributes.insert("event_bridge_configuration".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketNotificationConfiguration",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_notification_configuration::s3_bucket_notification_configuration_config().schema
            },
        )?;
        validate_read_attributes(
            "s3.BucketNotificationConfiguration",
            schema,
            &mut attributes,
        )?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketLogging`.
pub(crate) struct S3BucketLoggingReadModel {
    pub(crate) bucket: Value,
    pub(crate) target_bucket: Value,
    pub(crate) target_prefix: Value,
    pub(crate) target_object_key_format: Option<Value>,
}

impl sealed::Sealed for S3BucketLoggingReadModel {}

impl ManagedReadModel for S3BucketLoggingReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            target_bucket,
            target_prefix,
            target_object_key_format,
        } = self;
        let mut attributes = HashMap::with_capacity(4);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("target_bucket".to_string(), target_bucket);
        attributes.insert("target_prefix".to_string(), target_prefix);
        if let Some(value) = target_object_key_format {
            attributes.insert("target_object_key_format".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("s3.BucketLogging", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::s3::bucket_logging::s3_bucket_logging_config().schema
        })?;
        validate_read_attributes("s3.BucketLogging", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `s3.BucketPublicAccessBlock`.
pub(crate) struct S3BucketPublicAccessBlockReadModel {
    pub(crate) bucket: Value,
    pub(crate) block_public_acls: Value,
    pub(crate) ignore_public_acls: Value,
    pub(crate) block_public_policy: Value,
    pub(crate) restrict_public_buckets: Value,
}

impl sealed::Sealed for S3BucketPublicAccessBlockReadModel {}

impl ManagedReadModel for S3BucketPublicAccessBlockReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            bucket,
            block_public_acls,
            ignore_public_acls,
            block_public_policy,
            restrict_public_buckets,
        } = self;
        let mut attributes = HashMap::with_capacity(5);
        attributes.insert("bucket".to_string(), bucket);
        attributes.insert("block_public_acls".to_string(), block_public_acls);
        attributes.insert("ignore_public_acls".to_string(), ignore_public_acls);
        attributes.insert("block_public_policy".to_string(), block_public_policy);
        attributes.insert(
            "restrict_public_buckets".to_string(),
            restrict_public_buckets,
        );
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "s3.BucketPublicAccessBlock",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::s3::bucket_public_access_block::s3_bucket_public_access_block_config().schema
            },
        )?;
        validate_read_attributes("s3.BucketPublicAccessBlock", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `organizations.Organization`.
pub(crate) struct OrganizationsOrganizationReadModel {
    pub(crate) feature_set: Option<Value>,
    pub(crate) arn: Option<Value>,
    pub(crate) id: Option<Value>,
    pub(crate) master_account_arn: Option<Value>,
    pub(crate) master_account_email: Option<Value>,
    pub(crate) master_account_id: Option<Value>,
}

/// Canonical fields for `organizations.Organization` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct OrganizationsOrganizationReadModelSupplement {}

impl sealed::Sealed for OrganizationsOrganizationReadModel {}

impl ManagedReadModel for OrganizationsOrganizationReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            feature_set,
            arn,
            id,
            master_account_arn,
            master_account_email,
            master_account_id,
        } = self;
        let mut attributes = HashMap::with_capacity(6);
        if let Some(value) = feature_set {
            attributes.insert("feature_set".to_string(), value);
        }
        if let Some(value) = arn {
            attributes.insert("arn".to_string(), value);
        }
        if let Some(value) = id {
            attributes.insert("id".to_string(), value);
        }
        if let Some(value) = master_account_arn {
            attributes.insert("master_account_arn".to_string(), value);
        }
        if let Some(value) = master_account_email {
            attributes.insert("master_account_email".to_string(), value);
        }
        if let Some(value) = master_account_id {
            attributes.insert("master_account_id".to_string(), value);
        }
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema(
            "organizations.Organization",
            &VALIDATED_SCHEMA,
            || {
                crate::schemas::generated::organizations::organization::organizations_organization_config().schema
            },
        )?;
        validate_read_attributes("organizations.Organization", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `organizations.Account`.
pub(crate) struct OrganizationsAccountReadModel {
    pub(crate) account_name: Value,
    pub(crate) email: Value,
    pub(crate) arn: Option<Value>,
    pub(crate) id: Option<Value>,
    pub(crate) joined_method: Option<Value>,
    pub(crate) joined_timestamp: Option<Value>,
    pub(crate) status: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `organizations.Account` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct OrganizationsAccountReadModelSupplement {
    pub(crate) joined_timestamp: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for OrganizationsAccountReadModel {}

impl ManagedReadModel for OrganizationsAccountReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            account_name,
            email,
            arn,
            id,
            joined_method,
            joined_timestamp,
            status,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(8);
        attributes.insert("account_name".to_string(), account_name);
        attributes.insert("email".to_string(), email);
        if let Some(value) = arn {
            attributes.insert("arn".to_string(), value);
        }
        if let Some(value) = id {
            attributes.insert("id".to_string(), value);
        }
        if let Some(value) = joined_method {
            attributes.insert("joined_method".to_string(), value);
        }
        if let Some(value) = joined_timestamp {
            attributes.insert("joined_timestamp".to_string(), value);
        }
        if let Some(value) = status {
            attributes.insert("status".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema =
            cached_validated_read_schema("organizations.Account", &VALIDATED_SCHEMA, || {
                crate::schemas::generated::organizations::account::organizations_account_config()
                    .schema
            })?;
        validate_read_attributes("organizations.Account", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `route53.RecordSet`.
pub(crate) struct Route53RecordSetReadModel {
    pub(crate) alias_target: Option<Value>,
    pub(crate) name: Value,
    pub(crate) resource_records: Option<Value>,
    pub(crate) ttl: Option<Value>,
    pub(crate) r#type: Value,
    pub(crate) hosted_zone_id: Value,
}

/// Canonical fields for `route53.RecordSet` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct Route53RecordSetReadModelSupplement {
    pub(crate) alias_target: Option<Value>,
    pub(crate) resource_records: Option<Value>,
    pub(crate) hosted_zone_id: Value,
}

impl sealed::Sealed for Route53RecordSetReadModel {}

impl ManagedReadModel for Route53RecordSetReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            alias_target,
            name,
            resource_records,
            ttl,
            r#type,
            hosted_zone_id,
        } = self;
        let mut attributes = HashMap::with_capacity(6);
        if let Some(value) = alias_target {
            attributes.insert("alias_target".to_string(), value);
        }
        attributes.insert("name".to_string(), name);
        if let Some(value) = resource_records {
            attributes.insert("resource_records".to_string(), value);
        }
        if let Some(value) = ttl {
            attributes.insert("ttl".to_string(), value);
        }
        attributes.insert("type".to_string(), r#type);
        attributes.insert("hosted_zone_id".to_string(), hosted_zone_id);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("route53.RecordSet", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::route53::record_set::route53_record_set_config().schema
        })?;
        validate_read_attributes("route53.RecordSet", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `iam.Role`.
pub(crate) struct IamRoleReadModel {
    pub(crate) assume_role_policy_document: Value,
    pub(crate) description: Option<Value>,
    pub(crate) max_session_duration: Option<Value>,
    pub(crate) path: Option<Value>,
    pub(crate) role_name: Value,
    pub(crate) arn: Option<Value>,
    pub(crate) role_id: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `iam.Role` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct IamRoleReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for IamRoleReadModel {}

impl ManagedReadModel for IamRoleReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_name,
            arn,
            role_id,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(8);
        attributes.insert(
            "assume_role_policy_document".to_string(),
            assume_role_policy_document,
        );
        if let Some(value) = description {
            attributes.insert("description".to_string(), value);
        }
        if let Some(value) = max_session_duration {
            attributes.insert("max_session_duration".to_string(), value);
        }
        if let Some(value) = path {
            attributes.insert("path".to_string(), value);
        }
        attributes.insert("role_name".to_string(), role_name);
        if let Some(value) = arn {
            attributes.insert("arn".to_string(), value);
        }
        if let Some(value) = role_id {
            attributes.insert("role_id".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("iam.Role", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::iam::role::iam_role_config().schema
        })?;
        validate_read_attributes("iam.Role", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `logs.LogGroup`.
pub(crate) struct LogsLogGroupReadModel {
    pub(crate) deletion_protection_enabled: Value,
    pub(crate) kms_key_id: Option<Value>,
    pub(crate) log_group_class: Option<Value>,
    pub(crate) log_group_name: Value,
    pub(crate) retention_in_days: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `logs.LogGroup` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct LogsLogGroupReadModelSupplement {
    pub(crate) tags: Value,
}

impl sealed::Sealed for LogsLogGroupReadModel {}

impl ManagedReadModel for LogsLogGroupReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(6);
        attributes.insert(
            "deletion_protection_enabled".to_string(),
            deletion_protection_enabled,
        );
        if let Some(value) = kms_key_id {
            attributes.insert("kms_key_id".to_string(), value);
        }
        if let Some(value) = log_group_class {
            attributes.insert("log_group_class".to_string(), value);
        }
        attributes.insert("log_group_name".to_string(), log_group_name);
        if let Some(value) = retention_in_days {
            attributes.insert("retention_in_days".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("logs.LogGroup", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::logs::log_group::logs_log_group_config().schema
        })?;
        validate_read_attributes("logs.LogGroup", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `acm.Certificate`.
pub(crate) struct AcmCertificateReadModel {
    pub(crate) domain_name: Value,
    pub(crate) key_algorithm: Option<Value>,
    pub(crate) options: Option<Value>,
    pub(crate) subject_alternative_names: Value,
    pub(crate) validation_method: Option<Value>,
    pub(crate) certificate_arn: Option<Value>,
    pub(crate) domain_validation_options: Option<Value>,
    pub(crate) renewal_eligibility: Option<Value>,
    pub(crate) renewal_summary: Option<Value>,
    pub(crate) status: Option<Value>,
    pub(crate) r#type: Option<Value>,
    pub(crate) tags: Value,
}

/// Canonical fields for `acm.Certificate` supplied by identifiers, side reads, or complex SDK projections.
pub(crate) struct AcmCertificateReadModelSupplement {
    pub(crate) options: Option<Value>,
    pub(crate) validation_method: Option<Value>,
    pub(crate) domain_validation_options: Option<Value>,
    pub(crate) renewal_summary: Option<Value>,
    pub(crate) tags: Value,
}

impl sealed::Sealed for AcmCertificateReadModel {}

impl ManagedReadModel for AcmCertificateReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            domain_name,
            key_algorithm,
            options,
            subject_alternative_names,
            validation_method,
            certificate_arn,
            domain_validation_options,
            renewal_eligibility,
            renewal_summary,
            status,
            r#type,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(12);
        attributes.insert("domain_name".to_string(), domain_name);
        if let Some(value) = key_algorithm {
            attributes.insert("key_algorithm".to_string(), value);
        }
        if let Some(value) = options {
            attributes.insert("options".to_string(), value);
        }
        attributes.insert(
            "subject_alternative_names".to_string(),
            subject_alternative_names,
        );
        if let Some(value) = validation_method {
            attributes.insert("validation_method".to_string(), value);
        }
        if let Some(value) = certificate_arn {
            attributes.insert("certificate_arn".to_string(), value);
        }
        if let Some(value) = domain_validation_options {
            attributes.insert("domain_validation_options".to_string(), value);
        }
        if let Some(value) = renewal_eligibility {
            attributes.insert("renewal_eligibility".to_string(), value);
        }
        if let Some(value) = renewal_summary {
            attributes.insert("renewal_summary".to_string(), value);
        }
        if let Some(value) = status {
            attributes.insert("status".to_string(), value);
        }
        if let Some(value) = r#type {
            attributes.insert("type".to_string(), value);
        }
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("acm.Certificate", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::acm::certificate::acm_certificate_config().schema
        })?;
        validate_read_attributes("acm.Certificate", schema, &mut attributes)?;
        Ok(attributes)
    }
}

/// Exhaustive canonical read state for `sqs.Queue`.
pub(crate) struct SqsQueueReadModel {
    pub(crate) queue_name: Value,
    pub(crate) policy: Option<Value>,
    pub(crate) visibility_timeout: Value,
    pub(crate) message_retention_period: Value,
    pub(crate) delay_seconds: Value,
    pub(crate) maximum_message_size: Value,
    pub(crate) receive_message_wait_time_seconds: Value,
    pub(crate) redrive_policy: Option<Value>,
    pub(crate) redrive_allow_policy: Option<Value>,
    pub(crate) fifo_queue: Value,
    pub(crate) content_based_deduplication: Value,
    pub(crate) deduplication_scope: Option<Value>,
    pub(crate) fifo_throughput_limit: Option<Value>,
    pub(crate) kms_master_key_id: Option<Value>,
    pub(crate) kms_data_key_reuse_period_seconds: Option<Value>,
    pub(crate) sqs_managed_sse_enabled: Option<Value>,
    pub(crate) queue_arn: Value,
    pub(crate) approximate_number_of_messages: Option<Value>,
    pub(crate) approximate_number_of_messages_delayed: Option<Value>,
    pub(crate) approximate_number_of_messages_not_visible: Option<Value>,
    pub(crate) created_timestamp: Value,
    pub(crate) last_modified_timestamp: Value,
    pub(crate) tags: Value,
}

impl sealed::Sealed for SqsQueueReadModel {}

impl ManagedReadModel for SqsQueueReadModel {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {
        let Self {
            queue_name,
            policy,
            visibility_timeout,
            message_retention_period,
            delay_seconds,
            maximum_message_size,
            receive_message_wait_time_seconds,
            redrive_policy,
            redrive_allow_policy,
            fifo_queue,
            content_based_deduplication,
            deduplication_scope,
            fifo_throughput_limit,
            kms_master_key_id,
            kms_data_key_reuse_period_seconds,
            sqs_managed_sse_enabled,
            queue_arn,
            approximate_number_of_messages,
            approximate_number_of_messages_delayed,
            approximate_number_of_messages_not_visible,
            created_timestamp,
            last_modified_timestamp,
            tags,
        } = self;
        let mut attributes = HashMap::with_capacity(23);
        attributes.insert("queue_name".to_string(), queue_name);
        if let Some(value) = policy {
            attributes.insert("policy".to_string(), value);
        }
        attributes.insert("visibility_timeout".to_string(), visibility_timeout);
        attributes.insert(
            "message_retention_period".to_string(),
            message_retention_period,
        );
        attributes.insert("delay_seconds".to_string(), delay_seconds);
        attributes.insert("maximum_message_size".to_string(), maximum_message_size);
        attributes.insert(
            "receive_message_wait_time_seconds".to_string(),
            receive_message_wait_time_seconds,
        );
        if let Some(value) = redrive_policy {
            attributes.insert("redrive_policy".to_string(), value);
        }
        if let Some(value) = redrive_allow_policy {
            attributes.insert("redrive_allow_policy".to_string(), value);
        }
        attributes.insert("fifo_queue".to_string(), fifo_queue);
        attributes.insert(
            "content_based_deduplication".to_string(),
            content_based_deduplication,
        );
        if let Some(value) = deduplication_scope {
            attributes.insert("deduplication_scope".to_string(), value);
        }
        if let Some(value) = fifo_throughput_limit {
            attributes.insert("fifo_throughput_limit".to_string(), value);
        }
        if let Some(value) = kms_master_key_id {
            attributes.insert("kms_master_key_id".to_string(), value);
        }
        if let Some(value) = kms_data_key_reuse_period_seconds {
            attributes.insert("kms_data_key_reuse_period_seconds".to_string(), value);
        }
        if let Some(value) = sqs_managed_sse_enabled {
            attributes.insert("sqs_managed_sse_enabled".to_string(), value);
        }
        attributes.insert("queue_arn".to_string(), queue_arn);
        if let Some(value) = approximate_number_of_messages {
            attributes.insert("approximate_number_of_messages".to_string(), value);
        }
        if let Some(value) = approximate_number_of_messages_delayed {
            attributes.insert("approximate_number_of_messages_delayed".to_string(), value);
        }
        if let Some(value) = approximate_number_of_messages_not_visible {
            attributes.insert(
                "approximate_number_of_messages_not_visible".to_string(),
                value,
            );
        }
        attributes.insert("created_timestamp".to_string(), created_timestamp);
        attributes.insert(
            "last_modified_timestamp".to_string(),
            last_modified_timestamp,
        );
        attributes.insert("tags".to_string(), tags);
        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> =
            OnceLock::new();
        let schema = cached_validated_read_schema("sqs.Queue", &VALIDATED_SCHEMA, || {
            crate::schemas::generated::sqs::queue::sqs_queue_config().schema
        })?;
        validate_read_attributes("sqs.Queue", schema, &mut attributes)?;
        Ok(attributes)
    }
}
