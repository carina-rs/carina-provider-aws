//! Auto-generated exhaustive managed-resource send inputs.
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./scripts/generate-provider.sh

use std::collections::BTreeSet;

use carina_core::provider::{ProviderResult, Sent};
use carina_core::resource::{Resource, Value};

/// Exhaustive typed create input for `ec2.Vpc`.
pub(crate) struct Ec2VpcCreateInput {
    cidr_block: Option<String>,
    enable_dns_hostnames: Option<bool>,
    enable_dns_support: Option<bool>,
    instance_tenancy: Option<String>,
    ipv4_ipam_pool_id: Option<String>,
    ipv4_netmask_length: Option<i64>,
    tags: Option<Value>,
}

impl Ec2VpcCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc::ec2_vpc_config().schema;
        Ok(Self {
            cidr_block: crate::send_model::optional_string(_resource, "cidr_block")?,
            enable_dns_hostnames: crate::send_model::optional_bool(
                _resource,
                "enable_dns_hostnames",
            )?,
            enable_dns_support: crate::send_model::optional_bool(_resource, "enable_dns_support")?,
            instance_tenancy: crate::send_model::optional_enum(
                _resource,
                _schema,
                "instance_tenancy",
            )?,
            ipv4_ipam_pool_id: crate::send_model::optional_string(_resource, "ipv4_ipam_pool_id")?,
            ipv4_netmask_length: crate::send_model::optional_int(_resource, "ipv4_netmask_length")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.cidr_block.is_some() {
            sent.insert("cidr_block".to_string());
        }
        if self.enable_dns_hostnames.is_some() {
            sent.insert("enable_dns_hostnames".to_string());
        }
        if self.enable_dns_support.is_some() {
            sent.insert("enable_dns_support".to_string());
        }
        if self.instance_tenancy.is_some() {
            sent.insert("instance_tenancy".to_string());
        }
        if self.ipv4_ipam_pool_id.is_some() {
            sent.insert("ipv4_ipam_pool_id".to_string());
        }
        if self.ipv4_netmask_length.is_some() {
            sent.insert("ipv4_netmask_length".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<bool>,
        Option<bool>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<Value>,
    ) {
        let Self {
            cidr_block,
            enable_dns_hostnames,
            enable_dns_support,
            instance_tenancy,
            ipv4_ipam_pool_id,
            ipv4_netmask_length,
            tags,
        } = self;
        (
            cidr_block,
            enable_dns_hostnames,
            enable_dns_support,
            instance_tenancy,
            ipv4_ipam_pool_id,
            ipv4_netmask_length,
            tags,
        )
    }
}

/// Exhaustive typed update input for `ec2.Vpc`.
pub(crate) struct Ec2VpcUpdateInput {
    enable_dns_hostnames: Option<bool>,
    enable_dns_support: Option<bool>,
    tags: Option<Value>,
}

impl Ec2VpcUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc::ec2_vpc_config().schema;
        Ok(Self {
            enable_dns_hostnames: crate::send_model::optional_bool(
                _resource,
                "enable_dns_hostnames",
            )?,
            enable_dns_support: crate::send_model::optional_bool(_resource, "enable_dns_support")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<bool>, Option<bool>, Option<Value>) {
        let Self {
            enable_dns_hostnames,
            enable_dns_support,
            tags,
        } = self;
        (enable_dns_hostnames, enable_dns_support, tags)
    }
}

/// Exhaustive typed create input for `ec2.Subnet`.
pub(crate) struct Ec2SubnetCreateInput {
    assign_ipv6_address_on_creation: Option<bool>,
    availability_zone: Option<String>,
    availability_zone_id: Option<String>,
    cidr_block: Option<String>,
    enable_dns64: Option<bool>,
    enable_lni_at_device_index: Option<i64>,
    ipv4_ipam_pool_id: Option<String>,
    ipv4_netmask_length: Option<i64>,
    ipv6_cidr_block: Option<String>,
    ipv6_ipam_pool_id: Option<String>,
    ipv6_native: Option<bool>,
    ipv6_netmask_length: Option<i64>,
    map_public_ip_on_launch: Option<bool>,
    outpost_arn: Option<String>,
    private_dns_name_options_on_launch: Option<Value>,
    tags: Option<Value>,
    vpc_id: String,
}

impl Ec2SubnetCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet::ec2_subnet_config().schema;
        Ok(Self {
            assign_ipv6_address_on_creation: crate::send_model::optional_bool(
                _resource,
                "assign_ipv6_address_on_creation",
            )?,
            availability_zone: crate::send_model::optional_enum(
                _resource,
                _schema,
                "availability_zone",
            )?,
            availability_zone_id: crate::send_model::optional_string(
                _resource,
                "availability_zone_id",
            )?,
            cidr_block: crate::send_model::optional_string(_resource, "cidr_block")?,
            enable_dns64: crate::send_model::optional_bool(_resource, "enable_dns64")?,
            enable_lni_at_device_index: crate::send_model::optional_int(
                _resource,
                "enable_lni_at_device_index",
            )?,
            ipv4_ipam_pool_id: crate::send_model::optional_string(_resource, "ipv4_ipam_pool_id")?,
            ipv4_netmask_length: crate::send_model::optional_int(_resource, "ipv4_netmask_length")?,
            ipv6_cidr_block: crate::send_model::optional_string(_resource, "ipv6_cidr_block")?,
            ipv6_ipam_pool_id: crate::send_model::optional_string(_resource, "ipv6_ipam_pool_id")?,
            ipv6_native: crate::send_model::optional_bool(_resource, "ipv6_native")?,
            ipv6_netmask_length: crate::send_model::optional_int(_resource, "ipv6_netmask_length")?,
            map_public_ip_on_launch: crate::send_model::optional_bool(
                _resource,
                "map_public_ip_on_launch",
            )?,
            outpost_arn: crate::send_model::optional_string(_resource, "outpost_arn")?,
            private_dns_name_options_on_launch: crate::send_model::optional_value(
                _resource,
                "private_dns_name_options_on_launch",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.assign_ipv6_address_on_creation.is_some() {
            sent.insert("assign_ipv6_address_on_creation".to_string());
        }
        if self.availability_zone.is_some() {
            sent.insert("availability_zone".to_string());
        }
        if self.availability_zone_id.is_some() {
            sent.insert("availability_zone_id".to_string());
        }
        if self.cidr_block.is_some() {
            sent.insert("cidr_block".to_string());
        }
        if self.enable_dns64.is_some() {
            sent.insert("enable_dns64".to_string());
        }
        if self.enable_lni_at_device_index.is_some() {
            sent.insert("enable_lni_at_device_index".to_string());
        }
        if self.ipv4_ipam_pool_id.is_some() {
            sent.insert("ipv4_ipam_pool_id".to_string());
        }
        if self.ipv4_netmask_length.is_some() {
            sent.insert("ipv4_netmask_length".to_string());
        }
        if self.ipv6_cidr_block.is_some() {
            sent.insert("ipv6_cidr_block".to_string());
        }
        if self.ipv6_ipam_pool_id.is_some() {
            sent.insert("ipv6_ipam_pool_id".to_string());
        }
        if self.ipv6_native.is_some() {
            sent.insert("ipv6_native".to_string());
        }
        if self.ipv6_netmask_length.is_some() {
            sent.insert("ipv6_netmask_length".to_string());
        }
        if self.map_public_ip_on_launch.is_some() {
            sent.insert("map_public_ip_on_launch".to_string());
        }
        if self.outpost_arn.is_some() {
            sent.insert("outpost_arn".to_string());
        }
        if self.private_dns_name_options_on_launch.is_some() {
            sent.insert("private_dns_name_options_on_launch".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<bool>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<bool>,
        Option<i64>,
        Option<bool>,
        Option<String>,
        Option<Value>,
        Option<Value>,
        String,
    ) {
        let Self {
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
        } = self;
        (
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
        )
    }
}

/// Exhaustive typed update input for `ec2.Subnet`.
pub(crate) struct Ec2SubnetUpdateInput {
    assign_ipv6_address_on_creation: Option<bool>,
    enable_dns64: Option<bool>,
    enable_lni_at_device_index: Option<i64>,
    map_public_ip_on_launch: Option<bool>,
    private_dns_name_options_on_launch: Option<Value>,
    tags: Option<Value>,
}

impl Ec2SubnetUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet::ec2_subnet_config().schema;
        Ok(Self {
            assign_ipv6_address_on_creation: crate::send_model::optional_bool(
                _resource,
                "assign_ipv6_address_on_creation",
            )?,
            enable_dns64: crate::send_model::optional_bool(_resource, "enable_dns64")?,
            enable_lni_at_device_index: crate::send_model::optional_int(
                _resource,
                "enable_lni_at_device_index",
            )?,
            map_public_ip_on_launch: crate::send_model::optional_bool(
                _resource,
                "map_public_ip_on_launch",
            )?,
            private_dns_name_options_on_launch: crate::send_model::optional_value(
                _resource,
                "private_dns_name_options_on_launch",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<bool>,
        Option<i64>,
        Option<bool>,
        Option<Value>,
        Option<Value>,
    ) {
        let Self {
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index,
            map_public_ip_on_launch,
            private_dns_name_options_on_launch,
            tags,
        } = self;
        (
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index,
            map_public_ip_on_launch,
            private_dns_name_options_on_launch,
            tags,
        )
    }
}

/// Exhaustive typed create input for `ec2.InternetGateway`.
pub(crate) struct Ec2InternetGatewayCreateInput {
    tags: Option<Value>,
    vpc_id: Option<String>,
}

impl Ec2InternetGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::internet_gateway::ec2_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.vpc_id.is_some() {
            sent.insert("vpc_id".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, Option<String>) {
        let Self { tags, vpc_id } = self;
        (tags, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.InternetGateway`.
pub(crate) struct Ec2InternetGatewayUpdateInput {
    tags: Option<Value>,
}

impl Ec2InternetGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::internet_gateway::ec2_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.RouteTable`.
pub(crate) struct Ec2RouteTableCreateInput {
    tags: Option<Value>,
    vpc_id: String,
}

impl Ec2RouteTableCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route_table::ec2_route_table_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, String) {
        let Self { tags, vpc_id } = self;
        (tags, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.RouteTable`.
pub(crate) struct Ec2RouteTableUpdateInput {
    tags: Option<Value>,
}

impl Ec2RouteTableUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route_table::ec2_route_table_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.Route`.
pub(crate) struct Ec2RouteCreateInput {
    destination_cidr_block: Option<String>,
    gateway_id: Option<String>,
    nat_gateway_id: Option<String>,
    route_table_id: String,
}

impl Ec2RouteCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route::ec2_route_config().schema;
        Ok(Self {
            destination_cidr_block: crate::send_model::optional_string(
                _resource,
                "destination_cidr_block",
            )?,
            gateway_id: crate::send_model::optional_string(_resource, "gateway_id")?,
            nat_gateway_id: crate::send_model::optional_string(_resource, "nat_gateway_id")?,
            route_table_id: crate::send_model::required_string(_resource, "route_table_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.destination_cidr_block.is_some() {
            sent.insert("destination_cidr_block".to_string());
        }
        if self.gateway_id.is_some() {
            sent.insert("gateway_id".to_string());
        }
        if self.nat_gateway_id.is_some() {
            sent.insert("nat_gateway_id".to_string());
        }
        sent.insert("route_table_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, Option<String>, Option<String>, String) {
        let Self {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            route_table_id,
        } = self;
        (
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            route_table_id,
        )
    }
}

/// Exhaustive typed update input for `ec2.Route`.
pub(crate) struct Ec2RouteUpdateInput {
    gateway_id: Option<String>,
    nat_gateway_id: Option<String>,
}

impl Ec2RouteUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route::ec2_route_config().schema;
        Ok(Self {
            gateway_id: crate::send_model::optional_string(_resource, "gateway_id")?,
            nat_gateway_id: crate::send_model::optional_string(_resource, "nat_gateway_id")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, Option<String>) {
        let Self {
            gateway_id,
            nat_gateway_id,
        } = self;
        (gateway_id, nat_gateway_id)
    }
}

/// Exhaustive typed create input for `ec2.SecurityGroup`.
pub(crate) struct Ec2SecurityGroupCreateInput {
    description: String,
    group_name: String,
    tags: Option<Value>,
    vpc_id: Option<String>,
}

impl Ec2SecurityGroupCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::security_group::ec2_security_group_config().schema;
        Ok(Self {
            description: crate::send_model::required_string(_resource, "description")?,
            group_name: crate::send_model::required_string(_resource, "group_name")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("description".to_string());
        sent.insert("group_name".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.vpc_id.is_some() {
            sent.insert("vpc_id".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String, Option<Value>, Option<String>) {
        let Self {
            description,
            group_name,
            tags,
            vpc_id,
        } = self;
        (description, group_name, tags, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroup`.
pub(crate) struct Ec2SecurityGroupUpdateInput {
    tags: Option<Value>,
}

impl Ec2SecurityGroupUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::security_group::ec2_security_group_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.SecurityGroupIngress`.
pub(crate) struct Ec2SecurityGroupIngressCreateInput {
    cidr_ip: Option<String>,
    cidr_ipv6: Option<String>,
    description: Option<String>,
    from_port: Option<i64>,
    group_id: Option<String>,
    ip_protocol: String,
    source_prefix_list_id: Option<String>,
    source_security_group_id: Option<String>,
    source_security_group_owner_id: Option<String>,
    to_port: Option<i64>,
}

impl Ec2SecurityGroupIngressCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config().schema;
        Ok(Self {
            cidr_ip: crate::send_model::optional_string(_resource, "cidr_ip")?,
            cidr_ipv6: crate::send_model::optional_string(_resource, "cidr_ipv6")?,
            description: crate::send_model::optional_string(_resource, "description")?,
            from_port: crate::send_model::optional_int(_resource, "from_port")?,
            group_id: crate::send_model::optional_string(_resource, "group_id")?,
            ip_protocol: crate::send_model::required_enum(_resource, _schema, "ip_protocol")?,
            source_prefix_list_id: crate::send_model::optional_string(
                _resource,
                "source_prefix_list_id",
            )?,
            source_security_group_id: crate::send_model::optional_string(
                _resource,
                "source_security_group_id",
            )?,
            source_security_group_owner_id: crate::send_model::optional_string(
                _resource,
                "source_security_group_owner_id",
            )?,
            to_port: crate::send_model::optional_int(_resource, "to_port")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.cidr_ip.is_some() {
            sent.insert("cidr_ip".to_string());
        }
        if self.cidr_ipv6.is_some() {
            sent.insert("cidr_ipv6".to_string());
        }
        if self.description.is_some() {
            sent.insert("description".to_string());
        }
        if self.from_port.is_some() {
            sent.insert("from_port".to_string());
        }
        if self.group_id.is_some() {
            sent.insert("group_id".to_string());
        }
        sent.insert("ip_protocol".to_string());
        if self.source_prefix_list_id.is_some() {
            sent.insert("source_prefix_list_id".to_string());
        }
        if self.source_security_group_id.is_some() {
            sent.insert("source_security_group_id".to_string());
        }
        if self.source_security_group_owner_id.is_some() {
            sent.insert("source_security_group_owner_id".to_string());
        }
        if self.to_port.is_some() {
            sent.insert("to_port".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) {
        let Self {
            cidr_ip,
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            source_prefix_list_id,
            source_security_group_id,
            source_security_group_owner_id,
            to_port,
        } = self;
        (
            cidr_ip,
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            source_prefix_list_id,
            source_security_group_id,
            source_security_group_owner_id,
            to_port,
        )
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroupIngress`.
pub(crate) struct Ec2SecurityGroupIngressUpdateInput;

impl Ec2SecurityGroupIngressUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) {
        let Self = self;
    }
}

/// Exhaustive typed create input for `ec2.SecurityGroupEgress`.
pub(crate) struct Ec2SecurityGroupEgressCreateInput {
    cidr_ip: Option<String>,
    cidr_ipv6: Option<String>,
    description: Option<String>,
    destination_prefix_list_id: Option<String>,
    destination_security_group_id: Option<String>,
    from_port: Option<i64>,
    group_id: String,
    ip_protocol: String,
    source_security_group_owner_id: Option<String>,
    to_port: Option<i64>,
}

impl Ec2SecurityGroupEgressCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_egress::ec2_security_group_egress_config().schema;
        Ok(Self {
            cidr_ip: crate::send_model::optional_string(_resource, "cidr_ip")?,
            cidr_ipv6: crate::send_model::optional_string(_resource, "cidr_ipv6")?,
            description: crate::send_model::optional_string(_resource, "description")?,
            destination_prefix_list_id: crate::send_model::optional_string(
                _resource,
                "destination_prefix_list_id",
            )?,
            destination_security_group_id: crate::send_model::optional_string(
                _resource,
                "destination_security_group_id",
            )?,
            from_port: crate::send_model::optional_int(_resource, "from_port")?,
            group_id: crate::send_model::required_string(_resource, "group_id")?,
            ip_protocol: crate::send_model::required_enum(_resource, _schema, "ip_protocol")?,
            source_security_group_owner_id: crate::send_model::optional_string(
                _resource,
                "source_security_group_owner_id",
            )?,
            to_port: crate::send_model::optional_int(_resource, "to_port")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.cidr_ip.is_some() {
            sent.insert("cidr_ip".to_string());
        }
        if self.cidr_ipv6.is_some() {
            sent.insert("cidr_ipv6".to_string());
        }
        if self.description.is_some() {
            sent.insert("description".to_string());
        }
        if self.destination_prefix_list_id.is_some() {
            sent.insert("destination_prefix_list_id".to_string());
        }
        if self.destination_security_group_id.is_some() {
            sent.insert("destination_security_group_id".to_string());
        }
        if self.from_port.is_some() {
            sent.insert("from_port".to_string());
        }
        sent.insert("group_id".to_string());
        sent.insert("ip_protocol".to_string());
        if self.source_security_group_owner_id.is_some() {
            sent.insert("source_security_group_owner_id".to_string());
        }
        if self.to_port.is_some() {
            sent.insert("to_port".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        String,
        String,
        Option<String>,
        Option<i64>,
    ) {
        let Self {
            cidr_ip,
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            destination_security_group_id,
            from_port,
            group_id,
            ip_protocol,
            source_security_group_owner_id,
            to_port,
        } = self;
        (
            cidr_ip,
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            destination_security_group_id,
            from_port,
            group_id,
            ip_protocol,
            source_security_group_owner_id,
            to_port,
        )
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroupEgress`.
pub(crate) struct Ec2SecurityGroupEgressUpdateInput;

impl Ec2SecurityGroupEgressUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_egress::ec2_security_group_egress_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) {
        let Self = self;
    }
}

/// Exhaustive typed create input for `ec2.EgressOnlyInternetGateway`.
pub(crate) struct Ec2EgressOnlyInternetGatewayCreateInput {
    tags: Option<Value>,
    vpc_id: String,
}

impl Ec2EgressOnlyInternetGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, String) {
        let Self { tags, vpc_id } = self;
        (tags, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.EgressOnlyInternetGateway`.
pub(crate) struct Ec2EgressOnlyInternetGatewayUpdateInput {
    tags: Option<Value>,
}

impl Ec2EgressOnlyInternetGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.Eip`.
pub(crate) struct Ec2EipCreateInput {
    address: Option<String>,
    domain: Option<String>,
    public_ipv4_pool: Option<String>,
    tags: Option<Value>,
}

impl Ec2EipCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::eip::ec2_eip_config().schema;
        Ok(Self {
            address: crate::send_model::optional_string(_resource, "address")?,
            domain: crate::send_model::optional_enum(_resource, _schema, "domain")?,
            public_ipv4_pool: crate::send_model::optional_string(_resource, "public_ipv4_pool")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.address.is_some() {
            sent.insert("address".to_string());
        }
        if self.domain.is_some() {
            sent.insert("domain".to_string());
        }
        if self.public_ipv4_pool.is_some() {
            sent.insert("public_ipv4_pool".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Value>,
    ) {
        let Self {
            address,
            domain,
            public_ipv4_pool,
            tags,
        } = self;
        (address, domain, public_ipv4_pool, tags)
    }
}

/// Exhaustive typed update input for `ec2.Eip`.
pub(crate) struct Ec2EipUpdateInput {
    tags: Option<Value>,
}

impl Ec2EipUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::eip::ec2_eip_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.FlowLog`.
pub(crate) struct Ec2FlowLogCreateInput {
    deliver_logs_permission_arn: Option<String>,
    log_destination: Option<String>,
    log_destination_type: Option<String>,
    log_format: Option<String>,
    log_group_name: Option<String>,
    max_aggregation_interval: Option<i64>,
    resource_ids: Value,
    resource_type: String,
    tags: Option<Value>,
    traffic_type: Option<String>,
}

impl Ec2FlowLogCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::flow_log::ec2_flow_log_config().schema;
        Ok(Self {
            deliver_logs_permission_arn: crate::send_model::optional_string(
                _resource,
                "deliver_logs_permission_arn",
            )?,
            log_destination: crate::send_model::optional_string(_resource, "log_destination")?,
            log_destination_type: crate::send_model::optional_enum(
                _resource,
                _schema,
                "log_destination_type",
            )?,
            log_format: crate::send_model::optional_string(_resource, "log_format")?,
            log_group_name: crate::send_model::optional_string(_resource, "log_group_name")?,
            max_aggregation_interval: crate::send_model::optional_int(
                _resource,
                "max_aggregation_interval",
            )?,
            resource_ids: crate::send_model::required_value(_resource, "resource_ids")?,
            resource_type: crate::send_model::required_enum(_resource, _schema, "resource_type")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            traffic_type: crate::send_model::optional_enum(_resource, _schema, "traffic_type")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.deliver_logs_permission_arn.is_some() {
            sent.insert("deliver_logs_permission_arn".to_string());
        }
        if self.log_destination.is_some() {
            sent.insert("log_destination".to_string());
        }
        if self.log_destination_type.is_some() {
            sent.insert("log_destination_type".to_string());
        }
        if self.log_format.is_some() {
            sent.insert("log_format".to_string());
        }
        if self.log_group_name.is_some() {
            sent.insert("log_group_name".to_string());
        }
        if self.max_aggregation_interval.is_some() {
            sent.insert("max_aggregation_interval".to_string());
        }
        sent.insert("resource_ids".to_string());
        sent.insert("resource_type".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.traffic_type.is_some() {
            sent.insert("traffic_type".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Value,
        String,
        Option<Value>,
        Option<String>,
    ) {
        let Self {
            deliver_logs_permission_arn,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            resource_ids,
            resource_type,
            tags,
            traffic_type,
        } = self;
        (
            deliver_logs_permission_arn,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            resource_ids,
            resource_type,
            tags,
            traffic_type,
        )
    }
}

/// Exhaustive typed update input for `ec2.FlowLog`.
pub(crate) struct Ec2FlowLogUpdateInput {
    tags: Option<Value>,
}

impl Ec2FlowLogUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::flow_log::ec2_flow_log_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.NatGateway`.
pub(crate) struct Ec2NatGatewayCreateInput {
    allocation_id: Option<String>,
    availability_mode: Option<String>,
    availability_zone_addresses: Option<Value>,
    connectivity_type: Option<String>,
    private_ip_address: Option<String>,
    subnet_id: Option<String>,
    tags: Option<Value>,
    vpc_id: Option<String>,
}

impl Ec2NatGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema;
        Ok(Self {
            allocation_id: crate::send_model::optional_string(_resource, "allocation_id")?,
            availability_mode: crate::send_model::optional_enum(
                _resource,
                _schema,
                "availability_mode",
            )?,
            availability_zone_addresses: crate::send_model::optional_value(
                _resource,
                "availability_zone_addresses",
            )?,
            connectivity_type: crate::send_model::optional_enum(
                _resource,
                _schema,
                "connectivity_type",
            )?,
            private_ip_address: crate::send_model::optional_string(
                _resource,
                "private_ip_address",
            )?,
            subnet_id: crate::send_model::optional_string(_resource, "subnet_id")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.allocation_id.is_some() {
            sent.insert("allocation_id".to_string());
        }
        if self.availability_mode.is_some() {
            sent.insert("availability_mode".to_string());
        }
        if self.availability_zone_addresses.is_some() {
            sent.insert("availability_zone_addresses".to_string());
        }
        if self.connectivity_type.is_some() {
            sent.insert("connectivity_type".to_string());
        }
        if self.private_ip_address.is_some() {
            sent.insert("private_ip_address".to_string());
        }
        if self.subnet_id.is_some() {
            sent.insert("subnet_id".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.vpc_id.is_some() {
            sent.insert("vpc_id".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<Value>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Value>,
        Option<String>,
    ) {
        let Self {
            allocation_id,
            availability_mode,
            availability_zone_addresses,
            connectivity_type,
            private_ip_address,
            subnet_id,
            tags,
            vpc_id,
        } = self;
        (
            allocation_id,
            availability_mode,
            availability_zone_addresses,
            connectivity_type,
            private_ip_address,
            subnet_id,
            tags,
            vpc_id,
        )
    }
}

/// Exhaustive typed update input for `ec2.NatGateway`.
pub(crate) struct Ec2NatGatewayUpdateInput {
    tags: Option<Value>,
}

impl Ec2NatGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.SubnetRouteTableAssociation`.
pub(crate) struct Ec2SubnetRouteTableAssociationCreateInput {
    route_table_id: String,
    subnet_id: String,
}

impl Ec2SubnetRouteTableAssociationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet_route_table_association::ec2_subnet_route_table_association_config().schema;
        Ok(Self {
            route_table_id: crate::send_model::required_string(_resource, "route_table_id")?,
            subnet_id: crate::send_model::required_string(_resource, "subnet_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("route_table_id".to_string());
        sent.insert("subnet_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String) {
        let Self {
            route_table_id,
            subnet_id,
        } = self;
        (route_table_id, subnet_id)
    }
}

/// Exhaustive typed update input for `ec2.SubnetRouteTableAssociation`.
pub(crate) struct Ec2SubnetRouteTableAssociationUpdateInput;

impl Ec2SubnetRouteTableAssociationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet_route_table_association::ec2_subnet_route_table_association_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) {
        let Self = self;
    }
}

/// Exhaustive typed create input for `ec2.TransitGateway`.
pub(crate) struct Ec2TransitGatewayCreateInput {
    description: Option<String>,
    options: Option<Value>,
    tags: Option<Value>,
}

impl Ec2TransitGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema;
        Ok(Self {
            description: crate::send_model::optional_string(_resource, "description")?,
            options: crate::send_model::optional_value(_resource, "options")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.description.is_some() {
            sent.insert("description".to_string());
        }
        if self.options.is_some() {
            sent.insert("options".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, Option<Value>, Option<Value>) {
        let Self {
            description,
            options,
            tags,
        } = self;
        (description, options, tags)
    }
}

/// Exhaustive typed update input for `ec2.TransitGateway`.
pub(crate) struct Ec2TransitGatewayUpdateInput {
    description: Option<String>,
    tags: Option<Value>,
}

impl Ec2TransitGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema;
        Ok(Self {
            description: crate::send_model::optional_string(_resource, "description")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, Option<Value>) {
        let Self { description, tags } = self;
        (description, tags)
    }
}

/// Exhaustive typed create input for `ec2.TransitGatewayAttachment`.
pub(crate) struct Ec2TransitGatewayAttachmentCreateInput {
    options: Option<Value>,
    subnet_ids: Value,
    tags: Option<Value>,
    transit_gateway_id: String,
    vpc_id: String,
}

impl Ec2TransitGatewayAttachmentCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema;
        Ok(Self {
            options: crate::send_model::optional_value(_resource, "options")?,
            subnet_ids: crate::send_model::required_value(_resource, "subnet_ids")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            transit_gateway_id: crate::send_model::required_string(
                _resource,
                "transit_gateway_id",
            )?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.options.is_some() {
            sent.insert("options".to_string());
        }
        sent.insert("subnet_ids".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("transit_gateway_id".to_string());
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, Value, Option<Value>, String, String) {
        let Self {
            options,
            subnet_ids,
            tags,
            transit_gateway_id,
            vpc_id,
        } = self;
        (options, subnet_ids, tags, transit_gateway_id, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.TransitGatewayAttachment`.
pub(crate) struct Ec2TransitGatewayAttachmentUpdateInput {
    tags: Option<Value>,
}

impl Ec2TransitGatewayAttachmentUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.VpcEndpoint`.
pub(crate) struct Ec2VpcEndpointCreateInput {
    policy_document: Option<String>,
    private_dns_enabled: Option<bool>,
    resource_configuration_arn: Option<String>,
    route_table_ids: Option<Value>,
    security_group_ids: Option<Value>,
    service_name: String,
    service_network_arn: Option<String>,
    service_region: Option<String>,
    subnet_ids: Option<Value>,
    tags: Option<Value>,
    vpc_endpoint_type: Option<String>,
    vpc_id: String,
}

impl Ec2VpcEndpointCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::vpc_endpoint::ec2_vpc_endpoint_config().schema;
        Ok(Self {
            policy_document: crate::send_model::optional_string(_resource, "policy_document")?,
            private_dns_enabled: crate::send_model::optional_bool(
                _resource,
                "private_dns_enabled",
            )?,
            resource_configuration_arn: crate::send_model::optional_string(
                _resource,
                "resource_configuration_arn",
            )?,
            route_table_ids: crate::send_model::optional_value(_resource, "route_table_ids")?,
            security_group_ids: crate::send_model::optional_value(_resource, "security_group_ids")?,
            service_name: crate::send_model::required_string(_resource, "service_name")?,
            service_network_arn: crate::send_model::optional_string(
                _resource,
                "service_network_arn",
            )?,
            service_region: crate::send_model::optional_enum(_resource, _schema, "service_region")?,
            subnet_ids: crate::send_model::optional_value(_resource, "subnet_ids")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_endpoint_type: crate::send_model::optional_enum(
                _resource,
                _schema,
                "vpc_endpoint_type",
            )?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.policy_document.is_some() {
            sent.insert("policy_document".to_string());
        }
        if self.private_dns_enabled.is_some() {
            sent.insert("private_dns_enabled".to_string());
        }
        if self.resource_configuration_arn.is_some() {
            sent.insert("resource_configuration_arn".to_string());
        }
        if self.route_table_ids.is_some() {
            sent.insert("route_table_ids".to_string());
        }
        if self.security_group_ids.is_some() {
            sent.insert("security_group_ids".to_string());
        }
        sent.insert("service_name".to_string());
        if self.service_network_arn.is_some() {
            sent.insert("service_network_arn".to_string());
        }
        if self.service_region.is_some() {
            sent.insert("service_region".to_string());
        }
        if self.subnet_ids.is_some() {
            sent.insert("subnet_ids".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.vpc_endpoint_type.is_some() {
            sent.insert("vpc_endpoint_type".to_string());
        }
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<String>,
        Option<bool>,
        Option<String>,
        Option<Value>,
        Option<Value>,
        String,
        Option<String>,
        Option<String>,
        Option<Value>,
        Option<Value>,
        Option<String>,
        String,
    ) {
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
            tags,
            vpc_endpoint_type,
            vpc_id,
        } = self;
        (
            policy_document,
            private_dns_enabled,
            resource_configuration_arn,
            route_table_ids,
            security_group_ids,
            service_name,
            service_network_arn,
            service_region,
            subnet_ids,
            tags,
            vpc_endpoint_type,
            vpc_id,
        )
    }
}

/// Exhaustive typed update input for `ec2.VpcEndpoint`.
pub(crate) struct Ec2VpcEndpointUpdateInput {
    policy_document: Option<String>,
    private_dns_enabled: Option<bool>,
    tags: Option<Value>,
}

impl Ec2VpcEndpointUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::vpc_endpoint::ec2_vpc_endpoint_config().schema;
        Ok(Self {
            policy_document: crate::send_model::optional_string(_resource, "policy_document")?,
            private_dns_enabled: crate::send_model::optional_bool(
                _resource,
                "private_dns_enabled",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, Option<bool>, Option<Value>) {
        let Self {
            policy_document,
            private_dns_enabled,
            tags,
        } = self;
        (policy_document, private_dns_enabled, tags)
    }
}

/// Exhaustive typed create input for `ec2.VpcGatewayAttachment`.
pub(crate) struct Ec2VpcGatewayAttachmentCreateInput {
    internet_gateway_id: Option<String>,
    vpc_id: String,
    vpn_gateway_id: Option<String>,
}

impl Ec2VpcGatewayAttachmentCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config().schema;
        Ok(Self {
            internet_gateway_id: crate::send_model::optional_string(
                _resource,
                "internet_gateway_id",
            )?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
            vpn_gateway_id: crate::send_model::optional_string(_resource, "vpn_gateway_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.internet_gateway_id.is_some() {
            sent.insert("internet_gateway_id".to_string());
        }
        sent.insert("vpc_id".to_string());
        if self.vpn_gateway_id.is_some() {
            sent.insert("vpn_gateway_id".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, String, Option<String>) {
        let Self {
            internet_gateway_id,
            vpc_id,
            vpn_gateway_id,
        } = self;
        (internet_gateway_id, vpc_id, vpn_gateway_id)
    }
}

/// Exhaustive typed update input for `ec2.VpcGatewayAttachment`.
pub(crate) struct Ec2VpcGatewayAttachmentUpdateInput;

impl Ec2VpcGatewayAttachmentUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) {
        let Self = self;
    }
}

/// Exhaustive typed create input for `ec2.VpcPeeringConnection`.
pub(crate) struct Ec2VpcPeeringConnectionCreateInput {
    peer_owner_id: Option<String>,
    peer_vpc_id: String,
    tags: Option<Value>,
    vpc_id: String,
}

impl Ec2VpcPeeringConnectionCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config().schema;
        Ok(Self {
            peer_owner_id: crate::send_model::optional_string(_resource, "peer_owner_id")?,
            peer_vpc_id: crate::send_model::required_string(_resource, "peer_vpc_id")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.peer_owner_id.is_some() {
            sent.insert("peer_owner_id".to_string());
        }
        sent.insert("peer_vpc_id".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>, String, Option<Value>, String) {
        let Self {
            peer_owner_id,
            peer_vpc_id,
            tags,
            vpc_id,
        } = self;
        (peer_owner_id, peer_vpc_id, tags, vpc_id)
    }
}

/// Exhaustive typed update input for `ec2.VpcPeeringConnection`.
pub(crate) struct Ec2VpcPeeringConnectionUpdateInput {
    tags: Option<Value>,
}

impl Ec2VpcPeeringConnectionUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `ec2.VpnGateway`.
pub(crate) struct Ec2VpnGatewayCreateInput {
    amazon_side_asn: Option<i64>,
    availability_zone: Option<String>,
    tags: Option<Value>,
    r#type: String,
}

impl Ec2VpnGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpn_gateway::ec2_vpn_gateway_config().schema;
        Ok(Self {
            amazon_side_asn: crate::send_model::optional_int(_resource, "amazon_side_asn")?,
            availability_zone: crate::send_model::optional_enum(
                _resource,
                _schema,
                "availability_zone",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.amazon_side_asn.is_some() {
            sent.insert("amazon_side_asn".to_string());
        }
        if self.availability_zone.is_some() {
            sent.insert("availability_zone".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("type".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<i64>, Option<String>, Option<Value>, String) {
        let Self {
            amazon_side_asn,
            availability_zone,
            tags,
            r#type,
        } = self;
        (amazon_side_asn, availability_zone, tags, r#type)
    }
}

/// Exhaustive typed update input for `ec2.VpnGateway`.
pub(crate) struct Ec2VpnGatewayUpdateInput {
    tags: Option<Value>,
}

impl Ec2VpnGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpn_gateway::ec2_vpn_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `s3.Bucket`.
pub(crate) struct S3BucketCreateInput {
    bucket: String,
    bucket_namespace: Option<String>,
    tags: Option<Value>,
}

impl S3BucketCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket::s3_bucket_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            bucket_namespace: crate::send_model::optional_enum(
                _resource,
                _schema,
                "bucket_namespace",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        if self.bucket_namespace.is_some() {
            sent.insert("bucket_namespace".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Option<String>, Option<Value>) {
        let Self {
            bucket,
            bucket_namespace,
            tags,
        } = self;
        (bucket, bucket_namespace, tags)
    }
}

/// Exhaustive typed update input for `s3.Bucket`.
pub(crate) struct S3BucketUpdateInput {
    tags: Option<Value>,
}

impl S3BucketUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket::s3_bucket_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `s3.BucketPolicy`.
pub(crate) struct S3BucketPolicyCreateInput {
    bucket: String,
    policy: Value,
}

impl S3BucketPolicyCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_policy::s3_bucket_policy_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            policy: crate::send_model::required_value(_resource, "policy")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("policy".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Value) {
        let Self { bucket, policy } = self;
        (bucket, policy)
    }
}

/// Exhaustive typed update input for `s3.BucketPolicy`.
pub(crate) struct S3BucketPolicyUpdateInput {
    policy: Value,
}

impl S3BucketPolicyUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_policy::s3_bucket_policy_config().schema;
        Ok(Self {
            policy: crate::send_model::required_value(_resource, "policy")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Value,) {
        let Self { policy } = self;
        (policy,)
    }
}

/// Exhaustive typed create input for `s3.BucketVersioning`.
pub(crate) struct S3BucketVersioningCreateInput {
    bucket: String,
    status: String,
}

impl S3BucketVersioningCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_versioning::s3_bucket_versioning_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            status: crate::send_model::required_enum(_resource, _schema, "status")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("status".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String) {
        let Self { bucket, status } = self;
        (bucket, status)
    }
}

/// Exhaustive typed update input for `s3.BucketVersioning`.
pub(crate) struct S3BucketVersioningUpdateInput {
    status: String,
}

impl S3BucketVersioningUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_versioning::s3_bucket_versioning_config().schema;
        Ok(Self {
            status: crate::send_model::required_enum(_resource, _schema, "status")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String,) {
        let Self { status } = self;
        (status,)
    }
}

/// Exhaustive typed create input for `s3.BucketObjectLockConfiguration`.
pub(crate) struct S3BucketObjectLockConfigurationCreateInput {
    bucket: String,
    object_lock_enabled: String,
    rule: Option<Value>,
}

impl S3BucketObjectLockConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_object_lock_configuration::s3_bucket_object_lock_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            object_lock_enabled: crate::send_model::required_enum(
                _resource,
                _schema,
                "object_lock_enabled",
            )?,
            rule: crate::send_model::optional_value(_resource, "rule")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("object_lock_enabled".to_string());
        if self.rule.is_some() {
            sent.insert("rule".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String, Option<Value>) {
        let Self {
            bucket,
            object_lock_enabled,
            rule,
        } = self;
        (bucket, object_lock_enabled, rule)
    }
}

/// Exhaustive typed update input for `s3.BucketObjectLockConfiguration`.
pub(crate) struct S3BucketObjectLockConfigurationUpdateInput {
    object_lock_enabled: String,
    rule: Option<Value>,
}

impl S3BucketObjectLockConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_object_lock_configuration::s3_bucket_object_lock_configuration_config().schema;
        Ok(Self {
            object_lock_enabled: crate::send_model::required_enum(
                _resource,
                _schema,
                "object_lock_enabled",
            )?,
            rule: crate::send_model::optional_value(_resource, "rule")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Option<Value>) {
        let Self {
            object_lock_enabled,
            rule,
        } = self;
        (object_lock_enabled, rule)
    }
}

/// Exhaustive typed create input for `s3.BucketAcl`.
pub(crate) struct S3BucketAclCreateInput {
    acl: String,
    bucket: String,
}

impl S3BucketAclCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema;
        Ok(Self {
            acl: crate::send_model::required_enum(_resource, _schema, "acl")?,
            bucket: crate::send_model::required_string(_resource, "bucket")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("acl".to_string());
        sent.insert("bucket".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String) {
        let Self { acl, bucket } = self;
        (acl, bucket)
    }
}

/// Exhaustive typed update input for `s3.BucketAcl`.
pub(crate) struct S3BucketAclUpdateInput {
    acl: String,
}

impl S3BucketAclUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema;
        Ok(Self {
            acl: crate::send_model::required_enum(_resource, _schema, "acl")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String,) {
        let Self { acl } = self;
        (acl,)
    }
}

/// Exhaustive typed create input for `s3.BucketOwnershipControls`.
pub(crate) struct S3BucketOwnershipControlsCreateInput {
    bucket: String,
    object_ownership: String,
}

impl S3BucketOwnershipControlsCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_ownership_controls::s3_bucket_ownership_controls_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            object_ownership: crate::send_model::required_enum(
                _resource,
                _schema,
                "object_ownership",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("object_ownership".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String) {
        let Self {
            bucket,
            object_ownership,
        } = self;
        (bucket, object_ownership)
    }
}

/// Exhaustive typed update input for `s3.BucketOwnershipControls`.
pub(crate) struct S3BucketOwnershipControlsUpdateInput {
    object_ownership: String,
}

impl S3BucketOwnershipControlsUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_ownership_controls::s3_bucket_ownership_controls_config().schema;
        Ok(Self {
            object_ownership: crate::send_model::required_enum(
                _resource,
                _schema,
                "object_ownership",
            )?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String,) {
        let Self { object_ownership } = self;
        (object_ownership,)
    }
}

/// Exhaustive typed create input for `s3.BucketServerSideEncryptionConfiguration`.
pub(crate) struct S3BucketServerSideEncryptionConfigurationCreateInput {
    bucket: String,
    rules: Value,
}

impl S3BucketServerSideEncryptionConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("rules".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Value) {
        let Self { bucket, rules } = self;
        (bucket, rules)
    }
}

/// Exhaustive typed update input for `s3.BucketServerSideEncryptionConfiguration`.
pub(crate) struct S3BucketServerSideEncryptionConfigurationUpdateInput {
    rules: Value,
}

impl S3BucketServerSideEncryptionConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema;
        Ok(Self {
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Value,) {
        let Self { rules } = self;
        (rules,)
    }
}

/// Exhaustive typed create input for `s3.BucketReplicationConfiguration`.
pub(crate) struct S3BucketReplicationConfigurationCreateInput {
    bucket: String,
    role: String,
    rules: Value,
}

impl S3BucketReplicationConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_replication_configuration::s3_bucket_replication_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            role: crate::send_model::required_string(_resource, "role")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("role".to_string());
        sent.insert("rules".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String, Value) {
        let Self {
            bucket,
            role,
            rules,
        } = self;
        (bucket, role, rules)
    }
}

/// Exhaustive typed update input for `s3.BucketReplicationConfiguration`.
pub(crate) struct S3BucketReplicationConfigurationUpdateInput {
    role: String,
    rules: Value,
}

impl S3BucketReplicationConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_replication_configuration::s3_bucket_replication_configuration_config().schema;
        Ok(Self {
            role: crate::send_model::required_string(_resource, "role")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Value) {
        let Self { role, rules } = self;
        (role, rules)
    }
}

/// Exhaustive typed create input for `s3.BucketLifecycleConfiguration`.
pub(crate) struct S3BucketLifecycleConfigurationCreateInput {
    bucket: String,
    rules: Value,
}

impl S3BucketLifecycleConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("rules".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Value) {
        let Self { bucket, rules } = self;
        (bucket, rules)
    }
}

/// Exhaustive typed update input for `s3.BucketLifecycleConfiguration`.
pub(crate) struct S3BucketLifecycleConfigurationUpdateInput {
    rules: Value,
}

impl S3BucketLifecycleConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema;
        Ok(Self {
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Value,) {
        let Self { rules } = self;
        (rules,)
    }
}

/// Exhaustive typed create input for `s3.BucketWebsiteConfiguration`.
pub(crate) struct S3BucketWebsiteConfigurationCreateInput {
    bucket: String,
    error_document: Option<Value>,
    index_document: Option<Value>,
    redirect_all_requests_to: Option<Value>,
}

impl S3BucketWebsiteConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_website_configuration::s3_bucket_website_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            error_document: crate::send_model::optional_value(_resource, "error_document")?,
            index_document: crate::send_model::optional_value(_resource, "index_document")?,
            redirect_all_requests_to: crate::send_model::optional_value(
                _resource,
                "redirect_all_requests_to",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        if self.error_document.is_some() {
            sent.insert("error_document".to_string());
        }
        if self.index_document.is_some() {
            sent.insert("index_document".to_string());
        }
        if self.redirect_all_requests_to.is_some() {
            sent.insert("redirect_all_requests_to".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Option<Value>, Option<Value>, Option<Value>) {
        let Self {
            bucket,
            error_document,
            index_document,
            redirect_all_requests_to,
        } = self;
        (
            bucket,
            error_document,
            index_document,
            redirect_all_requests_to,
        )
    }
}

/// Exhaustive typed update input for `s3.BucketWebsiteConfiguration`.
pub(crate) struct S3BucketWebsiteConfigurationUpdateInput {
    error_document: Option<Value>,
    index_document: Option<Value>,
    redirect_all_requests_to: Option<Value>,
}

impl S3BucketWebsiteConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_website_configuration::s3_bucket_website_configuration_config().schema;
        Ok(Self {
            error_document: crate::send_model::optional_value(_resource, "error_document")?,
            index_document: crate::send_model::optional_value(_resource, "index_document")?,
            redirect_all_requests_to: crate::send_model::optional_value(
                _resource,
                "redirect_all_requests_to",
            )?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, Option<Value>, Option<Value>) {
        let Self {
            error_document,
            index_document,
            redirect_all_requests_to,
        } = self;
        (error_document, index_document, redirect_all_requests_to)
    }
}

/// Exhaustive typed create input for `s3.BucketCorsConfiguration`.
pub(crate) struct S3BucketCorsConfigurationCreateInput {
    bucket: String,
    cors_rules: Value,
}

impl S3BucketCorsConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_cors_configuration::s3_bucket_cors_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            cors_rules: crate::send_model::required_value(_resource, "cors_rules")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("cors_rules".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Value) {
        let Self { bucket, cors_rules } = self;
        (bucket, cors_rules)
    }
}

/// Exhaustive typed update input for `s3.BucketCorsConfiguration`.
pub(crate) struct S3BucketCorsConfigurationUpdateInput {
    cors_rules: Value,
}

impl S3BucketCorsConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_cors_configuration::s3_bucket_cors_configuration_config().schema;
        Ok(Self {
            cors_rules: crate::send_model::required_value(_resource, "cors_rules")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Value,) {
        let Self { cors_rules } = self;
        (cors_rules,)
    }
}

/// Exhaustive typed create input for `s3.BucketNotificationConfiguration`.
pub(crate) struct S3BucketNotificationConfigurationCreateInput {
    bucket: String,
    event_bridge_configuration: Option<Value>,
    lambda_function_configurations: Option<Value>,
    queue_configurations: Option<Value>,
    topic_configurations: Option<Value>,
}

impl S3BucketNotificationConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_notification_configuration::s3_bucket_notification_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            event_bridge_configuration: crate::send_model::optional_value(
                _resource,
                "event_bridge_configuration",
            )?,
            lambda_function_configurations: crate::send_model::optional_value(
                _resource,
                "lambda_function_configurations",
            )?,
            queue_configurations: crate::send_model::optional_value(
                _resource,
                "queue_configurations",
            )?,
            topic_configurations: crate::send_model::optional_value(
                _resource,
                "topic_configurations",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        if self.event_bridge_configuration.is_some() {
            sent.insert("event_bridge_configuration".to_string());
        }
        if self.lambda_function_configurations.is_some() {
            sent.insert("lambda_function_configurations".to_string());
        }
        if self.queue_configurations.is_some() {
            sent.insert("queue_configurations".to_string());
        }
        if self.topic_configurations.is_some() {
            sent.insert("topic_configurations".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        String,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
    ) {
        let Self {
            bucket,
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        } = self;
        (
            bucket,
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        )
    }
}

/// Exhaustive typed update input for `s3.BucketNotificationConfiguration`.
pub(crate) struct S3BucketNotificationConfigurationUpdateInput {
    event_bridge_configuration: Option<Value>,
    lambda_function_configurations: Option<Value>,
    queue_configurations: Option<Value>,
    topic_configurations: Option<Value>,
}

impl S3BucketNotificationConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_notification_configuration::s3_bucket_notification_configuration_config().schema;
        Ok(Self {
            event_bridge_configuration: crate::send_model::optional_value(
                _resource,
                "event_bridge_configuration",
            )?,
            lambda_function_configurations: crate::send_model::optional_value(
                _resource,
                "lambda_function_configurations",
            )?,
            queue_configurations: crate::send_model::optional_value(
                _resource,
                "queue_configurations",
            )?,
            topic_configurations: crate::send_model::optional_value(
                _resource,
                "topic_configurations",
            )?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, Option<Value>, Option<Value>, Option<Value>) {
        let Self {
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        } = self;
        (
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        )
    }
}

/// Exhaustive typed create input for `s3.BucketLogging`.
pub(crate) struct S3BucketLoggingCreateInput {
    bucket: String,
    target_bucket: String,
    target_object_key_format: Option<Value>,
    target_prefix: Option<String>,
}

impl S3BucketLoggingCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_logging::s3_bucket_logging_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            target_bucket: crate::send_model::required_string(_resource, "target_bucket")?,
            target_object_key_format: crate::send_model::optional_value(
                _resource,
                "target_object_key_format",
            )?,
            target_prefix: crate::send_model::optional_string(_resource, "target_prefix")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("target_bucket".to_string());
        if self.target_object_key_format.is_some() {
            sent.insert("target_object_key_format".to_string());
        }
        if self.target_prefix.is_some() {
            sent.insert("target_prefix".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, String, Option<Value>, Option<String>) {
        let Self {
            bucket,
            target_bucket,
            target_object_key_format,
            target_prefix,
        } = self;
        (
            bucket,
            target_bucket,
            target_object_key_format,
            target_prefix,
        )
    }
}

/// Exhaustive typed update input for `s3.BucketLogging`.
pub(crate) struct S3BucketLoggingUpdateInput {
    target_bucket: String,
    target_object_key_format: Option<Value>,
    target_prefix: Option<String>,
}

impl S3BucketLoggingUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_logging::s3_bucket_logging_config().schema;
        Ok(Self {
            target_bucket: crate::send_model::required_string(_resource, "target_bucket")?,
            target_object_key_format: crate::send_model::optional_value(
                _resource,
                "target_object_key_format",
            )?,
            target_prefix: crate::send_model::optional_string(_resource, "target_prefix")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Option<Value>, Option<String>) {
        let Self {
            target_bucket,
            target_object_key_format,
            target_prefix,
        } = self;
        (target_bucket, target_object_key_format, target_prefix)
    }
}

/// Exhaustive typed create input for `s3.BucketPublicAccessBlock`.
pub(crate) struct S3BucketPublicAccessBlockCreateInput {
    block_public_acls: Option<bool>,
    block_public_policy: Option<bool>,
    bucket: String,
    ignore_public_acls: Option<bool>,
    restrict_public_buckets: Option<bool>,
}

impl S3BucketPublicAccessBlockCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_public_access_block::s3_bucket_public_access_block_config().schema;
        Ok(Self {
            block_public_acls: crate::send_model::optional_bool(_resource, "block_public_acls")?,
            block_public_policy: crate::send_model::optional_bool(
                _resource,
                "block_public_policy",
            )?,
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            ignore_public_acls: crate::send_model::optional_bool(_resource, "ignore_public_acls")?,
            restrict_public_buckets: crate::send_model::optional_bool(
                _resource,
                "restrict_public_buckets",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.block_public_acls.is_some() {
            sent.insert("block_public_acls".to_string());
        }
        if self.block_public_policy.is_some() {
            sent.insert("block_public_policy".to_string());
        }
        sent.insert("bucket".to_string());
        if self.ignore_public_acls.is_some() {
            sent.insert("ignore_public_acls".to_string());
        }
        if self.restrict_public_buckets.is_some() {
            sent.insert("restrict_public_buckets".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<bool>,
        String,
        Option<bool>,
        Option<bool>,
    ) {
        let Self {
            block_public_acls,
            block_public_policy,
            bucket,
            ignore_public_acls,
            restrict_public_buckets,
        } = self;
        (
            block_public_acls,
            block_public_policy,
            bucket,
            ignore_public_acls,
            restrict_public_buckets,
        )
    }
}

/// Exhaustive typed update input for `s3.BucketPublicAccessBlock`.
pub(crate) struct S3BucketPublicAccessBlockUpdateInput {
    block_public_acls: Option<bool>,
    block_public_policy: Option<bool>,
    ignore_public_acls: Option<bool>,
    restrict_public_buckets: Option<bool>,
}

impl S3BucketPublicAccessBlockUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_public_access_block::s3_bucket_public_access_block_config().schema;
        Ok(Self {
            block_public_acls: crate::send_model::optional_bool(_resource, "block_public_acls")?,
            block_public_policy: crate::send_model::optional_bool(
                _resource,
                "block_public_policy",
            )?,
            ignore_public_acls: crate::send_model::optional_bool(_resource, "ignore_public_acls")?,
            restrict_public_buckets: crate::send_model::optional_bool(
                _resource,
                "restrict_public_buckets",
            )?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<bool>, Option<bool>, Option<bool>, Option<bool>) {
        let Self {
            block_public_acls,
            block_public_policy,
            ignore_public_acls,
            restrict_public_buckets,
        } = self;
        (
            block_public_acls,
            block_public_policy,
            ignore_public_acls,
            restrict_public_buckets,
        )
    }
}

/// Exhaustive typed create input for `organizations.Organization`.
pub(crate) struct OrganizationsOrganizationCreateInput {
    feature_set: Option<String>,
}

impl OrganizationsOrganizationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::organizations::organization::organizations_organization_config().schema;
        Ok(Self {
            feature_set: crate::send_model::optional_enum(_resource, _schema, "feature_set")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.feature_set.is_some() {
            sent.insert("feature_set".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<String>,) {
        let Self { feature_set } = self;
        (feature_set,)
    }
}

/// Exhaustive typed update input for `organizations.Organization`.
pub(crate) struct OrganizationsOrganizationUpdateInput;

impl OrganizationsOrganizationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::organizations::organization::organizations_organization_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) {
        let Self = self;
    }
}

/// Exhaustive typed create input for `organizations.Account`.
pub(crate) struct OrganizationsAccountCreateInput {
    account_name: String,
    email: String,
    iam_user_access_to_billing: Option<String>,
    role_name: Option<String>,
    tags: Option<Value>,
}

impl OrganizationsAccountCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::organizations::account::organizations_account_config()
                .schema;
        Ok(Self {
            account_name: crate::send_model::required_string(_resource, "account_name")?,
            email: crate::send_model::required_string(_resource, "email")?,
            iam_user_access_to_billing: crate::send_model::optional_enum(
                _resource,
                _schema,
                "iam_user_access_to_billing",
            )?,
            role_name: crate::send_model::optional_string(_resource, "role_name")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("account_name".to_string());
        sent.insert("email".to_string());
        if self.iam_user_access_to_billing.is_some() {
            sent.insert("iam_user_access_to_billing".to_string());
        }
        if self.role_name.is_some() {
            sent.insert("role_name".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<Value>,
    ) {
        let Self {
            account_name,
            email,
            iam_user_access_to_billing,
            role_name,
            tags,
        } = self;
        (
            account_name,
            email,
            iam_user_access_to_billing,
            role_name,
            tags,
        )
    }
}

/// Exhaustive typed update input for `organizations.Account`.
pub(crate) struct OrganizationsAccountUpdateInput {
    account_name: String,
    tags: Option<Value>,
}

impl OrganizationsAccountUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::organizations::account::organizations_account_config()
                .schema;
        Ok(Self {
            account_name: crate::send_model::required_string(_resource, "account_name")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (String, Option<Value>) {
        let Self { account_name, tags } = self;
        (account_name, tags)
    }
}

/// Exhaustive typed create input for `route53.RecordSet`.
pub(crate) struct Route53RecordSetCreateInput {
    alias_target: Option<Value>,
    hosted_zone_id: String,
    name: String,
    resource_records: Option<Value>,
    ttl: Option<i64>,
    r#type: String,
}

impl Route53RecordSetCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::route53::record_set::route53_record_set_config().schema;
        Ok(Self {
            alias_target: crate::send_model::optional_value(_resource, "alias_target")?,
            hosted_zone_id: crate::send_model::required_string(_resource, "hosted_zone_id")?,
            name: crate::send_model::required_string(_resource, "name")?,
            resource_records: crate::send_model::optional_value(_resource, "resource_records")?,
            ttl: crate::send_model::optional_int(_resource, "ttl")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.alias_target.is_some() {
            sent.insert("alias_target".to_string());
        }
        sent.insert("hosted_zone_id".to_string());
        sent.insert("name".to_string());
        if self.resource_records.is_some() {
            sent.insert("resource_records".to_string());
        }
        if self.ttl.is_some() {
            sent.insert("ttl".to_string());
        }
        sent.insert("type".to_string());
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<Value>,
        String,
        String,
        Option<Value>,
        Option<i64>,
        String,
    ) {
        let Self {
            alias_target,
            hosted_zone_id,
            name,
            resource_records,
            ttl,
            r#type,
        } = self;
        (
            alias_target,
            hosted_zone_id,
            name,
            resource_records,
            ttl,
            r#type,
        )
    }
}

/// Exhaustive typed update input for `route53.RecordSet`.
pub(crate) struct Route53RecordSetUpdateInput {
    alias_target: Option<Value>,
    resource_records: Option<Value>,
    ttl: Option<i64>,
    r#type: String,
}

impl Route53RecordSetUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::route53::record_set::route53_record_set_config().schema;
        Ok(Self {
            alias_target: crate::send_model::optional_value(_resource, "alias_target")?,
            resource_records: crate::send_model::optional_value(_resource, "resource_records")?,
            ttl: crate::send_model::optional_int(_resource, "ttl")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>, Option<Value>, Option<i64>, String) {
        let Self {
            alias_target,
            resource_records,
            ttl,
            r#type,
        } = self;
        (alias_target, resource_records, ttl, r#type)
    }
}

/// Exhaustive typed create input for `iam.Role`.
pub(crate) struct IamRoleCreateInput {
    assume_role_policy_document: Value,
    description: Option<String>,
    max_session_duration: Option<i64>,
    path: Option<String>,
    role_name: String,
    tags: Option<Value>,
}

impl IamRoleCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::iam::role::iam_role_config().schema;
        Ok(Self {
            assume_role_policy_document: crate::send_model::required_value(
                _resource,
                "assume_role_policy_document",
            )?,
            description: crate::send_model::optional_string(_resource, "description")?,
            max_session_duration: crate::send_model::optional_int(
                _resource,
                "max_session_duration",
            )?,
            path: crate::send_model::optional_string(_resource, "path")?,
            role_name: crate::send_model::required_string(_resource, "role_name")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("assume_role_policy_document".to_string());
        if self.description.is_some() {
            sent.insert("description".to_string());
        }
        if self.max_session_duration.is_some() {
            sent.insert("max_session_duration".to_string());
        }
        if self.path.is_some() {
            sent.insert("path".to_string());
        }
        sent.insert("role_name".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Value,
        Option<String>,
        Option<i64>,
        Option<String>,
        String,
        Option<Value>,
    ) {
        let Self {
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_name,
            tags,
        } = self;
        (
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_name,
            tags,
        )
    }
}

/// Exhaustive typed update input for `iam.Role`.
pub(crate) struct IamRoleUpdateInput {
    tags: Option<Value>,
}

impl IamRoleUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::iam::role::iam_role_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `logs.LogGroup`.
pub(crate) struct LogsLogGroupCreateInput {
    deletion_protection_enabled: Option<bool>,
    kms_key_id: Option<String>,
    log_group_class: Option<String>,
    log_group_name: String,
    retention_in_days: Option<i64>,
    tags: Option<Value>,
}

impl LogsLogGroupCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::logs::log_group::logs_log_group_config().schema;
        Ok(Self {
            deletion_protection_enabled: crate::send_model::optional_bool(
                _resource,
                "deletion_protection_enabled",
            )?,
            kms_key_id: crate::send_model::optional_string(_resource, "kms_key_id")?,
            log_group_class: crate::send_model::optional_enum(
                _resource,
                _schema,
                "log_group_class",
            )?,
            log_group_name: crate::send_model::required_string(_resource, "log_group_name")?,
            retention_in_days: crate::send_model::optional_int(_resource, "retention_in_days")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.deletion_protection_enabled.is_some() {
            sent.insert("deletion_protection_enabled".to_string());
        }
        if self.kms_key_id.is_some() {
            sent.insert("kms_key_id".to_string());
        }
        if self.log_group_class.is_some() {
            sent.insert("log_group_class".to_string());
        }
        sent.insert("log_group_name".to_string());
        if self.retention_in_days.is_some() {
            sent.insert("retention_in_days".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
        Option<Value>,
    ) {
        let Self {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            tags,
        } = self;
        (
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            tags,
        )
    }
}

/// Exhaustive typed update input for `logs.LogGroup`.
pub(crate) struct LogsLogGroupUpdateInput {
    deletion_protection_enabled: Option<bool>,
    retention_in_days: Option<i64>,
    tags: Option<Value>,
}

impl LogsLogGroupUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::logs::log_group::logs_log_group_config().schema;
        Ok(Self {
            deletion_protection_enabled: crate::send_model::optional_bool(
                _resource,
                "deletion_protection_enabled",
            )?,
            retention_in_days: crate::send_model::optional_int(_resource, "retention_in_days")?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<bool>, Option<i64>, Option<Value>) {
        let Self {
            deletion_protection_enabled,
            retention_in_days,
            tags,
        } = self;
        (deletion_protection_enabled, retention_in_days, tags)
    }
}

/// Exhaustive typed create input for `acm.Certificate`.
pub(crate) struct AcmCertificateCreateInput {
    domain_name: String,
    key_algorithm: Option<String>,
    options: Option<Value>,
    subject_alternative_names: Option<Value>,
    tags: Option<Value>,
    validation_method: Option<String>,
}

impl AcmCertificateCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        Ok(Self {
            domain_name: crate::send_model::required_string(_resource, "domain_name")?,
            key_algorithm: crate::send_model::optional_enum(_resource, _schema, "key_algorithm")?,
            options: crate::send_model::optional_value(_resource, "options")?,
            subject_alternative_names: crate::send_model::optional_value(
                _resource,
                "subject_alternative_names",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            validation_method: crate::send_model::optional_enum(
                _resource,
                _schema,
                "validation_method",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        sent.insert("domain_name".to_string());
        if self.key_algorithm.is_some() {
            sent.insert("key_algorithm".to_string());
        }
        if self.options.is_some() {
            sent.insert("options".to_string());
        }
        if self.subject_alternative_names.is_some() {
            sent.insert("subject_alternative_names".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.validation_method.is_some() {
            sent.insert("validation_method".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        String,
        Option<String>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        Option<String>,
    ) {
        let Self {
            domain_name,
            key_algorithm,
            options,
            subject_alternative_names,
            tags,
            validation_method,
        } = self;
        (
            domain_name,
            key_algorithm,
            options,
            subject_alternative_names,
            tags,
            validation_method,
        )
    }
}

/// Exhaustive typed update input for `acm.Certificate`.
pub(crate) struct AcmCertificateUpdateInput {
    tags: Option<Value>,
}

impl AcmCertificateUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_value(_resource, "tags")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(self) -> (Option<Value>,) {
        let Self { tags } = self;
        (tags,)
    }
}

/// Exhaustive typed create input for `sqs.Queue`.
pub(crate) struct SqsQueueCreateInput {
    content_based_deduplication: Option<bool>,
    deduplication_scope: Option<String>,
    delay_seconds: Option<i64>,
    fifo_queue: Option<bool>,
    fifo_throughput_limit: Option<String>,
    kms_data_key_reuse_period_seconds: Option<i64>,
    kms_master_key_id: Option<String>,
    maximum_message_size: Option<i64>,
    message_retention_period: Option<i64>,
    policy: Option<Value>,
    queue_name: String,
    receive_message_wait_time_seconds: Option<i64>,
    redrive_allow_policy: Option<Value>,
    redrive_policy: Option<Value>,
    sqs_managed_sse_enabled: Option<bool>,
    tags: Option<Value>,
    visibility_timeout: Option<i64>,
}

impl SqsQueueCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::sqs::queue::sqs_queue_config().schema;
        Ok(Self {
            content_based_deduplication: crate::send_model::optional_bool(
                _resource,
                "content_based_deduplication",
            )?,
            deduplication_scope: crate::send_model::optional_string(
                _resource,
                "deduplication_scope",
            )?,
            delay_seconds: crate::send_model::optional_int(_resource, "delay_seconds")?,
            fifo_queue: crate::send_model::optional_bool(_resource, "fifo_queue")?,
            fifo_throughput_limit: crate::send_model::optional_string(
                _resource,
                "fifo_throughput_limit",
            )?,
            kms_data_key_reuse_period_seconds: crate::send_model::optional_int(
                _resource,
                "kms_data_key_reuse_period_seconds",
            )?,
            kms_master_key_id: crate::send_model::optional_string(_resource, "kms_master_key_id")?,
            maximum_message_size: crate::send_model::optional_int(
                _resource,
                "maximum_message_size",
            )?,
            message_retention_period: crate::send_model::optional_int(
                _resource,
                "message_retention_period",
            )?,
            policy: crate::send_model::optional_value(_resource, "policy")?,
            queue_name: crate::send_model::required_string(_resource, "queue_name")?,
            receive_message_wait_time_seconds: crate::send_model::optional_int(
                _resource,
                "receive_message_wait_time_seconds",
            )?,
            redrive_allow_policy: crate::send_model::optional_value(
                _resource,
                "redrive_allow_policy",
            )?,
            redrive_policy: crate::send_model::optional_value(_resource, "redrive_policy")?,
            sqs_managed_sse_enabled: crate::send_model::optional_bool(
                _resource,
                "sqs_managed_sse_enabled",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            visibility_timeout: crate::send_model::optional_int(_resource, "visibility_timeout")?,
        })
    }

    pub(crate) fn sent(&self) -> Sent {
        let mut sent = BTreeSet::new();
        if self.content_based_deduplication.is_some() {
            sent.insert("content_based_deduplication".to_string());
        }
        if self.deduplication_scope.is_some() {
            sent.insert("deduplication_scope".to_string());
        }
        if self.delay_seconds.is_some() {
            sent.insert("delay_seconds".to_string());
        }
        if self.fifo_queue.is_some() {
            sent.insert("fifo_queue".to_string());
        }
        if self.fifo_throughput_limit.is_some() {
            sent.insert("fifo_throughput_limit".to_string());
        }
        if self.kms_data_key_reuse_period_seconds.is_some() {
            sent.insert("kms_data_key_reuse_period_seconds".to_string());
        }
        if self.kms_master_key_id.is_some() {
            sent.insert("kms_master_key_id".to_string());
        }
        if self.maximum_message_size.is_some() {
            sent.insert("maximum_message_size".to_string());
        }
        if self.message_retention_period.is_some() {
            sent.insert("message_retention_period".to_string());
        }
        if self.policy.is_some() {
            sent.insert("policy".to_string());
        }
        sent.insert("queue_name".to_string());
        if self.receive_message_wait_time_seconds.is_some() {
            sent.insert("receive_message_wait_time_seconds".to_string());
        }
        if self.redrive_allow_policy.is_some() {
            sent.insert("redrive_allow_policy".to_string());
        }
        if self.redrive_policy.is_some() {
            sent.insert("redrive_policy".to_string());
        }
        if self.sqs_managed_sse_enabled.is_some() {
            sent.insert("sqs_managed_sse_enabled".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.visibility_timeout.is_some() {
            sent.insert("visibility_timeout".to_string());
        }
        Sent::only(sent)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<String>,
        Option<i64>,
        Option<bool>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<Value>,
        String,
        Option<i64>,
        Option<Value>,
        Option<Value>,
        Option<bool>,
        Option<Value>,
        Option<i64>,
    ) {
        let Self {
            content_based_deduplication,
            deduplication_scope,
            delay_seconds,
            fifo_queue,
            fifo_throughput_limit,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            queue_name,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        } = self;
        (
            content_based_deduplication,
            deduplication_scope,
            delay_seconds,
            fifo_queue,
            fifo_throughput_limit,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            queue_name,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        )
    }
}

/// Exhaustive typed update input for `sqs.Queue`.
pub(crate) struct SqsQueueUpdateInput {
    content_based_deduplication: Option<bool>,
    delay_seconds: Option<i64>,
    kms_data_key_reuse_period_seconds: Option<i64>,
    kms_master_key_id: Option<String>,
    maximum_message_size: Option<i64>,
    message_retention_period: Option<i64>,
    policy: Option<Value>,
    receive_message_wait_time_seconds: Option<i64>,
    redrive_allow_policy: Option<Value>,
    redrive_policy: Option<Value>,
    sqs_managed_sse_enabled: Option<bool>,
    tags: Option<Value>,
    visibility_timeout: Option<i64>,
}

impl SqsQueueUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::sqs::queue::sqs_queue_config().schema;
        Ok(Self {
            content_based_deduplication: crate::send_model::optional_bool(
                _resource,
                "content_based_deduplication",
            )?,
            delay_seconds: crate::send_model::optional_int(_resource, "delay_seconds")?,
            kms_data_key_reuse_period_seconds: crate::send_model::optional_int(
                _resource,
                "kms_data_key_reuse_period_seconds",
            )?,
            kms_master_key_id: crate::send_model::optional_string(_resource, "kms_master_key_id")?,
            maximum_message_size: crate::send_model::optional_int(
                _resource,
                "maximum_message_size",
            )?,
            message_retention_period: crate::send_model::optional_int(
                _resource,
                "message_retention_period",
            )?,
            policy: crate::send_model::optional_value(_resource, "policy")?,
            receive_message_wait_time_seconds: crate::send_model::optional_int(
                _resource,
                "receive_message_wait_time_seconds",
            )?,
            redrive_allow_policy: crate::send_model::optional_value(
                _resource,
                "redrive_allow_policy",
            )?,
            redrive_policy: crate::send_model::optional_value(_resource, "redrive_policy")?,
            sqs_managed_sse_enabled: crate::send_model::optional_bool(
                _resource,
                "sqs_managed_sse_enabled",
            )?,
            tags: crate::send_model::optional_value(_resource, "tags")?,
            visibility_timeout: crate::send_model::optional_int(_resource, "visibility_timeout")?,
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<bool>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<Value>,
        Option<i64>,
        Option<Value>,
        Option<Value>,
        Option<bool>,
        Option<Value>,
        Option<i64>,
    ) {
        let Self {
            content_based_deduplication,
            delay_seconds,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        } = self;
        (
            content_based_deduplication,
            delay_seconds,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        )
    }
}

#[cfg(test)]
pub(crate) struct SendCaptureAttribute {
    pub(crate) name: &'static str,
    pub(crate) provider_name: &'static str,
    pub(crate) capture_names: &'static [&'static str],
}

#[cfg(test)]
pub(crate) struct SendCaptureContract {
    pub(crate) resource_type: &'static str,
    pub(crate) attributes: &'static [SendCaptureAttribute],
}

#[cfg(test)]
pub(crate) fn capture_contracts() -> &'static [SendCaptureContract] {
    &[
        SendCaptureContract {
            resource_type: "ec2.Vpc",
            attributes: &[
                SendCaptureAttribute {
                    name: "cidr_block",
                    provider_name: "CidrBlock",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "enable_dns_hostnames",
                    provider_name: "EnableDnsHostnames",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "enable_dns_support",
                    provider_name: "EnableDnsSupport",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "instance_tenancy",
                    provider_name: "InstanceTenancy",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv4_ipam_pool_id",
                    provider_name: "Ipv4IpamPoolId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv4_netmask_length",
                    provider_name: "Ipv4NetmaskLength",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.Subnet",
            attributes: &[
                SendCaptureAttribute {
                    name: "assign_ipv6_address_on_creation",
                    provider_name: "AssignIpv6AddressOnCreation",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "availability_zone",
                    provider_name: "AvailabilityZone",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "availability_zone_id",
                    provider_name: "AvailabilityZoneId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "cidr_block",
                    provider_name: "CidrBlock",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "enable_dns64",
                    provider_name: "EnableDns64",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "enable_lni_at_device_index",
                    provider_name: "EnableLniAtDeviceIndex",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv4_ipam_pool_id",
                    provider_name: "Ipv4IpamPoolId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv4_netmask_length",
                    provider_name: "Ipv4NetmaskLength",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv6_cidr_block",
                    provider_name: "Ipv6CidrBlock",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv6_ipam_pool_id",
                    provider_name: "Ipv6IpamPoolId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv6_native",
                    provider_name: "Ipv6Native",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ipv6_netmask_length",
                    provider_name: "Ipv6NetmaskLength",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "map_public_ip_on_launch",
                    provider_name: "MapPublicIpOnLaunch",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "outpost_arn",
                    provider_name: "OutpostArn",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "private_dns_name_options_on_launch",
                    provider_name: "PrivateDnsNameOptionsOnLaunch",
                    capture_names: &[
                        "HostnameType",
                        "EnableResourceNameDnsARecordOnLaunch",
                        "EnableResourceNameDnsAAAARecordOnLaunch",
                    ],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.InternetGateway",
            attributes: &[
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.RouteTable",
            attributes: &[
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.Route",
            attributes: &[
                SendCaptureAttribute {
                    name: "destination_cidr_block",
                    provider_name: "DestinationCidrBlock",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "gateway_id",
                    provider_name: "GatewayId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "nat_gateway_id",
                    provider_name: "NatGatewayId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "route_table_id",
                    provider_name: "RouteTableId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroup",
            attributes: &[
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "group_name",
                    provider_name: "GroupName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroupIngress",
            attributes: &[
                SendCaptureAttribute {
                    name: "cidr_ip",
                    provider_name: "CidrIp",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "cidr_ipv6",
                    provider_name: "CidrIpv6",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "from_port",
                    provider_name: "FromPort",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "group_id",
                    provider_name: "GroupId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ip_protocol",
                    provider_name: "IpProtocol",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "source_prefix_list_id",
                    provider_name: "SourcePrefixListId",
                    capture_names: &["PrefixListIds"],
                },
                SendCaptureAttribute {
                    name: "source_security_group_id",
                    provider_name: "SourceSecurityGroupId",
                    capture_names: &["Groups"],
                },
                SendCaptureAttribute {
                    name: "source_security_group_owner_id",
                    provider_name: "SourceSecurityGroupOwnerId",
                    capture_names: &["UserId"],
                },
                SendCaptureAttribute {
                    name: "to_port",
                    provider_name: "ToPort",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroupEgress",
            attributes: &[
                SendCaptureAttribute {
                    name: "cidr_ip",
                    provider_name: "CidrIp",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "cidr_ipv6",
                    provider_name: "CidrIpv6",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "destination_prefix_list_id",
                    provider_name: "DestinationPrefixListId",
                    capture_names: &["PrefixListIds"],
                },
                SendCaptureAttribute {
                    name: "destination_security_group_id",
                    provider_name: "DestinationSecurityGroupId",
                    capture_names: &["Groups"],
                },
                SendCaptureAttribute {
                    name: "from_port",
                    provider_name: "FromPort",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "group_id",
                    provider_name: "GroupId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ip_protocol",
                    provider_name: "IpProtocol",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "source_security_group_owner_id",
                    provider_name: "SourceSecurityGroupOwnerId",
                    capture_names: &["UserId"],
                },
                SendCaptureAttribute {
                    name: "to_port",
                    provider_name: "ToPort",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.EgressOnlyInternetGateway",
            attributes: &[
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.Eip",
            attributes: &[
                SendCaptureAttribute {
                    name: "address",
                    provider_name: "Address",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "domain",
                    provider_name: "Domain",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "public_ipv4_pool",
                    provider_name: "PublicIpv4Pool",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.FlowLog",
            attributes: &[
                SendCaptureAttribute {
                    name: "deliver_logs_permission_arn",
                    provider_name: "DeliverLogsPermissionArn",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_destination",
                    provider_name: "LogDestination",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_destination_type",
                    provider_name: "LogDestinationType",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_format",
                    provider_name: "LogFormat",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_group_name",
                    provider_name: "LogGroupName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "max_aggregation_interval",
                    provider_name: "MaxAggregationInterval",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "resource_ids",
                    provider_name: "ResourceIds",
                    capture_names: &["ResourceId"],
                },
                SendCaptureAttribute {
                    name: "resource_type",
                    provider_name: "ResourceType",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "traffic_type",
                    provider_name: "TrafficType",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.NatGateway",
            attributes: &[
                SendCaptureAttribute {
                    name: "allocation_id",
                    provider_name: "AllocationId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "availability_mode",
                    provider_name: "AvailabilityMode",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "availability_zone_addresses",
                    provider_name: "AvailabilityZoneAddresses",
                    capture_names: &["AvailabilityZoneAddress"],
                },
                SendCaptureAttribute {
                    name: "connectivity_type",
                    provider_name: "ConnectivityType",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "private_ip_address",
                    provider_name: "PrivateIpAddress",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "subnet_id",
                    provider_name: "SubnetId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.SubnetRouteTableAssociation",
            attributes: &[
                SendCaptureAttribute {
                    name: "route_table_id",
                    provider_name: "RouteTableId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "subnet_id",
                    provider_name: "SubnetId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.TransitGateway",
            attributes: &[
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "options",
                    provider_name: "Options",
                    capture_names: &[
                        "AmazonSideAsn",
                        "AutoAcceptSharedAttachments",
                        "DefaultRouteTableAssociation",
                        "DefaultRouteTablePropagation",
                        "DnsSupport",
                        "MulticastSupport",
                        "SecurityGroupReferencingSupport",
                        "TransitGatewayCidrBlocks",
                        "VpnEcmpSupport",
                    ],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.TransitGatewayAttachment",
            attributes: &[
                SendCaptureAttribute {
                    name: "options",
                    provider_name: "Options",
                    capture_names: &[
                        "ApplianceModeSupport",
                        "DnsSupport",
                        "Ipv6Support",
                        "SecurityGroupReferencingSupport",
                    ],
                },
                SendCaptureAttribute {
                    name: "subnet_ids",
                    provider_name: "SubnetIds",
                    capture_names: &["SubnetId"],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "transit_gateway_id",
                    provider_name: "TransitGatewayId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcEndpoint",
            attributes: &[
                SendCaptureAttribute {
                    name: "policy_document",
                    provider_name: "PolicyDocument",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "private_dns_enabled",
                    provider_name: "PrivateDnsEnabled",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "resource_configuration_arn",
                    provider_name: "ResourceConfigurationArn",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "route_table_ids",
                    provider_name: "RouteTableIds",
                    capture_names: &["RouteTableId"],
                },
                SendCaptureAttribute {
                    name: "security_group_ids",
                    provider_name: "SecurityGroupIds",
                    capture_names: &["SecurityGroupId"],
                },
                SendCaptureAttribute {
                    name: "service_name",
                    provider_name: "ServiceName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "service_network_arn",
                    provider_name: "ServiceNetworkArn",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "service_region",
                    provider_name: "ServiceRegion",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "subnet_ids",
                    provider_name: "SubnetIds",
                    capture_names: &["SubnetId"],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_endpoint_type",
                    provider_name: "VpcEndpointType",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcGatewayAttachment",
            attributes: &[
                SendCaptureAttribute {
                    name: "internet_gateway_id",
                    provider_name: "InternetGatewayId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpn_gateway_id",
                    provider_name: "VpnGatewayId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcPeeringConnection",
            attributes: &[
                SendCaptureAttribute {
                    name: "peer_owner_id",
                    provider_name: "PeerOwnerId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "peer_vpc_id",
                    provider_name: "PeerVpcId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "vpc_id",
                    provider_name: "VpcId",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.VpnGateway",
            attributes: &[
                SendCaptureAttribute {
                    name: "amazon_side_asn",
                    provider_name: "AmazonSideAsn",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "availability_zone",
                    provider_name: "AvailabilityZone",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "type",
                    provider_name: "Type",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.Bucket",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "bucket_namespace",
                    provider_name: "BucketNamespace",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketPolicy",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "policy",
                    provider_name: "Policy",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketVersioning",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "status",
                    provider_name: "Status",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketObjectLockConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "object_lock_enabled",
                    provider_name: "ObjectLockEnabled",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "rule",
                    provider_name: "Rule",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketAcl",
            attributes: &[
                SendCaptureAttribute {
                    name: "acl",
                    provider_name: "ACL",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketOwnershipControls",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "object_ownership",
                    provider_name: "ObjectOwnership",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketServerSideEncryptionConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "rules",
                    provider_name: "Rules",
                    capture_names: &["Rule"],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketReplicationConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "role",
                    provider_name: "Role",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "rules",
                    provider_name: "Rules",
                    capture_names: &["Rule"],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketLifecycleConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "rules",
                    provider_name: "Rules",
                    capture_names: &["Rule"],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketWebsiteConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "error_document",
                    provider_name: "ErrorDocument",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "index_document",
                    provider_name: "IndexDocument",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "redirect_all_requests_to",
                    provider_name: "RedirectAllRequestsTo",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketCorsConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "cors_rules",
                    provider_name: "CORSRules",
                    capture_names: &["CORSRule"],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketNotificationConfiguration",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "event_bridge_configuration",
                    provider_name: "EventBridgeConfiguration",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "lambda_function_configurations",
                    provider_name: "LambdaFunctionConfigurations",
                    capture_names: &["CloudFunctionConfiguration"],
                },
                SendCaptureAttribute {
                    name: "queue_configurations",
                    provider_name: "QueueConfigurations",
                    capture_names: &["QueueConfiguration"],
                },
                SendCaptureAttribute {
                    name: "topic_configurations",
                    provider_name: "TopicConfigurations",
                    capture_names: &["TopicConfiguration"],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketLogging",
            attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "target_bucket",
                    provider_name: "TargetBucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "target_object_key_format",
                    provider_name: "TargetObjectKeyFormat",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "target_prefix",
                    provider_name: "TargetPrefix",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "s3.BucketPublicAccessBlock",
            attributes: &[
                SendCaptureAttribute {
                    name: "block_public_acls",
                    provider_name: "BlockPublicAcls",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "block_public_policy",
                    provider_name: "BlockPublicPolicy",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ignore_public_acls",
                    provider_name: "IgnorePublicAcls",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "restrict_public_buckets",
                    provider_name: "RestrictPublicBuckets",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "organizations.Organization",
            attributes: &[SendCaptureAttribute {
                name: "feature_set",
                provider_name: "FeatureSet",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "organizations.Account",
            attributes: &[
                SendCaptureAttribute {
                    name: "account_name",
                    provider_name: "AccountName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "email",
                    provider_name: "Email",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "iam_user_access_to_billing",
                    provider_name: "IamUserAccessToBilling",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "role_name",
                    provider_name: "RoleName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "route53.RecordSet",
            attributes: &[
                SendCaptureAttribute {
                    name: "alias_target",
                    provider_name: "AliasTarget",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "hosted_zone_id",
                    provider_name: "HostedZoneId",
                    capture_names: &["hostedzone"],
                },
                SendCaptureAttribute {
                    name: "name",
                    provider_name: "Name",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "resource_records",
                    provider_name: "ResourceRecords",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "ttl",
                    provider_name: "TTL",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "type",
                    provider_name: "Type",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "iam.Role",
            attributes: &[
                SendCaptureAttribute {
                    name: "assume_role_policy_document",
                    provider_name: "AssumeRolePolicyDocument",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "max_session_duration",
                    provider_name: "MaxSessionDuration",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "path",
                    provider_name: "Path",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "role_name",
                    provider_name: "RoleName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "logs.LogGroup",
            attributes: &[
                SendCaptureAttribute {
                    name: "deletion_protection_enabled",
                    provider_name: "deletionProtectionEnabled",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "kms_key_id",
                    provider_name: "kmsKeyId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_group_class",
                    provider_name: "logGroupClass",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "log_group_name",
                    provider_name: "logGroupName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "retention_in_days",
                    provider_name: "retentionInDays",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "acm.Certificate",
            attributes: &[
                SendCaptureAttribute {
                    name: "domain_name",
                    provider_name: "DomainName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "key_algorithm",
                    provider_name: "KeyAlgorithm",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "options",
                    provider_name: "Options",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "subject_alternative_names",
                    provider_name: "SubjectAlternativeNames",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "validation_method",
                    provider_name: "ValidationMethod",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "sqs.Queue",
            attributes: &[
                SendCaptureAttribute {
                    name: "content_based_deduplication",
                    provider_name: "ContentBasedDeduplication",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "deduplication_scope",
                    provider_name: "DeduplicationScope",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "delay_seconds",
                    provider_name: "DelaySeconds",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "fifo_queue",
                    provider_name: "FifoQueue",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "fifo_throughput_limit",
                    provider_name: "FifoThroughputLimit",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "kms_data_key_reuse_period_seconds",
                    provider_name: "KmsDataKeyReusePeriodSeconds",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "kms_master_key_id",
                    provider_name: "KmsMasterKeyId",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "maximum_message_size",
                    provider_name: "MaximumMessageSize",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "message_retention_period",
                    provider_name: "MessageRetentionPeriod",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "policy",
                    provider_name: "Policy",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "queue_name",
                    provider_name: "QueueName",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "receive_message_wait_time_seconds",
                    provider_name: "ReceiveMessageWaitTimeSeconds",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "redrive_allow_policy",
                    provider_name: "RedriveAllowPolicy",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "redrive_policy",
                    provider_name: "RedrivePolicy",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "sqs_managed_sse_enabled",
                    provider_name: "SqsManagedSseEnabled",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "visibility_timeout",
                    provider_name: "VisibilityTimeout",
                    capture_names: &[],
                },
            ],
        },
    ]
}
