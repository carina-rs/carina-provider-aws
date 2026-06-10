//! Auto-generated provider boilerplate
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-provider.sh

use indexmap::IndexMap;
use std::collections::HashMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use carina_core::provider::{BoxFuture, ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, DataSource, Resource, ResourceId, State, Value};

// ===== Generated Methods on AwsProvider =====

#[allow(dead_code)]
impl AwsProvider {
    /// Delete ec2.Vpc (generated)
    pub(crate) async fn delete_ec2_vpc(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_vpc()
            .vpc_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete vpc", "ec2.DeleteVpc", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.Subnet (generated)
    pub(crate) async fn delete_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_subnet()
            .subnet_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete subnet", "ec2.DeleteSubnet", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.RouteTable (generated)
    pub(crate) async fn delete_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_route_table()
            .route_table_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete route table", "ec2.DeleteRouteTable", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.SecurityGroup (generated)
    pub(crate) async fn delete_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_security_group()
            .group_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to delete security group",
                    "ec2.DeleteSecurityGroup",
                    e,
                )
                .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Update ec2.InternetGateway: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_internet_gateway(&id, Some(identifier)).await
    }

    /// Update ec2.RouteTable: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_route_table(&id, Some(identifier)).await
    }

    /// Update ec2.SecurityGroup: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_security_group(&id, Some(identifier)).await
    }

    /// Update organizations.Organization (no-op, just read back current state) (generated)
    pub(crate) async fn update_organizations_organization(
        &self,
        id: ResourceId,
        identifier: &str,
        _to: Resource,
    ) -> ProviderResult<State> {
        self.read_organizations_organization(&id, Some(identifier))
            .await
    }

    /// Extract ec2.Vpc attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_attributes(
        obj: &aws_sdk_ec2::types::Vpc,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.cidr_block() {
            attributes.insert(
                "cidr_block".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.instance_tenancy() {
            attributes.insert(
                "instance_tenancy".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.vpc_id().map(String::from)
    }

    /// Extract ec2.Subnet attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_attributes(
        obj: &aws_sdk_ec2::types::Subnet,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.assign_ipv6_address_on_creation() {
            attributes.insert(
                "assign_ipv6_address_on_creation".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.availability_zone() {
            attributes.insert(
                "availability_zone".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.availability_zone_id() {
            attributes.insert(
                "availability_zone_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.cidr_block() {
            attributes.insert(
                "cidr_block".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.enable_dns64() {
            attributes.insert(
                "enable_dns64".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.enable_lni_at_device_index() {
            attributes.insert(
                "enable_lni_at_device_index".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        if let Some(v) = obj.ipv6_native() {
            attributes.insert(
                "ipv6_native".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.map_public_ip_on_launch() {
            attributes.insert(
                "map_public_ip_on_launch".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.outpost_arn() {
            attributes.insert(
                "outpost_arn".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.subnet_id() {
            attributes.insert(
                "subnet_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(dns_opts) = obj.private_dns_name_options_on_launch() {
            let mut fields = IndexMap::new();
            if let Some(v) = dns_opts.hostname_type() {
                fields.insert(
                    "hostname_type".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
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
            if !fields.is_empty() {
                attributes.insert(
                    "private_dns_name_options_on_launch".to_string(),
                    Value::Concrete(ConcreteValue::Map(fields)),
                );
            }
        }
        obj.subnet_id().map(String::from)
    }

    /// Extract ec2.InternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::InternetGateway,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.internet_gateway_id() {
            attributes.insert(
                "internet_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.internet_gateway_id().map(String::from)
    }

    /// Extract ec2.RouteTable attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_table_attributes(
        obj: &aws_sdk_ec2::types::RouteTable,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.route_table_id() {
            attributes.insert(
                "route_table_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.route_table_id().map(String::from)
    }

    /// Extract ec2.Route attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_attributes(
        obj: &aws_sdk_ec2::types::Route,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.destination_cidr_block() {
            attributes.insert(
                "destination_cidr_block".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.gateway_id() {
            attributes.insert(
                "gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.nat_gateway_id() {
            attributes.insert(
                "nat_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        None
    }

    /// Extract ec2.SecurityGroup attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroup,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.description() {
            attributes.insert(
                "description".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.group_id() {
            attributes.insert(
                "group_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.group_name() {
            attributes.insert(
                "group_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.group_id().map(String::from)
    }

    /// Extract ec2.SecurityGroupIngress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_ingress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.cidr_ipv6() {
            attributes.insert(
                "cidr_ipv6".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.description() {
            attributes.insert(
                "description".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.from_port() {
            attributes.insert(
                "from_port".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        if let Some(v) = obj.group_id() {
            attributes.insert(
                "group_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.ip_protocol() {
            attributes.insert(
                "ip_protocol".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.security_group_rule_id() {
            attributes.insert(
                "security_group_rule_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.prefix_list_id() {
            attributes.insert(
                "source_prefix_list_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.to_port() {
            attributes.insert(
                "to_port".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        obj.security_group_rule_id().map(String::from)
    }

    /// Extract ec2.SecurityGroupEgress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_egress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.cidr_ipv6() {
            attributes.insert(
                "cidr_ipv6".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.description() {
            attributes.insert(
                "description".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.prefix_list_id() {
            attributes.insert(
                "destination_prefix_list_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.from_port() {
            attributes.insert(
                "from_port".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        if let Some(v) = obj.group_id() {
            attributes.insert(
                "group_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.ip_protocol() {
            attributes.insert(
                "ip_protocol".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.security_group_rule_id() {
            attributes.insert(
                "security_group_rule_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.to_port() {
            attributes.insert(
                "to_port".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        obj.security_group_rule_id().map(String::from)
    }

    /// Extract ec2.EgressOnlyInternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_egress_only_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::EgressOnlyInternetGateway,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.egress_only_internet_gateway_id() {
            attributes.insert(
                "egress_only_internet_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(addr) = obj.attachments().first()
            && let Some(v) = addr.vpc_id()
        {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.egress_only_internet_gateway_id().map(String::from)
    }

    /// Extract ec2.Eip attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_eip_attributes(
        obj: &aws_sdk_ec2::types::Address,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.allocation_id() {
            attributes.insert(
                "allocation_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.domain() {
            attributes.insert(
                "domain".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.public_ip() {
            attributes.insert(
                "public_ip".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.public_ipv4_pool() {
            attributes.insert(
                "public_ipv4_pool".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.allocation_id().map(String::from)
    }

    /// Extract ec2.NatGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_nat_gateway_attributes(
        obj: &aws_sdk_ec2::types::NatGateway,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.availability_mode() {
            attributes.insert(
                "availability_mode".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.connectivity_type() {
            attributes.insert(
                "connectivity_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.nat_gateway_id() {
            attributes.insert(
                "nat_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.subnet_id() {
            attributes.insert(
                "subnet_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(addr) = obj.nat_gateway_addresses().first()
            && let Some(v) = addr.allocation_id()
        {
            attributes.insert(
                "allocation_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.nat_gateway_id().map(String::from)
    }

    /// Extract ec2.SubnetRouteTableAssociation attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_route_table_association_attributes(
        obj: &aws_sdk_ec2::types::RouteTableAssociation,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.public_ipv4_pool() {
            attributes.insert(
                "public_ipv4_pool".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.route_table_id() {
            attributes.insert(
                "route_table_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.subnet_id() {
            attributes.insert(
                "subnet_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        None
    }

    /// Extract ec2.TransitGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attributes(
        obj: &aws_sdk_ec2::types::TransitGateway,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.description() {
            attributes.insert(
                "description".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.transit_gateway_id() {
            attributes.insert(
                "transit_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.amazon_side_asn()
        {
            attributes.insert(
                "amazon_side_asn".to_string(),
                Value::Concrete(ConcreteValue::Int(v)),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.auto_accept_shared_attachments()
        {
            attributes.insert(
                "auto_accept_shared_attachments".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.default_route_table_association()
        {
            attributes.insert(
                "default_route_table_association".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.default_route_table_propagation()
        {
            attributes.insert(
                "default_route_table_propagation".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.dns_support()
        {
            attributes.insert(
                "dns_support".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(opts) = obj.options()
            && let Some(v) = opts.vpn_ecmp_support()
        {
            attributes.insert(
                "vpn_ecmp_support".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        obj.transit_gateway_id().map(String::from)
    }

    /// Extract ec2.TransitGatewayAttachment attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attachment_attributes(
        obj: &aws_sdk_ec2::types::TransitGatewayVpcAttachment,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        {
            let ids = obj.subnet_ids();
            if !ids.is_empty() {
                let list: Vec<Value> = ids
                    .iter()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string())))
                    .collect();
                attributes.insert(
                    "subnet_ids".to_string(),
                    Value::Concrete(ConcreteValue::List(list)),
                );
            }
        }
        if let Some(v) = obj.transit_gateway_attachment_id() {
            attributes.insert(
                "transit_gateway_attachment_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.transit_gateway_id() {
            attributes.insert(
                "transit_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.transit_gateway_attachment_id().map(String::from)
    }

    /// Extract ec2.VpcPeeringConnection attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_peering_connection_attributes(
        obj: &aws_sdk_ec2::types::VpcPeeringConnection,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.vpc_peering_connection_id() {
            attributes.insert(
                "vpc_peering_connection_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(opts) = obj.requester_vpc_info()
            && let Some(v) = opts.vpc_id()
        {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(opts) = obj.accepter_vpc_info()
            && let Some(v) = opts.vpc_id()
        {
            attributes.insert(
                "peer_vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(opts) = obj.accepter_vpc_info()
            && let Some(v) = opts.owner_id()
        {
            attributes.insert(
                "peer_owner_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(opts) = obj.accepter_vpc_info()
            && let Some(v) = opts.region()
        {
            attributes.insert(
                "peer_region".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.vpc_peering_connection_id().map(String::from)
    }

    /// Extract ec2.VpnGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpn_gateway_attributes(
        obj: &aws_sdk_ec2::types::VpnGateway,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.amazon_side_asn() {
            attributes.insert(
                "amazon_side_asn".to_string(),
                Value::Concrete(ConcreteValue::Int(v)),
            );
        }
        if let Some(v) = obj.availability_zone() {
            attributes.insert(
                "availability_zone".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.r#type() {
            attributes.insert(
                "type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.vpn_gateway_id() {
            attributes.insert(
                "vpn_gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        obj.vpn_gateway_id().map(String::from)
    }

    /// Extract route53.RecordSet attributes from SDK response type (generated)
    pub(crate) fn extract_route53_record_set_attributes(
        obj: &aws_sdk_route53::types::ResourceRecordSet,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        let v = obj.name();
        if !v.is_empty() {
            attributes.insert(
                "name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        Some(obj.name().to_string())
    }

    /// Extract logs.LogGroup attributes from SDK response type (generated)
    pub(crate) fn extract_logs_log_group_attributes(
        obj: &aws_sdk_cloudwatchlogs::types::LogGroup,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.deletion_protection_enabled() {
            attributes.insert(
                "deletion_protection_enabled".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.kms_key_id() {
            attributes.insert(
                "kms_key_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.log_group_class() {
            attributes.insert(
                "log_group_class".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.log_group_name() {
            attributes.insert(
                "log_group_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.retention_in_days() {
            attributes.insert(
                "retention_in_days".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        None
    }

    /// Extract acm.Certificate attributes from SDK response type (generated)
    pub(crate) fn extract_acm_certificate_attributes(
        obj: &aws_sdk_acm::types::CertificateDetail,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.certificate_arn() {
            attributes.insert(
                "certificate_arn".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.domain_name() {
            attributes.insert(
                "domain_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.key_algorithm() {
            attributes.insert(
                "key_algorithm".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.renewal_eligibility() {
            attributes.insert(
                "renewal_eligibility".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.status() {
            attributes.insert(
                "status".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        {
            let ids = obj.subject_alternative_names();
            if !ids.is_empty() {
                let list: Vec<Value> = ids
                    .iter()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string())))
                    .collect();
                attributes.insert(
                    "subject_alternative_names".to_string(),
                    Value::Concrete(ConcreteValue::List(list)),
                );
            }
        }
        if let Some(v) = obj.r#type() {
            attributes.insert(
                "type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        obj.certificate_arn().map(String::from)
    }
}

// ===== Generated DataSourceLookups Trait =====

/// One method per `DataSourceDef`. AwsProvider must implement all of
/// them; the codegen-emitted dispatcher below routes by
/// `resource.id.resource_type`.
pub trait DataSourceLookups {
    fn read_sts_caller_identity_data_source(
        &self,
        resource: &DataSource,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_identitystore_user_data_source(
        &self,
        resource: &DataSource,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_s3_bucket_data_source(
        &self,
        resource: &DataSource,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_iam_roles_data_source(
        &self,
        resource: &DataSource,
    ) -> BoxFuture<'_, ProviderResult<State>>;
}

// ===== Generated read_data_source dispatcher =====

/// Routes `Provider::read_data_source` calls to the matching
/// `DataSourceLookups` trait method. The default arm refuses
/// to drop user-supplied inputs silently.
pub(crate) fn dispatch_read_data_source<'a>(
    provider: &'a AwsProvider,
    resource: &'a DataSource,
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
