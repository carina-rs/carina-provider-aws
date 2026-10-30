//! Auto-generated exhaustive managed-resource send inputs.
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./scripts/generate-provider.sh

use std::collections::BTreeSet;

use carina_core::provider::{ProviderResult, Sent as CreateSent};
use carina_core::resource::{Resource, Value};

/// Exhaustive typed create input for `ec2.Vpc`.
pub(crate) struct Ec2VpcCreateInput {
    cidr_block: Option<String>,
    enable_dns_hostnames: Option<bool>,
    enable_dns_support: Option<bool>,
    instance_tenancy: Option<String>,
    ipv4_ipam_pool_id: Option<String>,
    ipv4_netmask_length: Option<i64>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2VpcCreateParts {
    pub(crate) cidr_block: crate::send_model::Sent<Option<String>>,
    pub(crate) enable_dns_hostnames: crate::send_model::Sent<Option<bool>>,
    pub(crate) enable_dns_support: crate::send_model::Sent<Option<bool>>,
    pub(crate) instance_tenancy: crate::send_model::Sent<Option<String>>,
    pub(crate) ipv4_ipam_pool_id: crate::send_model::Sent<Option<String>>,
    pub(crate) ipv4_netmask_length: crate::send_model::Sent<Option<i64>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.cidr_block.is_some() {
            sent.insert("cidr_block".to_string());
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2VpcCreateParts {
        let Self {
            cidr_block,
            enable_dns_hostnames,
            enable_dns_support,
            instance_tenancy,
            ipv4_ipam_pool_id,
            ipv4_netmask_length,
            tags,
        } = self;
        Ec2VpcCreateParts {
            cidr_block: crate::send_model::Sent::new("ec2.Vpc.create.cidr_block", cidr_block),
            enable_dns_hostnames: crate::send_model::Sent::new(
                "ec2.Vpc.create.enable_dns_hostnames",
                enable_dns_hostnames,
            ),
            enable_dns_support: crate::send_model::Sent::new(
                "ec2.Vpc.create.enable_dns_support",
                enable_dns_support,
            ),
            instance_tenancy: crate::send_model::Sent::new(
                "ec2.Vpc.create.instance_tenancy",
                instance_tenancy,
            ),
            ipv4_ipam_pool_id: crate::send_model::Sent::new(
                "ec2.Vpc.create.ipv4_ipam_pool_id",
                ipv4_ipam_pool_id,
            ),
            ipv4_netmask_length: crate::send_model::Sent::new(
                "ec2.Vpc.create.ipv4_netmask_length",
                ipv4_netmask_length,
            ),
            tags: crate::send_model::Sent::new("ec2.Vpc.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `ec2.Vpc`.
pub(crate) struct Ec2VpcUpdateInput {
    enable_dns_hostnames: Option<bool>,
    enable_dns_support: Option<bool>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2VpcUpdateParts {
    pub(crate) enable_dns_hostnames: crate::send_model::Sent<Option<bool>>,
    pub(crate) enable_dns_support: crate::send_model::Sent<Option<bool>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2VpcUpdateParts {
        let Self {
            enable_dns_hostnames,
            enable_dns_support,
            tags,
        } = self;
        Ec2VpcUpdateParts {
            enable_dns_hostnames: crate::send_model::Sent::new(
                "ec2.Vpc.update.enable_dns_hostnames",
                enable_dns_hostnames,
            ),
            enable_dns_support: crate::send_model::Sent::new(
                "ec2.Vpc.update.enable_dns_support",
                enable_dns_support,
            ),
            tags: crate::send_model::Sent::new("ec2.Vpc.update.tags", tags),
        }
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
    tags: Option<crate::send_model::Tags>,
    vpc_id: String,
}

pub(crate) struct Ec2SubnetCreateParts {
    pub(crate) assign_ipv6_address_on_creation: crate::send_model::Sent<Option<bool>>,
    pub(crate) availability_zone: crate::send_model::Sent<Option<String>>,
    pub(crate) availability_zone_id: crate::send_model::Sent<Option<String>>,
    pub(crate) cidr_block: crate::send_model::Sent<Option<String>>,
    pub(crate) enable_dns64: crate::send_model::Sent<Option<bool>>,
    pub(crate) enable_lni_at_device_index: crate::send_model::Sent<Option<i64>>,
    pub(crate) ipv4_ipam_pool_id: crate::send_model::Sent<Option<String>>,
    pub(crate) ipv4_netmask_length: crate::send_model::Sent<Option<i64>>,
    pub(crate) ipv6_cidr_block: crate::send_model::Sent<Option<String>>,
    pub(crate) ipv6_ipam_pool_id: crate::send_model::Sent<Option<String>>,
    pub(crate) ipv6_native: crate::send_model::Sent<Option<bool>>,
    pub(crate) ipv6_netmask_length: crate::send_model::Sent<Option<i64>>,
    pub(crate) map_public_ip_on_launch: crate::send_model::Sent<Option<bool>>,
    pub(crate) outpost_arn: crate::send_model::Sent<Option<String>>,
    pub(crate) private_dns_name_options_on_launch: crate::send_model::Sent<Option<Value>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.availability_zone.is_some() {
            sent.insert("availability_zone".to_string());
        }
        if self.availability_zone_id.is_some() {
            sent.insert("availability_zone_id".to_string());
        }
        if self.cidr_block.is_some() {
            sent.insert("cidr_block".to_string());
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
        if self.outpost_arn.is_some() {
            sent.insert("outpost_arn".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2SubnetCreateParts {
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
        Ec2SubnetCreateParts {
            assign_ipv6_address_on_creation: crate::send_model::Sent::new(
                "ec2.Subnet.create.assign_ipv6_address_on_creation",
                assign_ipv6_address_on_creation,
            ),
            availability_zone: crate::send_model::Sent::new(
                "ec2.Subnet.create.availability_zone",
                availability_zone,
            ),
            availability_zone_id: crate::send_model::Sent::new(
                "ec2.Subnet.create.availability_zone_id",
                availability_zone_id,
            ),
            cidr_block: crate::send_model::Sent::new("ec2.Subnet.create.cidr_block", cidr_block),
            enable_dns64: crate::send_model::Sent::new(
                "ec2.Subnet.create.enable_dns64",
                enable_dns64,
            ),
            enable_lni_at_device_index: crate::send_model::Sent::new(
                "ec2.Subnet.create.enable_lni_at_device_index",
                enable_lni_at_device_index,
            ),
            ipv4_ipam_pool_id: crate::send_model::Sent::new(
                "ec2.Subnet.create.ipv4_ipam_pool_id",
                ipv4_ipam_pool_id,
            ),
            ipv4_netmask_length: crate::send_model::Sent::new(
                "ec2.Subnet.create.ipv4_netmask_length",
                ipv4_netmask_length,
            ),
            ipv6_cidr_block: crate::send_model::Sent::new(
                "ec2.Subnet.create.ipv6_cidr_block",
                ipv6_cidr_block,
            ),
            ipv6_ipam_pool_id: crate::send_model::Sent::new(
                "ec2.Subnet.create.ipv6_ipam_pool_id",
                ipv6_ipam_pool_id,
            ),
            ipv6_native: crate::send_model::Sent::new("ec2.Subnet.create.ipv6_native", ipv6_native),
            ipv6_netmask_length: crate::send_model::Sent::new(
                "ec2.Subnet.create.ipv6_netmask_length",
                ipv6_netmask_length,
            ),
            map_public_ip_on_launch: crate::send_model::Sent::new(
                "ec2.Subnet.create.map_public_ip_on_launch",
                map_public_ip_on_launch,
            ),
            outpost_arn: crate::send_model::Sent::new("ec2.Subnet.create.outpost_arn", outpost_arn),
            private_dns_name_options_on_launch: crate::send_model::Sent::new(
                "ec2.Subnet.create.private_dns_name_options_on_launch",
                private_dns_name_options_on_launch,
            ),
            tags: crate::send_model::Sent::new("ec2.Subnet.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.Subnet.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.Subnet`.
pub(crate) struct Ec2SubnetUpdateInput {
    assign_ipv6_address_on_creation: Option<bool>,
    enable_dns64: Option<bool>,
    enable_lni_at_device_index: Option<i64>,
    map_public_ip_on_launch: Option<bool>,
    private_dns_name_options_on_launch: Option<Value>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2SubnetUpdateParts {
    pub(crate) assign_ipv6_address_on_creation: crate::send_model::Sent<Option<bool>>,
    pub(crate) enable_dns64: crate::send_model::Sent<Option<bool>>,
    pub(crate) enable_lni_at_device_index: crate::send_model::Sent<Option<i64>>,
    pub(crate) map_public_ip_on_launch: crate::send_model::Sent<Option<bool>>,
    pub(crate) private_dns_name_options_on_launch: crate::send_model::Sent<Option<Value>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2SubnetUpdateParts {
        let Self {
            assign_ipv6_address_on_creation,
            enable_dns64,
            enable_lni_at_device_index,
            map_public_ip_on_launch,
            private_dns_name_options_on_launch,
            tags,
        } = self;
        Ec2SubnetUpdateParts {
            assign_ipv6_address_on_creation: crate::send_model::Sent::new(
                "ec2.Subnet.update.assign_ipv6_address_on_creation",
                assign_ipv6_address_on_creation,
            ),
            enable_dns64: crate::send_model::Sent::new(
                "ec2.Subnet.update.enable_dns64",
                enable_dns64,
            ),
            enable_lni_at_device_index: crate::send_model::Sent::new(
                "ec2.Subnet.update.enable_lni_at_device_index",
                enable_lni_at_device_index,
            ),
            map_public_ip_on_launch: crate::send_model::Sent::new(
                "ec2.Subnet.update.map_public_ip_on_launch",
                map_public_ip_on_launch,
            ),
            private_dns_name_options_on_launch: crate::send_model::Sent::new(
                "ec2.Subnet.update.private_dns_name_options_on_launch",
                private_dns_name_options_on_launch,
            ),
            tags: crate::send_model::Sent::new("ec2.Subnet.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.InternetGateway`.
pub(crate) struct Ec2InternetGatewayCreateInput {
    tags: Option<crate::send_model::Tags>,
    vpc_id: Option<String>,
}

pub(crate) struct Ec2InternetGatewayCreateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<Option<String>>,
}

impl Ec2InternetGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::internet_gateway::ec2_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2InternetGatewayCreateParts {
        let Self { tags, vpc_id } = self;
        Ec2InternetGatewayCreateParts {
            tags: crate::send_model::Sent::new("ec2.InternetGateway.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.InternetGateway.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.InternetGateway`.
pub(crate) struct Ec2InternetGatewayUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2InternetGatewayUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2InternetGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::internet_gateway::ec2_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2InternetGatewayUpdateParts {
        let Self { tags } = self;
        Ec2InternetGatewayUpdateParts {
            tags: crate::send_model::Sent::new("ec2.InternetGateway.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.RouteTable`.
pub(crate) struct Ec2RouteTableCreateInput {
    tags: Option<crate::send_model::Tags>,
    vpc_id: String,
}

pub(crate) struct Ec2RouteTableCreateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
}

impl Ec2RouteTableCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route_table::ec2_route_table_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2RouteTableCreateParts {
        let Self { tags, vpc_id } = self;
        Ec2RouteTableCreateParts {
            tags: crate::send_model::Sent::new("ec2.RouteTable.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.RouteTable.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.RouteTable`.
pub(crate) struct Ec2RouteTableUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2RouteTableUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2RouteTableUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route_table::ec2_route_table_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2RouteTableUpdateParts {
        let Self { tags } = self;
        Ec2RouteTableUpdateParts {
            tags: crate::send_model::Sent::new("ec2.RouteTable.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.Route`.
pub(crate) struct Ec2RouteCreateInput {
    destination_cidr_block: Option<String>,
    gateway_id: Option<String>,
    nat_gateway_id: Option<String>,
    route_table_id: String,
}

pub(crate) struct Ec2RouteCreateParts {
    pub(crate) destination_cidr_block: crate::send_model::Sent<Option<String>>,
    pub(crate) gateway_id: crate::send_model::Sent<Option<String>>,
    pub(crate) nat_gateway_id: crate::send_model::Sent<Option<String>>,
    pub(crate) route_table_id: crate::send_model::Sent<String>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2RouteCreateParts {
        let Self {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            route_table_id,
        } = self;
        Ec2RouteCreateParts {
            destination_cidr_block: crate::send_model::Sent::new(
                "ec2.Route.create.destination_cidr_block",
                destination_cidr_block,
            ),
            gateway_id: crate::send_model::Sent::new("ec2.Route.create.gateway_id", gateway_id),
            nat_gateway_id: crate::send_model::Sent::new(
                "ec2.Route.create.nat_gateway_id",
                nat_gateway_id,
            ),
            route_table_id: crate::send_model::Sent::new(
                "ec2.Route.create.route_table_id",
                route_table_id,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.Route`.
pub(crate) struct Ec2RouteUpdateInput {
    gateway_id: Option<String>,
    nat_gateway_id: Option<String>,
}

pub(crate) struct Ec2RouteUpdateParts {
    pub(crate) gateway_id: crate::send_model::Sent<Option<String>>,
    pub(crate) nat_gateway_id: crate::send_model::Sent<Option<String>>,
}

impl Ec2RouteUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::route::ec2_route_config().schema;
        Ok(Self {
            gateway_id: crate::send_model::optional_string(_resource, "gateway_id")?,
            nat_gateway_id: crate::send_model::optional_string(_resource, "nat_gateway_id")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2RouteUpdateParts {
        let Self {
            gateway_id,
            nat_gateway_id,
        } = self;
        Ec2RouteUpdateParts {
            gateway_id: crate::send_model::Sent::new("ec2.Route.update.gateway_id", gateway_id),
            nat_gateway_id: crate::send_model::Sent::new(
                "ec2.Route.update.nat_gateway_id",
                nat_gateway_id,
            ),
        }
    }
}

/// Exhaustive typed create input for `ec2.SecurityGroup`.
pub(crate) struct Ec2SecurityGroupCreateInput {
    description: String,
    group_name: String,
    tags: Option<crate::send_model::Tags>,
    vpc_id: Option<String>,
}

pub(crate) struct Ec2SecurityGroupCreateParts {
    pub(crate) description: crate::send_model::Sent<String>,
    pub(crate) group_name: crate::send_model::Sent<String>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<Option<String>>,
}

impl Ec2SecurityGroupCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::security_group::ec2_security_group_config().schema;
        Ok(Self {
            description: crate::send_model::required_string(_resource, "description")?,
            group_name: crate::send_model::required_string(_resource, "group_name")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("description".to_string());
        sent.insert("group_name".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.vpc_id.is_some() {
            sent.insert("vpc_id".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupCreateParts {
        let Self {
            description,
            group_name,
            tags,
            vpc_id,
        } = self;
        Ec2SecurityGroupCreateParts {
            description: crate::send_model::Sent::new(
                "ec2.SecurityGroup.create.description",
                description,
            ),
            group_name: crate::send_model::Sent::new(
                "ec2.SecurityGroup.create.group_name",
                group_name,
            ),
            tags: crate::send_model::Sent::new("ec2.SecurityGroup.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.SecurityGroup.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroup`.
pub(crate) struct Ec2SecurityGroupUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2SecurityGroupUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2SecurityGroupUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::security_group::ec2_security_group_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupUpdateParts {
        let Self { tags } = self;
        Ec2SecurityGroupUpdateParts {
            tags: crate::send_model::Sent::new("ec2.SecurityGroup.update.tags", tags),
        }
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

pub(crate) struct Ec2SecurityGroupIngressCreateParts {
    pub(crate) cidr_ip: crate::send_model::Sent<Option<String>>,
    pub(crate) cidr_ipv6: crate::send_model::Sent<Option<String>>,
    pub(crate) description: crate::send_model::Sent<Option<String>>,
    pub(crate) from_port: crate::send_model::Sent<Option<i64>>,
    pub(crate) group_id: crate::send_model::Sent<Option<String>>,
    pub(crate) ip_protocol: crate::send_model::Sent<String>,
    pub(crate) source_prefix_list_id: crate::send_model::Sent<Option<String>>,
    pub(crate) source_security_group_id: crate::send_model::Sent<Option<String>>,
    pub(crate) source_security_group_owner_id: crate::send_model::Sent<Option<String>>,
    pub(crate) to_port: crate::send_model::Sent<Option<i64>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupIngressCreateParts {
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
        Ec2SecurityGroupIngressCreateParts {
            cidr_ip: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.cidr_ip",
                cidr_ip,
            ),
            cidr_ipv6: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.cidr_ipv6",
                cidr_ipv6,
            ),
            description: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.description",
                description,
            ),
            from_port: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.from_port",
                from_port,
            ),
            group_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.group_id",
                group_id,
            ),
            ip_protocol: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.ip_protocol",
                ip_protocol,
            ),
            source_prefix_list_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.source_prefix_list_id",
                source_prefix_list_id,
            ),
            source_security_group_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.source_security_group_id",
                source_security_group_id,
            ),
            source_security_group_owner_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.source_security_group_owner_id",
                source_security_group_owner_id,
            ),
            to_port: crate::send_model::Sent::new(
                "ec2.SecurityGroupIngress.create.to_port",
                to_port,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroupIngress`.
pub(crate) struct Ec2SecurityGroupIngressUpdateInput;

pub(crate) struct Ec2SecurityGroupIngressUpdateParts;

impl Ec2SecurityGroupIngressUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupIngressUpdateParts {
        let Self = self;
        Ec2SecurityGroupIngressUpdateParts
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

pub(crate) struct Ec2SecurityGroupEgressCreateParts {
    pub(crate) cidr_ip: crate::send_model::Sent<Option<String>>,
    pub(crate) cidr_ipv6: crate::send_model::Sent<Option<String>>,
    pub(crate) description: crate::send_model::Sent<Option<String>>,
    pub(crate) destination_prefix_list_id: crate::send_model::Sent<Option<String>>,
    pub(crate) destination_security_group_id: crate::send_model::Sent<Option<String>>,
    pub(crate) from_port: crate::send_model::Sent<Option<i64>>,
    pub(crate) group_id: crate::send_model::Sent<String>,
    pub(crate) ip_protocol: crate::send_model::Sent<String>,
    pub(crate) source_security_group_owner_id: crate::send_model::Sent<Option<String>>,
    pub(crate) to_port: crate::send_model::Sent<Option<i64>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupEgressCreateParts {
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
        Ec2SecurityGroupEgressCreateParts {
            cidr_ip: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.cidr_ip",
                cidr_ip,
            ),
            cidr_ipv6: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.cidr_ipv6",
                cidr_ipv6,
            ),
            description: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.description",
                description,
            ),
            destination_prefix_list_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.destination_prefix_list_id",
                destination_prefix_list_id,
            ),
            destination_security_group_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.destination_security_group_id",
                destination_security_group_id,
            ),
            from_port: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.from_port",
                from_port,
            ),
            group_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.group_id",
                group_id,
            ),
            ip_protocol: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.ip_protocol",
                ip_protocol,
            ),
            source_security_group_owner_id: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.source_security_group_owner_id",
                source_security_group_owner_id,
            ),
            to_port: crate::send_model::Sent::new(
                "ec2.SecurityGroupEgress.create.to_port",
                to_port,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.SecurityGroupEgress`.
pub(crate) struct Ec2SecurityGroupEgressUpdateInput;

pub(crate) struct Ec2SecurityGroupEgressUpdateParts;

impl Ec2SecurityGroupEgressUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::security_group_egress::ec2_security_group_egress_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) -> Ec2SecurityGroupEgressUpdateParts {
        let Self = self;
        Ec2SecurityGroupEgressUpdateParts
    }
}

/// Exhaustive typed create input for `ec2.EgressOnlyInternetGateway`.
pub(crate) struct Ec2EgressOnlyInternetGatewayCreateInput {
    tags: Option<crate::send_model::Tags>,
    vpc_id: String,
}

pub(crate) struct Ec2EgressOnlyInternetGatewayCreateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
}

impl Ec2EgressOnlyInternetGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2EgressOnlyInternetGatewayCreateParts {
        let Self { tags, vpc_id } = self;
        Ec2EgressOnlyInternetGatewayCreateParts {
            tags: crate::send_model::Sent::new("ec2.EgressOnlyInternetGateway.create.tags", tags),
            vpc_id: crate::send_model::Sent::new(
                "ec2.EgressOnlyInternetGateway.create.vpc_id",
                vpc_id,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.EgressOnlyInternetGateway`.
pub(crate) struct Ec2EgressOnlyInternetGatewayUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2EgressOnlyInternetGatewayUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2EgressOnlyInternetGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2EgressOnlyInternetGatewayUpdateParts {
        let Self { tags } = self;
        Ec2EgressOnlyInternetGatewayUpdateParts {
            tags: crate::send_model::Sent::new("ec2.EgressOnlyInternetGateway.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.Eip`.
pub(crate) struct Ec2EipCreateInput {
    address: Option<String>,
    domain: Option<String>,
    public_ipv4_pool: Option<String>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2EipCreateParts {
    pub(crate) address: crate::send_model::Sent<Option<String>>,
    pub(crate) domain: crate::send_model::Sent<Option<String>>,
    pub(crate) public_ipv4_pool: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2EipCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::eip::ec2_eip_config().schema;
        Ok(Self {
            address: crate::send_model::optional_string(_resource, "address")?,
            domain: crate::send_model::optional_enum(_resource, _schema, "domain")?,
            public_ipv4_pool: crate::send_model::optional_string(_resource, "public_ipv4_pool")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2EipCreateParts {
        let Self {
            address,
            domain,
            public_ipv4_pool,
            tags,
        } = self;
        Ec2EipCreateParts {
            address: crate::send_model::Sent::new("ec2.Eip.create.address", address),
            domain: crate::send_model::Sent::new("ec2.Eip.create.domain", domain),
            public_ipv4_pool: crate::send_model::Sent::new(
                "ec2.Eip.create.public_ipv4_pool",
                public_ipv4_pool,
            ),
            tags: crate::send_model::Sent::new("ec2.Eip.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `ec2.Eip`.
pub(crate) struct Ec2EipUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2EipUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2EipUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::eip::ec2_eip_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2EipUpdateParts {
        let Self { tags } = self;
        Ec2EipUpdateParts {
            tags: crate::send_model::Sent::new("ec2.Eip.update.tags", tags),
        }
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
    resource_ids: crate::send_model::StringList,
    resource_type: String,
    tag_field_specifications: Option<Value>,
    tags: Option<crate::send_model::Tags>,
    traffic_type: Option<String>,
}

pub(crate) struct Ec2FlowLogCreateParts {
    pub(crate) deliver_logs_permission_arn: crate::send_model::Sent<Option<String>>,
    pub(crate) log_destination: crate::send_model::Sent<Option<String>>,
    pub(crate) log_destination_type: crate::send_model::Sent<Option<String>>,
    pub(crate) log_format: crate::send_model::Sent<Option<String>>,
    pub(crate) log_group_name: crate::send_model::Sent<Option<String>>,
    pub(crate) max_aggregation_interval: crate::send_model::Sent<Option<i64>>,
    pub(crate) resource_ids: crate::send_model::Sent<crate::send_model::StringList>,
    pub(crate) resource_type: crate::send_model::Sent<String>,
    pub(crate) tag_field_specifications: crate::send_model::Sent<Option<Value>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) traffic_type: crate::send_model::Sent<Option<String>>,
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
            resource_ids: crate::send_model::required_string_list(_resource, "resource_ids")?,
            resource_type: crate::send_model::required_enum(_resource, _schema, "resource_type")?,
            tag_field_specifications: crate::send_model::optional_value(
                _resource,
                "tag_field_specifications",
            )?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            traffic_type: crate::send_model::optional_enum(_resource, _schema, "traffic_type")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        if self.tag_field_specifications.is_some() {
            sent.insert("tag_field_specifications".to_string());
        }
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        if self.traffic_type.is_some() {
            sent.insert("traffic_type".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2FlowLogCreateParts {
        let Self {
            deliver_logs_permission_arn,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            resource_ids,
            resource_type,
            tag_field_specifications,
            tags,
            traffic_type,
        } = self;
        Ec2FlowLogCreateParts {
            deliver_logs_permission_arn: crate::send_model::Sent::new(
                "ec2.FlowLog.create.deliver_logs_permission_arn",
                deliver_logs_permission_arn,
            ),
            log_destination: crate::send_model::Sent::new(
                "ec2.FlowLog.create.log_destination",
                log_destination,
            ),
            log_destination_type: crate::send_model::Sent::new(
                "ec2.FlowLog.create.log_destination_type",
                log_destination_type,
            ),
            log_format: crate::send_model::Sent::new("ec2.FlowLog.create.log_format", log_format),
            log_group_name: crate::send_model::Sent::new(
                "ec2.FlowLog.create.log_group_name",
                log_group_name,
            ),
            max_aggregation_interval: crate::send_model::Sent::new(
                "ec2.FlowLog.create.max_aggregation_interval",
                max_aggregation_interval,
            ),
            resource_ids: crate::send_model::Sent::new(
                "ec2.FlowLog.create.resource_ids",
                resource_ids,
            ),
            resource_type: crate::send_model::Sent::new(
                "ec2.FlowLog.create.resource_type",
                resource_type,
            ),
            tag_field_specifications: crate::send_model::Sent::new(
                "ec2.FlowLog.create.tag_field_specifications",
                tag_field_specifications,
            ),
            tags: crate::send_model::Sent::new("ec2.FlowLog.create.tags", tags),
            traffic_type: crate::send_model::Sent::new(
                "ec2.FlowLog.create.traffic_type",
                traffic_type,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.FlowLog`.
pub(crate) struct Ec2FlowLogUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2FlowLogUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2FlowLogUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::flow_log::ec2_flow_log_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2FlowLogUpdateParts {
        let Self { tags } = self;
        Ec2FlowLogUpdateParts {
            tags: crate::send_model::Sent::new("ec2.FlowLog.update.tags", tags),
        }
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
    tags: Option<crate::send_model::Tags>,
    vpc_id: Option<String>,
}

pub(crate) struct Ec2NatGatewayCreateParts {
    pub(crate) allocation_id: crate::send_model::Sent<Option<String>>,
    pub(crate) availability_mode: crate::send_model::Sent<Option<String>>,
    pub(crate) availability_zone_addresses: crate::send_model::Sent<Option<Value>>,
    pub(crate) connectivity_type: crate::send_model::Sent<Option<String>>,
    pub(crate) private_ip_address: crate::send_model::Sent<Option<String>>,
    pub(crate) subnet_id: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<Option<String>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::optional_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2NatGatewayCreateParts {
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
        Ec2NatGatewayCreateParts {
            allocation_id: crate::send_model::Sent::new(
                "ec2.NatGateway.create.allocation_id",
                allocation_id,
            ),
            availability_mode: crate::send_model::Sent::new(
                "ec2.NatGateway.create.availability_mode",
                availability_mode,
            ),
            availability_zone_addresses: crate::send_model::Sent::new(
                "ec2.NatGateway.create.availability_zone_addresses",
                availability_zone_addresses,
            ),
            connectivity_type: crate::send_model::Sent::new(
                "ec2.NatGateway.create.connectivity_type",
                connectivity_type,
            ),
            private_ip_address: crate::send_model::Sent::new(
                "ec2.NatGateway.create.private_ip_address",
                private_ip_address,
            ),
            subnet_id: crate::send_model::Sent::new("ec2.NatGateway.create.subnet_id", subnet_id),
            tags: crate::send_model::Sent::new("ec2.NatGateway.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.NatGateway.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.NatGateway`.
pub(crate) struct Ec2NatGatewayUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2NatGatewayUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2NatGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2NatGatewayUpdateParts {
        let Self { tags } = self;
        Ec2NatGatewayUpdateParts {
            tags: crate::send_model::Sent::new("ec2.NatGateway.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.SubnetRouteTableAssociation`.
pub(crate) struct Ec2SubnetRouteTableAssociationCreateInput {
    route_table_id: String,
    subnet_id: String,
}

pub(crate) struct Ec2SubnetRouteTableAssociationCreateParts {
    pub(crate) route_table_id: crate::send_model::Sent<String>,
    pub(crate) subnet_id: crate::send_model::Sent<String>,
}

impl Ec2SubnetRouteTableAssociationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet_route_table_association::ec2_subnet_route_table_association_config().schema;
        Ok(Self {
            route_table_id: crate::send_model::required_string(_resource, "route_table_id")?,
            subnet_id: crate::send_model::required_string(_resource, "subnet_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("route_table_id".to_string());
        sent.insert("subnet_id".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2SubnetRouteTableAssociationCreateParts {
        let Self {
            route_table_id,
            subnet_id,
        } = self;
        Ec2SubnetRouteTableAssociationCreateParts {
            route_table_id: crate::send_model::Sent::new(
                "ec2.SubnetRouteTableAssociation.create.route_table_id",
                route_table_id,
            ),
            subnet_id: crate::send_model::Sent::new(
                "ec2.SubnetRouteTableAssociation.create.subnet_id",
                subnet_id,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.SubnetRouteTableAssociation`.
pub(crate) struct Ec2SubnetRouteTableAssociationUpdateInput;

pub(crate) struct Ec2SubnetRouteTableAssociationUpdateParts;

impl Ec2SubnetRouteTableAssociationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::subnet_route_table_association::ec2_subnet_route_table_association_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) -> Ec2SubnetRouteTableAssociationUpdateParts {
        let Self = self;
        Ec2SubnetRouteTableAssociationUpdateParts
    }
}

/// Exhaustive typed create input for `ec2.TransitGateway`.
pub(crate) struct Ec2TransitGatewayCreateInput {
    description: Option<String>,
    options: Option<Value>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2TransitGatewayCreateParts {
    pub(crate) description: crate::send_model::Sent<Option<String>>,
    pub(crate) options: crate::send_model::Sent<Option<Value>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2TransitGatewayCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema;
        Ok(Self {
            description: crate::send_model::optional_string(_resource, "description")?,
            options: crate::send_model::optional_value(_resource, "options")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2TransitGatewayCreateParts {
        let Self {
            description,
            options,
            tags,
        } = self;
        Ec2TransitGatewayCreateParts {
            description: crate::send_model::Sent::new(
                "ec2.TransitGateway.create.description",
                description,
            ),
            options: crate::send_model::Sent::new("ec2.TransitGateway.create.options", options),
            tags: crate::send_model::Sent::new("ec2.TransitGateway.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `ec2.TransitGateway`.
pub(crate) struct Ec2TransitGatewayUpdateInput {
    description: Option<String>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2TransitGatewayUpdateParts {
    pub(crate) description: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2TransitGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema;
        Ok(Self {
            description: crate::send_model::optional_string(_resource, "description")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2TransitGatewayUpdateParts {
        let Self { description, tags } = self;
        Ec2TransitGatewayUpdateParts {
            description: crate::send_model::Sent::new(
                "ec2.TransitGateway.update.description",
                description,
            ),
            tags: crate::send_model::Sent::new("ec2.TransitGateway.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.TransitGatewayAttachment`.
pub(crate) struct Ec2TransitGatewayAttachmentCreateInput {
    options: Option<Value>,
    subnet_ids: crate::send_model::StringList,
    tags: Option<crate::send_model::Tags>,
    transit_gateway_id: String,
    vpc_id: String,
}

pub(crate) struct Ec2TransitGatewayAttachmentCreateParts {
    pub(crate) options: crate::send_model::Sent<Option<Value>>,
    pub(crate) subnet_ids: crate::send_model::Sent<crate::send_model::StringList>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) transit_gateway_id: crate::send_model::Sent<String>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
}

impl Ec2TransitGatewayAttachmentCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema;
        Ok(Self {
            options: crate::send_model::optional_value(_resource, "options")?,
            subnet_ids: crate::send_model::required_string_list(_resource, "subnet_ids")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            transit_gateway_id: crate::send_model::required_string(
                _resource,
                "transit_gateway_id",
            )?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2TransitGatewayAttachmentCreateParts {
        let Self {
            options,
            subnet_ids,
            tags,
            transit_gateway_id,
            vpc_id,
        } = self;
        Ec2TransitGatewayAttachmentCreateParts {
            options: crate::send_model::Sent::new(
                "ec2.TransitGatewayAttachment.create.options",
                options,
            ),
            subnet_ids: crate::send_model::Sent::new(
                "ec2.TransitGatewayAttachment.create.subnet_ids",
                subnet_ids,
            ),
            tags: crate::send_model::Sent::new("ec2.TransitGatewayAttachment.create.tags", tags),
            transit_gateway_id: crate::send_model::Sent::new(
                "ec2.TransitGatewayAttachment.create.transit_gateway_id",
                transit_gateway_id,
            ),
            vpc_id: crate::send_model::Sent::new(
                "ec2.TransitGatewayAttachment.create.vpc_id",
                vpc_id,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.TransitGatewayAttachment`.
pub(crate) struct Ec2TransitGatewayAttachmentUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2TransitGatewayAttachmentUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2TransitGatewayAttachmentUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2TransitGatewayAttachmentUpdateParts {
        let Self { tags } = self;
        Ec2TransitGatewayAttachmentUpdateParts {
            tags: crate::send_model::Sent::new("ec2.TransitGatewayAttachment.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.VpcEndpoint`.
pub(crate) struct Ec2VpcEndpointCreateInput {
    policy_document: Option<Value>,
    private_dns_enabled: Option<bool>,
    resource_configuration_arn: Option<String>,
    route_table_ids: Option<crate::send_model::StringList>,
    security_group_ids: Option<crate::send_model::StringList>,
    service_name: String,
    service_network_arn: Option<String>,
    service_region: Option<String>,
    subnet_ids: Option<crate::send_model::StringList>,
    tags: Option<crate::send_model::Tags>,
    vpc_endpoint_type: Option<String>,
    vpc_id: String,
}

pub(crate) struct Ec2VpcEndpointCreateParts {
    pub(crate) policy_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) private_dns_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) resource_configuration_arn: crate::send_model::Sent<Option<String>>,
    pub(crate) route_table_ids: crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) security_group_ids: crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) service_name: crate::send_model::Sent<String>,
    pub(crate) service_network_arn: crate::send_model::Sent<Option<String>>,
    pub(crate) service_region: crate::send_model::Sent<Option<String>>,
    pub(crate) subnet_ids: crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_endpoint_type: crate::send_model::Sent<Option<String>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
}

impl Ec2VpcEndpointCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::vpc_endpoint::ec2_vpc_endpoint_config().schema;
        Ok(Self {
            policy_document: crate::send_model::optional_value(_resource, "policy_document")?,
            private_dns_enabled: crate::send_model::optional_bool(
                _resource,
                "private_dns_enabled",
            )?,
            resource_configuration_arn: crate::send_model::optional_string(
                _resource,
                "resource_configuration_arn",
            )?,
            route_table_ids: crate::send_model::optional_string_list(_resource, "route_table_ids")?,
            security_group_ids: crate::send_model::optional_string_list(
                _resource,
                "security_group_ids",
            )?,
            service_name: crate::send_model::required_string(_resource, "service_name")?,
            service_network_arn: crate::send_model::optional_string(
                _resource,
                "service_network_arn",
            )?,
            service_region: crate::send_model::optional_enum(_resource, _schema, "service_region")?,
            subnet_ids: crate::send_model::optional_string_list(_resource, "subnet_ids")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_endpoint_type: crate::send_model::optional_enum(
                _resource,
                _schema,
                "vpc_endpoint_type",
            )?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2VpcEndpointCreateParts {
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
        Ec2VpcEndpointCreateParts {
            policy_document: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.policy_document",
                policy_document,
            ),
            private_dns_enabled: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.private_dns_enabled",
                private_dns_enabled,
            ),
            resource_configuration_arn: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.resource_configuration_arn",
                resource_configuration_arn,
            ),
            route_table_ids: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.route_table_ids",
                route_table_ids,
            ),
            security_group_ids: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.security_group_ids",
                security_group_ids,
            ),
            service_name: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.service_name",
                service_name,
            ),
            service_network_arn: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.service_network_arn",
                service_network_arn,
            ),
            service_region: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.service_region",
                service_region,
            ),
            subnet_ids: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.subnet_ids",
                subnet_ids,
            ),
            tags: crate::send_model::Sent::new("ec2.VpcEndpoint.create.tags", tags),
            vpc_endpoint_type: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.create.vpc_endpoint_type",
                vpc_endpoint_type,
            ),
            vpc_id: crate::send_model::Sent::new("ec2.VpcEndpoint.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.VpcEndpoint`.
pub(crate) struct Ec2VpcEndpointUpdateInput {
    policy_document: Option<Value>,
    private_dns_enabled: Option<bool>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2VpcEndpointUpdateParts {
    pub(crate) policy_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) private_dns_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2VpcEndpointUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::ec2::vpc_endpoint::ec2_vpc_endpoint_config().schema;
        Ok(Self {
            policy_document: crate::send_model::optional_value(_resource, "policy_document")?,
            private_dns_enabled: crate::send_model::optional_bool(
                _resource,
                "private_dns_enabled",
            )?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2VpcEndpointUpdateParts {
        let Self {
            policy_document,
            private_dns_enabled,
            tags,
        } = self;
        Ec2VpcEndpointUpdateParts {
            policy_document: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.update.policy_document",
                policy_document,
            ),
            private_dns_enabled: crate::send_model::Sent::new(
                "ec2.VpcEndpoint.update.private_dns_enabled",
                private_dns_enabled,
            ),
            tags: crate::send_model::Sent::new("ec2.VpcEndpoint.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.VpcGatewayAttachment`.
pub(crate) struct Ec2VpcGatewayAttachmentCreateInput {
    internet_gateway_id: Option<String>,
    vpc_id: String,
    vpn_gateway_id: Option<String>,
}

pub(crate) struct Ec2VpcGatewayAttachmentCreateParts {
    pub(crate) internet_gateway_id: crate::send_model::Sent<Option<String>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
    pub(crate) vpn_gateway_id: crate::send_model::Sent<Option<String>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.internet_gateway_id.is_some() {
            sent.insert("internet_gateway_id".to_string());
        }
        sent.insert("vpc_id".to_string());
        if self.vpn_gateway_id.is_some() {
            sent.insert("vpn_gateway_id".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2VpcGatewayAttachmentCreateParts {
        let Self {
            internet_gateway_id,
            vpc_id,
            vpn_gateway_id,
        } = self;
        Ec2VpcGatewayAttachmentCreateParts {
            internet_gateway_id: crate::send_model::Sent::new(
                "ec2.VpcGatewayAttachment.create.internet_gateway_id",
                internet_gateway_id,
            ),
            vpc_id: crate::send_model::Sent::new("ec2.VpcGatewayAttachment.create.vpc_id", vpc_id),
            vpn_gateway_id: crate::send_model::Sent::new(
                "ec2.VpcGatewayAttachment.create.vpn_gateway_id",
                vpn_gateway_id,
            ),
        }
    }
}

/// Exhaustive typed update input for `ec2.VpcGatewayAttachment`.
pub(crate) struct Ec2VpcGatewayAttachmentUpdateInput;

pub(crate) struct Ec2VpcGatewayAttachmentUpdateParts;

impl Ec2VpcGatewayAttachmentUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) -> Ec2VpcGatewayAttachmentUpdateParts {
        let Self = self;
        Ec2VpcGatewayAttachmentUpdateParts
    }
}

/// Exhaustive typed create input for `ec2.VpcPeeringConnection`.
pub(crate) struct Ec2VpcPeeringConnectionCreateInput {
    peer_owner_id: Option<String>,
    peer_vpc_id: String,
    tags: Option<crate::send_model::Tags>,
    vpc_id: String,
}

pub(crate) struct Ec2VpcPeeringConnectionCreateParts {
    pub(crate) peer_owner_id: crate::send_model::Sent<Option<String>>,
    pub(crate) peer_vpc_id: crate::send_model::Sent<String>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) vpc_id: crate::send_model::Sent<String>,
}

impl Ec2VpcPeeringConnectionCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config().schema;
        Ok(Self {
            peer_owner_id: crate::send_model::optional_string(_resource, "peer_owner_id")?,
            peer_vpc_id: crate::send_model::required_string(_resource, "peer_vpc_id")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            vpc_id: crate::send_model::required_string(_resource, "vpc_id")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.peer_owner_id.is_some() {
            sent.insert("peer_owner_id".to_string());
        }
        sent.insert("peer_vpc_id".to_string());
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        sent.insert("vpc_id".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2VpcPeeringConnectionCreateParts {
        let Self {
            peer_owner_id,
            peer_vpc_id,
            tags,
            vpc_id,
        } = self;
        Ec2VpcPeeringConnectionCreateParts {
            peer_owner_id: crate::send_model::Sent::new(
                "ec2.VpcPeeringConnection.create.peer_owner_id",
                peer_owner_id,
            ),
            peer_vpc_id: crate::send_model::Sent::new(
                "ec2.VpcPeeringConnection.create.peer_vpc_id",
                peer_vpc_id,
            ),
            tags: crate::send_model::Sent::new("ec2.VpcPeeringConnection.create.tags", tags),
            vpc_id: crate::send_model::Sent::new("ec2.VpcPeeringConnection.create.vpc_id", vpc_id),
        }
    }
}

/// Exhaustive typed update input for `ec2.VpcPeeringConnection`.
pub(crate) struct Ec2VpcPeeringConnectionUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2VpcPeeringConnectionUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2VpcPeeringConnectionUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2VpcPeeringConnectionUpdateParts {
        let Self { tags } = self;
        Ec2VpcPeeringConnectionUpdateParts {
            tags: crate::send_model::Sent::new("ec2.VpcPeeringConnection.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `ec2.VpnGateway`.
pub(crate) struct Ec2VpnGatewayCreateInput {
    amazon_side_asn: Option<i64>,
    availability_zone: Option<String>,
    tags: Option<crate::send_model::Tags>,
    r#type: String,
}

pub(crate) struct Ec2VpnGatewayCreateParts {
    pub(crate) amazon_side_asn: crate::send_model::Sent<Option<i64>>,
    pub(crate) availability_zone: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) r#type: crate::send_model::Sent<String>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Ec2VpnGatewayCreateParts {
        let Self {
            amazon_side_asn,
            availability_zone,
            tags,
            r#type,
        } = self;
        Ec2VpnGatewayCreateParts {
            amazon_side_asn: crate::send_model::Sent::new(
                "ec2.VpnGateway.create.amazon_side_asn",
                amazon_side_asn,
            ),
            availability_zone: crate::send_model::Sent::new(
                "ec2.VpnGateway.create.availability_zone",
                availability_zone,
            ),
            tags: crate::send_model::Sent::new("ec2.VpnGateway.create.tags", tags),
            r#type: crate::send_model::Sent::new("ec2.VpnGateway.create.type", r#type),
        }
    }
}

/// Exhaustive typed update input for `ec2.VpnGateway`.
pub(crate) struct Ec2VpnGatewayUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct Ec2VpnGatewayUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl Ec2VpnGatewayUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::ec2::vpn_gateway::ec2_vpn_gateway_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> Ec2VpnGatewayUpdateParts {
        let Self { tags } = self;
        Ec2VpnGatewayUpdateParts {
            tags: crate::send_model::Sent::new("ec2.VpnGateway.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `s3.Bucket`.
pub(crate) struct S3BucketCreateInput {
    bucket: String,
    bucket_namespace: Option<String>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct S3BucketCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) bucket_namespace: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        if self.bucket_namespace.is_some() {
            sent.insert("bucket_namespace".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketCreateParts {
        let Self {
            bucket,
            bucket_namespace,
            tags,
        } = self;
        S3BucketCreateParts {
            bucket: crate::send_model::Sent::new("s3.Bucket.create.bucket", bucket),
            bucket_namespace: crate::send_model::Sent::new(
                "s3.Bucket.create.bucket_namespace",
                bucket_namespace,
            ),
            tags: crate::send_model::Sent::new("s3.Bucket.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `s3.Bucket`.
pub(crate) struct S3BucketUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct S3BucketUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl S3BucketUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket::s3_bucket_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketUpdateParts {
        let Self { tags } = self;
        S3BucketUpdateParts {
            tags: crate::send_model::Sent::new("s3.Bucket.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketPolicy`.
pub(crate) struct S3BucketPolicyCreateInput {
    bucket: String,
    policy: Value,
}

pub(crate) struct S3BucketPolicyCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) policy: crate::send_model::Sent<Value>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("policy".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketPolicyCreateParts {
        let Self { bucket, policy } = self;
        S3BucketPolicyCreateParts {
            bucket: crate::send_model::Sent::new("s3.BucketPolicy.create.bucket", bucket),
            policy: crate::send_model::Sent::new("s3.BucketPolicy.create.policy", policy),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketPolicy`.
pub(crate) struct S3BucketPolicyUpdateInput {
    policy: Value,
}

pub(crate) struct S3BucketPolicyUpdateParts {
    pub(crate) policy: crate::send_model::Sent<Value>,
}

impl S3BucketPolicyUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_policy::s3_bucket_policy_config().schema;
        Ok(Self {
            policy: crate::send_model::required_value(_resource, "policy")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketPolicyUpdateParts {
        let Self { policy } = self;
        S3BucketPolicyUpdateParts {
            policy: crate::send_model::Sent::new("s3.BucketPolicy.update.policy", policy),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketVersioning`.
pub(crate) struct S3BucketVersioningCreateInput {
    bucket: String,
    status: String,
}

pub(crate) struct S3BucketVersioningCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) status: crate::send_model::Sent<String>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("status".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketVersioningCreateParts {
        let Self { bucket, status } = self;
        S3BucketVersioningCreateParts {
            bucket: crate::send_model::Sent::new("s3.BucketVersioning.create.bucket", bucket),
            status: crate::send_model::Sent::new("s3.BucketVersioning.create.status", status),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketVersioning`.
pub(crate) struct S3BucketVersioningUpdateInput {
    status: String,
}

pub(crate) struct S3BucketVersioningUpdateParts {
    pub(crate) status: crate::send_model::Sent<String>,
}

impl S3BucketVersioningUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::s3::bucket_versioning::s3_bucket_versioning_config().schema;
        Ok(Self {
            status: crate::send_model::required_enum(_resource, _schema, "status")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketVersioningUpdateParts {
        let Self { status } = self;
        S3BucketVersioningUpdateParts {
            status: crate::send_model::Sent::new("s3.BucketVersioning.update.status", status),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketObjectLockConfiguration`.
pub(crate) struct S3BucketObjectLockConfigurationCreateInput {
    bucket: String,
    object_lock_enabled: String,
    rule: Option<Value>,
}

pub(crate) struct S3BucketObjectLockConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) object_lock_enabled: crate::send_model::Sent<String>,
    pub(crate) rule: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("object_lock_enabled".to_string());
        if self.rule.is_some() {
            sent.insert("rule".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketObjectLockConfigurationCreateParts {
        let Self {
            bucket,
            object_lock_enabled,
            rule,
        } = self;
        S3BucketObjectLockConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketObjectLockConfiguration.create.bucket",
                bucket,
            ),
            object_lock_enabled: crate::send_model::Sent::new(
                "s3.BucketObjectLockConfiguration.create.object_lock_enabled",
                object_lock_enabled,
            ),
            rule: crate::send_model::Sent::new(
                "s3.BucketObjectLockConfiguration.create.rule",
                rule,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketObjectLockConfiguration`.
pub(crate) struct S3BucketObjectLockConfigurationUpdateInput {
    object_lock_enabled: String,
    rule: Option<Value>,
}

pub(crate) struct S3BucketObjectLockConfigurationUpdateParts {
    pub(crate) object_lock_enabled: crate::send_model::Sent<String>,
    pub(crate) rule: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn into_parts(self) -> S3BucketObjectLockConfigurationUpdateParts {
        let Self {
            object_lock_enabled,
            rule,
        } = self;
        S3BucketObjectLockConfigurationUpdateParts {
            object_lock_enabled: crate::send_model::Sent::new(
                "s3.BucketObjectLockConfiguration.update.object_lock_enabled",
                object_lock_enabled,
            ),
            rule: crate::send_model::Sent::new(
                "s3.BucketObjectLockConfiguration.update.rule",
                rule,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketAcl`.
pub(crate) struct S3BucketAclCreateInput {
    acl: String,
    bucket: String,
}

pub(crate) struct S3BucketAclCreateParts {
    pub(crate) acl: crate::send_model::Sent<String>,
    pub(crate) bucket: crate::send_model::Sent<String>,
}

impl S3BucketAclCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema;
        Ok(Self {
            acl: crate::send_model::required_enum(_resource, _schema, "acl")?,
            bucket: crate::send_model::required_string(_resource, "bucket")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("acl".to_string());
        sent.insert("bucket".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketAclCreateParts {
        let Self { acl, bucket } = self;
        S3BucketAclCreateParts {
            acl: crate::send_model::Sent::new("s3.BucketAcl.create.acl", acl),
            bucket: crate::send_model::Sent::new("s3.BucketAcl.create.bucket", bucket),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketAcl`.
pub(crate) struct S3BucketAclUpdateInput {
    acl: String,
}

pub(crate) struct S3BucketAclUpdateParts {
    pub(crate) acl: crate::send_model::Sent<String>,
}

impl S3BucketAclUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema;
        Ok(Self {
            acl: crate::send_model::required_enum(_resource, _schema, "acl")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketAclUpdateParts {
        let Self { acl } = self;
        S3BucketAclUpdateParts {
            acl: crate::send_model::Sent::new("s3.BucketAcl.update.acl", acl),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketOwnershipControls`.
pub(crate) struct S3BucketOwnershipControlsCreateInput {
    bucket: String,
    object_ownership: String,
}

pub(crate) struct S3BucketOwnershipControlsCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) object_ownership: crate::send_model::Sent<String>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("object_ownership".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketOwnershipControlsCreateParts {
        let Self {
            bucket,
            object_ownership,
        } = self;
        S3BucketOwnershipControlsCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketOwnershipControls.create.bucket",
                bucket,
            ),
            object_ownership: crate::send_model::Sent::new(
                "s3.BucketOwnershipControls.create.object_ownership",
                object_ownership,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketOwnershipControls`.
pub(crate) struct S3BucketOwnershipControlsUpdateInput {
    object_ownership: String,
}

pub(crate) struct S3BucketOwnershipControlsUpdateParts {
    pub(crate) object_ownership: crate::send_model::Sent<String>,
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

    pub(crate) fn into_parts(self) -> S3BucketOwnershipControlsUpdateParts {
        let Self { object_ownership } = self;
        S3BucketOwnershipControlsUpdateParts {
            object_ownership: crate::send_model::Sent::new(
                "s3.BucketOwnershipControls.update.object_ownership",
                object_ownership,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketServerSideEncryptionConfiguration`.
pub(crate) struct S3BucketServerSideEncryptionConfigurationCreateInput {
    bucket: String,
    rules: Value,
}

pub(crate) struct S3BucketServerSideEncryptionConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) rules: crate::send_model::Sent<Value>,
}

impl S3BucketServerSideEncryptionConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("rules".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketServerSideEncryptionConfigurationCreateParts {
        let Self { bucket, rules } = self;
        S3BucketServerSideEncryptionConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketServerSideEncryptionConfiguration.create.bucket",
                bucket,
            ),
            rules: crate::send_model::Sent::new(
                "s3.BucketServerSideEncryptionConfiguration.create.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketServerSideEncryptionConfiguration`.
pub(crate) struct S3BucketServerSideEncryptionConfigurationUpdateInput {
    rules: Value,
}

pub(crate) struct S3BucketServerSideEncryptionConfigurationUpdateParts {
    pub(crate) rules: crate::send_model::Sent<Value>,
}

impl S3BucketServerSideEncryptionConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema;
        Ok(Self {
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketServerSideEncryptionConfigurationUpdateParts {
        let Self { rules } = self;
        S3BucketServerSideEncryptionConfigurationUpdateParts {
            rules: crate::send_model::Sent::new(
                "s3.BucketServerSideEncryptionConfiguration.update.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketReplicationConfiguration`.
pub(crate) struct S3BucketReplicationConfigurationCreateInput {
    bucket: String,
    role: String,
    rules: Value,
}

pub(crate) struct S3BucketReplicationConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) role: crate::send_model::Sent<String>,
    pub(crate) rules: crate::send_model::Sent<Value>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("role".to_string());
        sent.insert("rules".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketReplicationConfigurationCreateParts {
        let Self {
            bucket,
            role,
            rules,
        } = self;
        S3BucketReplicationConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketReplicationConfiguration.create.bucket",
                bucket,
            ),
            role: crate::send_model::Sent::new(
                "s3.BucketReplicationConfiguration.create.role",
                role,
            ),
            rules: crate::send_model::Sent::new(
                "s3.BucketReplicationConfiguration.create.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketReplicationConfiguration`.
pub(crate) struct S3BucketReplicationConfigurationUpdateInput {
    role: String,
    rules: Value,
}

pub(crate) struct S3BucketReplicationConfigurationUpdateParts {
    pub(crate) role: crate::send_model::Sent<String>,
    pub(crate) rules: crate::send_model::Sent<Value>,
}

impl S3BucketReplicationConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_replication_configuration::s3_bucket_replication_configuration_config().schema;
        Ok(Self {
            role: crate::send_model::required_string(_resource, "role")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketReplicationConfigurationUpdateParts {
        let Self { role, rules } = self;
        S3BucketReplicationConfigurationUpdateParts {
            role: crate::send_model::Sent::new(
                "s3.BucketReplicationConfiguration.update.role",
                role,
            ),
            rules: crate::send_model::Sent::new(
                "s3.BucketReplicationConfiguration.update.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketLifecycleConfiguration`.
pub(crate) struct S3BucketLifecycleConfigurationCreateInput {
    bucket: String,
    rules: Value,
}

pub(crate) struct S3BucketLifecycleConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) rules: crate::send_model::Sent<Value>,
}

impl S3BucketLifecycleConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("rules".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketLifecycleConfigurationCreateParts {
        let Self { bucket, rules } = self;
        S3BucketLifecycleConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketLifecycleConfiguration.create.bucket",
                bucket,
            ),
            rules: crate::send_model::Sent::new(
                "s3.BucketLifecycleConfiguration.create.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketLifecycleConfiguration`.
pub(crate) struct S3BucketLifecycleConfigurationUpdateInput {
    rules: Value,
}

pub(crate) struct S3BucketLifecycleConfigurationUpdateParts {
    pub(crate) rules: crate::send_model::Sent<Value>,
}

impl S3BucketLifecycleConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema;
        Ok(Self {
            rules: crate::send_model::required_value(_resource, "rules")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketLifecycleConfigurationUpdateParts {
        let Self { rules } = self;
        S3BucketLifecycleConfigurationUpdateParts {
            rules: crate::send_model::Sent::new(
                "s3.BucketLifecycleConfiguration.update.rules",
                rules,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketWebsiteConfiguration`.
pub(crate) struct S3BucketWebsiteConfigurationCreateInput {
    bucket: String,
    error_document: Option<Value>,
    index_document: Option<Value>,
    redirect_all_requests_to: Option<Value>,
}

pub(crate) struct S3BucketWebsiteConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) error_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) index_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) redirect_all_requests_to: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketWebsiteConfigurationCreateParts {
        let Self {
            bucket,
            error_document,
            index_document,
            redirect_all_requests_to,
        } = self;
        S3BucketWebsiteConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.create.bucket",
                bucket,
            ),
            error_document: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.create.error_document",
                error_document,
            ),
            index_document: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.create.index_document",
                index_document,
            ),
            redirect_all_requests_to: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.create.redirect_all_requests_to",
                redirect_all_requests_to,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketWebsiteConfiguration`.
pub(crate) struct S3BucketWebsiteConfigurationUpdateInput {
    error_document: Option<Value>,
    index_document: Option<Value>,
    redirect_all_requests_to: Option<Value>,
}

pub(crate) struct S3BucketWebsiteConfigurationUpdateParts {
    pub(crate) error_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) index_document: crate::send_model::Sent<Option<Value>>,
    pub(crate) redirect_all_requests_to: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn into_parts(self) -> S3BucketWebsiteConfigurationUpdateParts {
        let Self {
            error_document,
            index_document,
            redirect_all_requests_to,
        } = self;
        S3BucketWebsiteConfigurationUpdateParts {
            error_document: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.update.error_document",
                error_document,
            ),
            index_document: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.update.index_document",
                index_document,
            ),
            redirect_all_requests_to: crate::send_model::Sent::new(
                "s3.BucketWebsiteConfiguration.update.redirect_all_requests_to",
                redirect_all_requests_to,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketCorsConfiguration`.
pub(crate) struct S3BucketCorsConfigurationCreateInput {
    bucket: String,
    cors_rules: Value,
}

pub(crate) struct S3BucketCorsConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) cors_rules: crate::send_model::Sent<Value>,
}

impl S3BucketCorsConfigurationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_cors_configuration::s3_bucket_cors_configuration_config().schema;
        Ok(Self {
            bucket: crate::send_model::required_string(_resource, "bucket")?,
            cors_rules: crate::send_model::required_value(_resource, "cors_rules")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("cors_rules".to_string());
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketCorsConfigurationCreateParts {
        let Self { bucket, cors_rules } = self;
        S3BucketCorsConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketCorsConfiguration.create.bucket",
                bucket,
            ),
            cors_rules: crate::send_model::Sent::new(
                "s3.BucketCorsConfiguration.create.cors_rules",
                cors_rules,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketCorsConfiguration`.
pub(crate) struct S3BucketCorsConfigurationUpdateInput {
    cors_rules: Value,
}

pub(crate) struct S3BucketCorsConfigurationUpdateParts {
    pub(crate) cors_rules: crate::send_model::Sent<Value>,
}

impl S3BucketCorsConfigurationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::s3::bucket_cors_configuration::s3_bucket_cors_configuration_config().schema;
        Ok(Self {
            cors_rules: crate::send_model::required_value(_resource, "cors_rules")?,
        })
    }

    pub(crate) fn into_parts(self) -> S3BucketCorsConfigurationUpdateParts {
        let Self { cors_rules } = self;
        S3BucketCorsConfigurationUpdateParts {
            cors_rules: crate::send_model::Sent::new(
                "s3.BucketCorsConfiguration.update.cors_rules",
                cors_rules,
            ),
        }
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

pub(crate) struct S3BucketNotificationConfigurationCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) event_bridge_configuration: crate::send_model::Sent<Option<Value>>,
    pub(crate) lambda_function_configurations: crate::send_model::Sent<Option<Value>>,
    pub(crate) queue_configurations: crate::send_model::Sent<Option<Value>>,
    pub(crate) topic_configurations: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketNotificationConfigurationCreateParts {
        let Self {
            bucket,
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        } = self;
        S3BucketNotificationConfigurationCreateParts {
            bucket: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.create.bucket",
                bucket,
            ),
            event_bridge_configuration: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.create.event_bridge_configuration",
                event_bridge_configuration,
            ),
            lambda_function_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.create.lambda_function_configurations",
                lambda_function_configurations,
            ),
            queue_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.create.queue_configurations",
                queue_configurations,
            ),
            topic_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.create.topic_configurations",
                topic_configurations,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketNotificationConfiguration`.
pub(crate) struct S3BucketNotificationConfigurationUpdateInput {
    event_bridge_configuration: Option<Value>,
    lambda_function_configurations: Option<Value>,
    queue_configurations: Option<Value>,
    topic_configurations: Option<Value>,
}

pub(crate) struct S3BucketNotificationConfigurationUpdateParts {
    pub(crate) event_bridge_configuration: crate::send_model::Sent<Option<Value>>,
    pub(crate) lambda_function_configurations: crate::send_model::Sent<Option<Value>>,
    pub(crate) queue_configurations: crate::send_model::Sent<Option<Value>>,
    pub(crate) topic_configurations: crate::send_model::Sent<Option<Value>>,
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

    pub(crate) fn into_parts(self) -> S3BucketNotificationConfigurationUpdateParts {
        let Self {
            event_bridge_configuration,
            lambda_function_configurations,
            queue_configurations,
            topic_configurations,
        } = self;
        S3BucketNotificationConfigurationUpdateParts {
            event_bridge_configuration: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.update.event_bridge_configuration",
                event_bridge_configuration,
            ),
            lambda_function_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.update.lambda_function_configurations",
                lambda_function_configurations,
            ),
            queue_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.update.queue_configurations",
                queue_configurations,
            ),
            topic_configurations: crate::send_model::Sent::new(
                "s3.BucketNotificationConfiguration.update.topic_configurations",
                topic_configurations,
            ),
        }
    }
}

/// Exhaustive typed create input for `s3.BucketLogging`.
pub(crate) struct S3BucketLoggingCreateInput {
    bucket: String,
    target_bucket: String,
    target_object_key_format: Option<Value>,
    target_prefix: Option<String>,
}

pub(crate) struct S3BucketLoggingCreateParts {
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) target_bucket: crate::send_model::Sent<String>,
    pub(crate) target_object_key_format: crate::send_model::Sent<Option<Value>>,
    pub(crate) target_prefix: crate::send_model::Sent<Option<String>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        sent.insert("bucket".to_string());
        sent.insert("target_bucket".to_string());
        if self.target_object_key_format.is_some() {
            sent.insert("target_object_key_format".to_string());
        }
        if self.target_prefix.is_some() {
            sent.insert("target_prefix".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketLoggingCreateParts {
        let Self {
            bucket,
            target_bucket,
            target_object_key_format,
            target_prefix,
        } = self;
        S3BucketLoggingCreateParts {
            bucket: crate::send_model::Sent::new("s3.BucketLogging.create.bucket", bucket),
            target_bucket: crate::send_model::Sent::new(
                "s3.BucketLogging.create.target_bucket",
                target_bucket,
            ),
            target_object_key_format: crate::send_model::Sent::new(
                "s3.BucketLogging.create.target_object_key_format",
                target_object_key_format,
            ),
            target_prefix: crate::send_model::Sent::new(
                "s3.BucketLogging.create.target_prefix",
                target_prefix,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketLogging`.
pub(crate) struct S3BucketLoggingUpdateInput {
    target_bucket: String,
    target_object_key_format: Option<Value>,
    target_prefix: Option<String>,
}

pub(crate) struct S3BucketLoggingUpdateParts {
    pub(crate) target_bucket: crate::send_model::Sent<String>,
    pub(crate) target_object_key_format: crate::send_model::Sent<Option<Value>>,
    pub(crate) target_prefix: crate::send_model::Sent<Option<String>>,
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

    pub(crate) fn into_parts(self) -> S3BucketLoggingUpdateParts {
        let Self {
            target_bucket,
            target_object_key_format,
            target_prefix,
        } = self;
        S3BucketLoggingUpdateParts {
            target_bucket: crate::send_model::Sent::new(
                "s3.BucketLogging.update.target_bucket",
                target_bucket,
            ),
            target_object_key_format: crate::send_model::Sent::new(
                "s3.BucketLogging.update.target_object_key_format",
                target_object_key_format,
            ),
            target_prefix: crate::send_model::Sent::new(
                "s3.BucketLogging.update.target_prefix",
                target_prefix,
            ),
        }
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

pub(crate) struct S3BucketPublicAccessBlockCreateParts {
    pub(crate) block_public_acls: crate::send_model::Sent<Option<bool>>,
    pub(crate) block_public_policy: crate::send_model::Sent<Option<bool>>,
    pub(crate) bucket: crate::send_model::Sent<String>,
    pub(crate) ignore_public_acls: crate::send_model::Sent<Option<bool>>,
    pub(crate) restrict_public_buckets: crate::send_model::Sent<Option<bool>>,
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

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> S3BucketPublicAccessBlockCreateParts {
        let Self {
            block_public_acls,
            block_public_policy,
            bucket,
            ignore_public_acls,
            restrict_public_buckets,
        } = self;
        S3BucketPublicAccessBlockCreateParts {
            block_public_acls: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.create.block_public_acls",
                block_public_acls,
            ),
            block_public_policy: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.create.block_public_policy",
                block_public_policy,
            ),
            bucket: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.create.bucket",
                bucket,
            ),
            ignore_public_acls: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.create.ignore_public_acls",
                ignore_public_acls,
            ),
            restrict_public_buckets: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.create.restrict_public_buckets",
                restrict_public_buckets,
            ),
        }
    }
}

/// Exhaustive typed update input for `s3.BucketPublicAccessBlock`.
pub(crate) struct S3BucketPublicAccessBlockUpdateInput {
    block_public_acls: Option<bool>,
    block_public_policy: Option<bool>,
    ignore_public_acls: Option<bool>,
    restrict_public_buckets: Option<bool>,
}

pub(crate) struct S3BucketPublicAccessBlockUpdateParts {
    pub(crate) block_public_acls: crate::send_model::Sent<Option<bool>>,
    pub(crate) block_public_policy: crate::send_model::Sent<Option<bool>>,
    pub(crate) ignore_public_acls: crate::send_model::Sent<Option<bool>>,
    pub(crate) restrict_public_buckets: crate::send_model::Sent<Option<bool>>,
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

    pub(crate) fn into_parts(self) -> S3BucketPublicAccessBlockUpdateParts {
        let Self {
            block_public_acls,
            block_public_policy,
            ignore_public_acls,
            restrict_public_buckets,
        } = self;
        S3BucketPublicAccessBlockUpdateParts {
            block_public_acls: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.update.block_public_acls",
                block_public_acls,
            ),
            block_public_policy: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.update.block_public_policy",
                block_public_policy,
            ),
            ignore_public_acls: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.update.ignore_public_acls",
                ignore_public_acls,
            ),
            restrict_public_buckets: crate::send_model::Sent::new(
                "s3.BucketPublicAccessBlock.update.restrict_public_buckets",
                restrict_public_buckets,
            ),
        }
    }
}

/// Exhaustive typed create input for `organizations.Organization`.
pub(crate) struct OrganizationsOrganizationCreateInput {
    feature_set: Option<String>,
}

pub(crate) struct OrganizationsOrganizationCreateParts {
    pub(crate) feature_set: crate::send_model::Sent<Option<String>>,
}

impl OrganizationsOrganizationCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::organizations::organization::organizations_organization_config().schema;
        Ok(Self {
            feature_set: crate::send_model::optional_enum(_resource, _schema, "feature_set")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
        let mut sent = BTreeSet::new();
        if self.feature_set.is_some() {
            sent.insert("feature_set".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> OrganizationsOrganizationCreateParts {
        let Self { feature_set } = self;
        OrganizationsOrganizationCreateParts {
            feature_set: crate::send_model::Sent::new(
                "organizations.Organization.create.feature_set",
                feature_set,
            ),
        }
    }
}

/// Exhaustive typed update input for `organizations.Organization`.
pub(crate) struct OrganizationsOrganizationUpdateInput;

pub(crate) struct OrganizationsOrganizationUpdateParts;

impl OrganizationsOrganizationUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::organizations::organization::organizations_organization_config().schema;
        Ok(Self)
    }

    pub(crate) fn into_parts(self) -> OrganizationsOrganizationUpdateParts {
        let Self = self;
        OrganizationsOrganizationUpdateParts
    }
}

/// Exhaustive typed create input for `organizations.Account`.
pub(crate) struct OrganizationsAccountCreateInput {
    account_name: String,
    email: String,
    iam_user_access_to_billing: Option<String>,
    role_name: Option<String>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct OrganizationsAccountCreateParts {
    pub(crate) account_name: crate::send_model::Sent<String>,
    pub(crate) email: crate::send_model::Sent<String>,
    pub(crate) iam_user_access_to_billing: crate::send_model::Sent<Option<String>>,
    pub(crate) role_name: crate::send_model::Sent<Option<String>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> OrganizationsAccountCreateParts {
        let Self {
            account_name,
            email,
            iam_user_access_to_billing,
            role_name,
            tags,
        } = self;
        OrganizationsAccountCreateParts {
            account_name: crate::send_model::Sent::new(
                "organizations.Account.create.account_name",
                account_name,
            ),
            email: crate::send_model::Sent::new("organizations.Account.create.email", email),
            iam_user_access_to_billing: crate::send_model::Sent::new(
                "organizations.Account.create.iam_user_access_to_billing",
                iam_user_access_to_billing,
            ),
            role_name: crate::send_model::Sent::new(
                "organizations.Account.create.role_name",
                role_name,
            ),
            tags: crate::send_model::Sent::new("organizations.Account.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `organizations.Account`.
pub(crate) struct OrganizationsAccountUpdateInput {
    account_name: String,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct OrganizationsAccountUpdateParts {
    pub(crate) account_name: crate::send_model::Sent<String>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl OrganizationsAccountUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::organizations::account::organizations_account_config()
                .schema;
        Ok(Self {
            account_name: crate::send_model::required_string(_resource, "account_name")?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> OrganizationsAccountUpdateParts {
        let Self { account_name, tags } = self;
        OrganizationsAccountUpdateParts {
            account_name: crate::send_model::Sent::new(
                "organizations.Account.update.account_name",
                account_name,
            ),
            tags: crate::send_model::Sent::new("organizations.Account.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `route53.RecordSet`.
pub(crate) struct Route53RecordSetCreateInput {
    alias_target: Option<Value>,
    hosted_zone_id: String,
    name: String,
    resource_records: Option<crate::send_model::StringList>,
    ttl: Option<i64>,
    r#type: String,
}

pub(crate) struct Route53RecordSetCreateParts {
    pub(crate) alias_target: crate::send_model::Sent<Option<Value>>,
    pub(crate) hosted_zone_id: crate::send_model::Sent<String>,
    pub(crate) name: crate::send_model::Sent<String>,
    pub(crate) resource_records: crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) ttl: crate::send_model::Sent<Option<i64>>,
    pub(crate) r#type: crate::send_model::Sent<String>,
}

impl Route53RecordSetCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::route53::record_set::route53_record_set_config().schema;
        Ok(Self {
            alias_target: crate::send_model::optional_value(_resource, "alias_target")?,
            hosted_zone_id: crate::send_model::required_string(_resource, "hosted_zone_id")?,
            name: crate::send_model::required_string(_resource, "name")?,
            resource_records: crate::send_model::optional_string_list(
                _resource,
                "resource_records",
            )?,
            ttl: crate::send_model::optional_int(_resource, "ttl")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> Route53RecordSetCreateParts {
        let Self {
            alias_target,
            hosted_zone_id,
            name,
            resource_records,
            ttl,
            r#type,
        } = self;
        Route53RecordSetCreateParts {
            alias_target: crate::send_model::Sent::new(
                "route53.RecordSet.create.alias_target",
                alias_target,
            ),
            hosted_zone_id: crate::send_model::Sent::new(
                "route53.RecordSet.create.hosted_zone_id",
                hosted_zone_id,
            ),
            name: crate::send_model::Sent::new("route53.RecordSet.create.name", name),
            resource_records: crate::send_model::Sent::new(
                "route53.RecordSet.create.resource_records",
                resource_records,
            ),
            ttl: crate::send_model::Sent::new("route53.RecordSet.create.ttl", ttl),
            r#type: crate::send_model::Sent::new("route53.RecordSet.create.type", r#type),
        }
    }
}

/// Exhaustive typed update input for `route53.RecordSet`.
pub(crate) struct Route53RecordSetUpdateInput {
    alias_target: Option<Value>,
    resource_records: Option<crate::send_model::StringList>,
    ttl: Option<i64>,
    r#type: String,
}

pub(crate) struct Route53RecordSetUpdateParts {
    pub(crate) alias_target: crate::send_model::Sent<Option<Value>>,
    pub(crate) resource_records: crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) ttl: crate::send_model::Sent<Option<i64>>,
    pub(crate) r#type: crate::send_model::Sent<String>,
}

impl Route53RecordSetUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema =
            &crate::schemas::generated::route53::record_set::route53_record_set_config().schema;
        Ok(Self {
            alias_target: crate::send_model::optional_value(_resource, "alias_target")?,
            resource_records: crate::send_model::optional_string_list(
                _resource,
                "resource_records",
            )?,
            ttl: crate::send_model::optional_int(_resource, "ttl")?,
            r#type: crate::send_model::required_enum(_resource, _schema, "type")?,
        })
    }

    pub(crate) fn into_parts(self) -> Route53RecordSetUpdateParts {
        let Self {
            alias_target,
            resource_records,
            ttl,
            r#type,
        } = self;
        Route53RecordSetUpdateParts {
            alias_target: crate::send_model::Sent::new(
                "route53.RecordSet.update.alias_target",
                alias_target,
            ),
            resource_records: crate::send_model::Sent::new(
                "route53.RecordSet.update.resource_records",
                resource_records,
            ),
            ttl: crate::send_model::Sent::new("route53.RecordSet.update.ttl", ttl),
            r#type: crate::send_model::Sent::new("route53.RecordSet.update.type", r#type),
        }
    }
}

/// Exhaustive typed create input for `iam.Role`.
pub(crate) struct IamRoleCreateInput {
    assume_role_policy_document: Value,
    description: Option<String>,
    max_session_duration: Option<i64>,
    path: Option<String>,
    role_name: String,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct IamRoleCreateParts {
    pub(crate) assume_role_policy_document: crate::send_model::Sent<Value>,
    pub(crate) description: crate::send_model::Sent<Option<String>>,
    pub(crate) max_session_duration: crate::send_model::Sent<Option<i64>>,
    pub(crate) path: crate::send_model::Sent<Option<String>>,
    pub(crate) role_name: crate::send_model::Sent<String>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> IamRoleCreateParts {
        let Self {
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_name,
            tags,
        } = self;
        IamRoleCreateParts {
            assume_role_policy_document: crate::send_model::Sent::new(
                "iam.Role.create.assume_role_policy_document",
                assume_role_policy_document,
            ),
            description: crate::send_model::Sent::new("iam.Role.create.description", description),
            max_session_duration: crate::send_model::Sent::new(
                "iam.Role.create.max_session_duration",
                max_session_duration,
            ),
            path: crate::send_model::Sent::new("iam.Role.create.path", path),
            role_name: crate::send_model::Sent::new("iam.Role.create.role_name", role_name),
            tags: crate::send_model::Sent::new("iam.Role.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `iam.Role`.
pub(crate) struct IamRoleUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct IamRoleUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl IamRoleUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::iam::role::iam_role_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> IamRoleUpdateParts {
        let Self { tags } = self;
        IamRoleUpdateParts {
            tags: crate::send_model::Sent::new("iam.Role.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `logs.LogGroup`.
pub(crate) struct LogsLogGroupCreateInput {
    deletion_protection_enabled: Option<bool>,
    kms_key_id: Option<String>,
    log_group_class: Option<String>,
    log_group_name: String,
    retention_in_days: Option<i64>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct LogsLogGroupCreateParts {
    pub(crate) deletion_protection_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) kms_key_id: crate::send_model::Sent<Option<String>>,
    pub(crate) log_group_class: crate::send_model::Sent<Option<String>>,
    pub(crate) log_group_name: crate::send_model::Sent<String>,
    pub(crate) retention_in_days: crate::send_model::Sent<Option<i64>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        if self.tags.is_some() {
            sent.insert("tags".to_string());
        }
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> LogsLogGroupCreateParts {
        let Self {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            tags,
        } = self;
        LogsLogGroupCreateParts {
            deletion_protection_enabled: crate::send_model::Sent::new(
                "logs.LogGroup.create.deletion_protection_enabled",
                deletion_protection_enabled,
            ),
            kms_key_id: crate::send_model::Sent::new("logs.LogGroup.create.kms_key_id", kms_key_id),
            log_group_class: crate::send_model::Sent::new(
                "logs.LogGroup.create.log_group_class",
                log_group_class,
            ),
            log_group_name: crate::send_model::Sent::new(
                "logs.LogGroup.create.log_group_name",
                log_group_name,
            ),
            retention_in_days: crate::send_model::Sent::new(
                "logs.LogGroup.create.retention_in_days",
                retention_in_days,
            ),
            tags: crate::send_model::Sent::new("logs.LogGroup.create.tags", tags),
        }
    }
}

/// Exhaustive typed update input for `logs.LogGroup`.
pub(crate) struct LogsLogGroupUpdateInput {
    deletion_protection_enabled: Option<bool>,
    retention_in_days: Option<i64>,
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct LogsLogGroupUpdateParts {
    pub(crate) deletion_protection_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) retention_in_days: crate::send_model::Sent<Option<i64>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> LogsLogGroupUpdateParts {
        let Self {
            deletion_protection_enabled,
            retention_in_days,
            tags,
        } = self;
        LogsLogGroupUpdateParts {
            deletion_protection_enabled: crate::send_model::Sent::new(
                "logs.LogGroup.update.deletion_protection_enabled",
                deletion_protection_enabled,
            ),
            retention_in_days: crate::send_model::Sent::new(
                "logs.LogGroup.update.retention_in_days",
                retention_in_days,
            ),
            tags: crate::send_model::Sent::new("logs.LogGroup.update.tags", tags),
        }
    }
}

/// Exhaustive typed create input for `acm.Certificate`.
pub(crate) struct AcmCertificateCreateInput {
    domain_name: String,
    key_algorithm: Option<String>,
    options: Option<Value>,
    subject_alternative_names: Option<crate::send_model::StringList>,
    tags: Option<crate::send_model::Tags>,
    validation_method: Option<String>,
}

pub(crate) struct AcmCertificateCreateParts {
    pub(crate) domain_name: crate::send_model::Sent<String>,
    pub(crate) key_algorithm: crate::send_model::Sent<Option<String>>,
    pub(crate) options: crate::send_model::Sent<Option<Value>>,
    pub(crate) subject_alternative_names:
        crate::send_model::Sent<Option<crate::send_model::StringList>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) validation_method: crate::send_model::Sent<Option<String>>,
}

impl AcmCertificateCreateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        Ok(Self {
            domain_name: crate::send_model::required_string(_resource, "domain_name")?,
            key_algorithm: crate::send_model::optional_enum(_resource, _schema, "key_algorithm")?,
            options: crate::send_model::optional_value(_resource, "options")?,
            subject_alternative_names: crate::send_model::optional_string_list(
                _resource,
                "subject_alternative_names",
            )?,
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            validation_method: crate::send_model::optional_enum(
                _resource,
                _schema,
                "validation_method",
            )?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> AcmCertificateCreateParts {
        let Self {
            domain_name,
            key_algorithm,
            options,
            subject_alternative_names,
            tags,
            validation_method,
        } = self;
        AcmCertificateCreateParts {
            domain_name: crate::send_model::Sent::new(
                "acm.Certificate.create.domain_name",
                domain_name,
            ),
            key_algorithm: crate::send_model::Sent::new(
                "acm.Certificate.create.key_algorithm",
                key_algorithm,
            ),
            options: crate::send_model::Sent::new("acm.Certificate.create.options", options),
            subject_alternative_names: crate::send_model::Sent::new(
                "acm.Certificate.create.subject_alternative_names",
                subject_alternative_names,
            ),
            tags: crate::send_model::Sent::new("acm.Certificate.create.tags", tags),
            validation_method: crate::send_model::Sent::new(
                "acm.Certificate.create.validation_method",
                validation_method,
            ),
        }
    }
}

/// Exhaustive typed update input for `acm.Certificate`.
pub(crate) struct AcmCertificateUpdateInput {
    tags: Option<crate::send_model::Tags>,
}

pub(crate) struct AcmCertificateUpdateParts {
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
}

impl AcmCertificateUpdateInput {
    pub(crate) fn decode(_resource: &Resource<impl Sized>) -> ProviderResult<Self> {
        let _schema = &crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        Ok(Self {
            tags: crate::send_model::optional_tags(_resource, "tags")?,
        })
    }

    pub(crate) fn into_parts(self) -> AcmCertificateUpdateParts {
        let Self { tags } = self;
        AcmCertificateUpdateParts {
            tags: crate::send_model::Sent::new("acm.Certificate.update.tags", tags),
        }
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
    tags: Option<crate::send_model::Tags>,
    visibility_timeout: Option<i64>,
}

pub(crate) struct SqsQueueCreateParts {
    pub(crate) content_based_deduplication: crate::send_model::Sent<Option<bool>>,
    pub(crate) deduplication_scope: crate::send_model::Sent<Option<String>>,
    pub(crate) delay_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) fifo_queue: crate::send_model::Sent<Option<bool>>,
    pub(crate) fifo_throughput_limit: crate::send_model::Sent<Option<String>>,
    pub(crate) kms_data_key_reuse_period_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) kms_master_key_id: crate::send_model::Sent<Option<String>>,
    pub(crate) maximum_message_size: crate::send_model::Sent<Option<i64>>,
    pub(crate) message_retention_period: crate::send_model::Sent<Option<i64>>,
    pub(crate) policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) queue_name: crate::send_model::Sent<String>,
    pub(crate) receive_message_wait_time_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) redrive_allow_policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) redrive_policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) sqs_managed_sse_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) visibility_timeout: crate::send_model::Sent<Option<i64>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            visibility_timeout: crate::send_model::optional_int(_resource, "visibility_timeout")?,
        })
    }

    pub(crate) fn sent(&self) -> CreateSent {
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
        CreateSent::only(sent)
    }

    pub(crate) fn into_parts(self) -> SqsQueueCreateParts {
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
        SqsQueueCreateParts {
            content_based_deduplication: crate::send_model::Sent::new(
                "sqs.Queue.create.content_based_deduplication",
                content_based_deduplication,
            ),
            deduplication_scope: crate::send_model::Sent::new(
                "sqs.Queue.create.deduplication_scope",
                deduplication_scope,
            ),
            delay_seconds: crate::send_model::Sent::new(
                "sqs.Queue.create.delay_seconds",
                delay_seconds,
            ),
            fifo_queue: crate::send_model::Sent::new("sqs.Queue.create.fifo_queue", fifo_queue),
            fifo_throughput_limit: crate::send_model::Sent::new(
                "sqs.Queue.create.fifo_throughput_limit",
                fifo_throughput_limit,
            ),
            kms_data_key_reuse_period_seconds: crate::send_model::Sent::new(
                "sqs.Queue.create.kms_data_key_reuse_period_seconds",
                kms_data_key_reuse_period_seconds,
            ),
            kms_master_key_id: crate::send_model::Sent::new(
                "sqs.Queue.create.kms_master_key_id",
                kms_master_key_id,
            ),
            maximum_message_size: crate::send_model::Sent::new(
                "sqs.Queue.create.maximum_message_size",
                maximum_message_size,
            ),
            message_retention_period: crate::send_model::Sent::new(
                "sqs.Queue.create.message_retention_period",
                message_retention_period,
            ),
            policy: crate::send_model::Sent::new("sqs.Queue.create.policy", policy),
            queue_name: crate::send_model::Sent::new("sqs.Queue.create.queue_name", queue_name),
            receive_message_wait_time_seconds: crate::send_model::Sent::new(
                "sqs.Queue.create.receive_message_wait_time_seconds",
                receive_message_wait_time_seconds,
            ),
            redrive_allow_policy: crate::send_model::Sent::new(
                "sqs.Queue.create.redrive_allow_policy",
                redrive_allow_policy,
            ),
            redrive_policy: crate::send_model::Sent::new(
                "sqs.Queue.create.redrive_policy",
                redrive_policy,
            ),
            sqs_managed_sse_enabled: crate::send_model::Sent::new(
                "sqs.Queue.create.sqs_managed_sse_enabled",
                sqs_managed_sse_enabled,
            ),
            tags: crate::send_model::Sent::new("sqs.Queue.create.tags", tags),
            visibility_timeout: crate::send_model::Sent::new(
                "sqs.Queue.create.visibility_timeout",
                visibility_timeout,
            ),
        }
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
    tags: Option<crate::send_model::Tags>,
    visibility_timeout: Option<i64>,
}

pub(crate) struct SqsQueueUpdateParts {
    pub(crate) content_based_deduplication: crate::send_model::Sent<Option<bool>>,
    pub(crate) delay_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) kms_data_key_reuse_period_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) kms_master_key_id: crate::send_model::Sent<Option<String>>,
    pub(crate) maximum_message_size: crate::send_model::Sent<Option<i64>>,
    pub(crate) message_retention_period: crate::send_model::Sent<Option<i64>>,
    pub(crate) policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) receive_message_wait_time_seconds: crate::send_model::Sent<Option<i64>>,
    pub(crate) redrive_allow_policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) redrive_policy: crate::send_model::Sent<Option<Value>>,
    pub(crate) sqs_managed_sse_enabled: crate::send_model::Sent<Option<bool>>,
    pub(crate) tags: crate::send_model::Sent<Option<crate::send_model::Tags>>,
    pub(crate) visibility_timeout: crate::send_model::Sent<Option<i64>>,
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
            tags: crate::send_model::optional_tags(_resource, "tags")?,
            visibility_timeout: crate::send_model::optional_int(_resource, "visibility_timeout")?,
        })
    }

    pub(crate) fn into_parts(self) -> SqsQueueUpdateParts {
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
        SqsQueueUpdateParts {
            content_based_deduplication: crate::send_model::Sent::new(
                "sqs.Queue.update.content_based_deduplication",
                content_based_deduplication,
            ),
            delay_seconds: crate::send_model::Sent::new(
                "sqs.Queue.update.delay_seconds",
                delay_seconds,
            ),
            kms_data_key_reuse_period_seconds: crate::send_model::Sent::new(
                "sqs.Queue.update.kms_data_key_reuse_period_seconds",
                kms_data_key_reuse_period_seconds,
            ),
            kms_master_key_id: crate::send_model::Sent::new(
                "sqs.Queue.update.kms_master_key_id",
                kms_master_key_id,
            ),
            maximum_message_size: crate::send_model::Sent::new(
                "sqs.Queue.update.maximum_message_size",
                maximum_message_size,
            ),
            message_retention_period: crate::send_model::Sent::new(
                "sqs.Queue.update.message_retention_period",
                message_retention_period,
            ),
            policy: crate::send_model::Sent::new("sqs.Queue.update.policy", policy),
            receive_message_wait_time_seconds: crate::send_model::Sent::new(
                "sqs.Queue.update.receive_message_wait_time_seconds",
                receive_message_wait_time_seconds,
            ),
            redrive_allow_policy: crate::send_model::Sent::new(
                "sqs.Queue.update.redrive_allow_policy",
                redrive_allow_policy,
            ),
            redrive_policy: crate::send_model::Sent::new(
                "sqs.Queue.update.redrive_policy",
                redrive_policy,
            ),
            sqs_managed_sse_enabled: crate::send_model::Sent::new(
                "sqs.Queue.update.sqs_managed_sse_enabled",
                sqs_managed_sse_enabled,
            ),
            tags: crate::send_model::Sent::new("sqs.Queue.update.tags", tags),
            visibility_timeout: crate::send_model::Sent::new(
                "sqs.Queue.update.visibility_timeout",
                visibility_timeout,
            ),
        }
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
    pub(crate) create_attributes: &'static [SendCaptureAttribute],
    pub(crate) update_attributes: &'static [SendCaptureAttribute],
}

#[cfg(test)]
pub(crate) fn capture_contracts() -> &'static [SendCaptureContract] {
    &[
        SendCaptureContract {
            resource_type: "ec2.Vpc",
            create_attributes: &[
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
            update_attributes: &[
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
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.Subnet",
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "assign_ipv6_address_on_creation",
                    provider_name: "AssignIpv6AddressOnCreation",
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
                    name: "map_public_ip_on_launch",
                    provider_name: "MapPublicIpOnLaunch",
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
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.InternetGateway",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.RouteTable",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.Route",
            create_attributes: &[
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
            update_attributes: &[
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
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroup",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroupIngress",
            create_attributes: &[
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
            update_attributes: &[],
        },
        SendCaptureContract {
            resource_type: "ec2.SecurityGroupEgress",
            create_attributes: &[
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
            update_attributes: &[],
        },
        SendCaptureContract {
            resource_type: "ec2.EgressOnlyInternetGateway",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.Eip",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.FlowLog",
            create_attributes: &[
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
                    name: "tag_field_specifications",
                    provider_name: "TagFieldSpecifications",
                    capture_names: &["TagFieldSpecification"],
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.NatGateway",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.SubnetRouteTableAssociation",
            create_attributes: &[
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
            update_attributes: &[],
        },
        SendCaptureContract {
            resource_type: "ec2.TransitGateway",
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "description",
                    provider_name: "Description",
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
            resource_type: "ec2.TransitGatewayAttachment",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcEndpoint",
            create_attributes: &[
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
            update_attributes: &[
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
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcGatewayAttachment",
            create_attributes: &[
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
            update_attributes: &[],
        },
        SendCaptureContract {
            resource_type: "ec2.VpcPeeringConnection",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "ec2.VpnGateway",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.Bucket",
            create_attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "bucket_namespace",
                    provider_name: "BucketNamespace",
                    capture_names: &["header:x-amz-bucket-namespace"],
                },
                SendCaptureAttribute {
                    name: "tags",
                    provider_name: "Tags",
                    capture_names: &[],
                },
            ],
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketPolicy",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "policy",
                provider_name: "Policy",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketVersioning",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "status",
                provider_name: "Status",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketObjectLockConfiguration",
            create_attributes: &[
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
            update_attributes: &[
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
            create_attributes: &[
                SendCaptureAttribute {
                    name: "acl",
                    provider_name: "ACL",
                    capture_names: &["header:x-amz-acl"],
                },
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
            ],
            update_attributes: &[SendCaptureAttribute {
                name: "acl",
                provider_name: "ACL",
                capture_names: &["header:x-amz-acl"],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketOwnershipControls",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "object_ownership",
                provider_name: "ObjectOwnership",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketServerSideEncryptionConfiguration",
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "rules",
                provider_name: "Rules",
                capture_names: &["Rule"],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketReplicationConfiguration",
            create_attributes: &[
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
            update_attributes: &[
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
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "rules",
                provider_name: "Rules",
                capture_names: &["Rule"],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketWebsiteConfiguration",
            create_attributes: &[
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
            update_attributes: &[
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
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "cors_rules",
                provider_name: "CORSRules",
                capture_names: &["CORSRule"],
            }],
        },
        SendCaptureContract {
            resource_type: "s3.BucketNotificationConfiguration",
            create_attributes: &[
                SendCaptureAttribute {
                    name: "bucket",
                    provider_name: "Bucket",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "event_bridge_configuration",
                    provider_name: "EventBridgeConfiguration",
                    capture_names: &["presence:EventBridgeConfiguration"],
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "event_bridge_configuration",
                    provider_name: "EventBridgeConfiguration",
                    capture_names: &["presence:EventBridgeConfiguration"],
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
            create_attributes: &[
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
            update_attributes: &[
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
            create_attributes: &[
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
            update_attributes: &[
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
            create_attributes: &[SendCaptureAttribute {
                name: "feature_set",
                provider_name: "FeatureSet",
                capture_names: &[],
            }],
            update_attributes: &[],
        },
        SendCaptureContract {
            resource_type: "organizations.Account",
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "account_name",
                    provider_name: "AccountName",
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
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "alias_target",
                    provider_name: "AliasTarget",
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
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "logs.LogGroup",
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "deletion_protection_enabled",
                    provider_name: "deletionProtectionEnabled",
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
            create_attributes: &[
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
            update_attributes: &[SendCaptureAttribute {
                name: "tags",
                provider_name: "Tags",
                capture_names: &[],
            }],
        },
        SendCaptureContract {
            resource_type: "sqs.Queue",
            create_attributes: &[
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
            update_attributes: &[
                SendCaptureAttribute {
                    name: "content_based_deduplication",
                    provider_name: "ContentBasedDeduplication",
                    capture_names: &[],
                },
                SendCaptureAttribute {
                    name: "delay_seconds",
                    provider_name: "DelaySeconds",
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
