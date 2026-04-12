//! Auto-generated AWS provider resource schemas
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

// Re-export all types and validators from types so that
// generated schema files can use `super::` to access them.
pub use super::types::*;

pub mod ec2;
pub mod iam;
pub mod identitystore;
pub mod logs;
pub mod organizations;
pub mod s3;
pub mod sts;

/// Returns all generated schema configs
pub fn configs() -> Vec<AwsSchemaConfig> {
    vec![
        ec2::egress_only_internet_gateway::ec2_egress_only_internet_gateway_config(),
        ec2::eip::ec2_eip_config(),
        ec2::flow_log::ec2_flow_log_config(),
        ec2::internet_gateway::ec2_internet_gateway_config(),
        ec2::nat_gateway::ec2_nat_gateway_config(),
        ec2::route::ec2_route_config(),
        ec2::route_table::ec2_route_table_config(),
        ec2::security_group::ec2_security_group_config(),
        ec2::security_group_egress::ec2_security_group_egress_config(),
        ec2::security_group_ingress::ec2_security_group_ingress_config(),
        ec2::subnet::ec2_subnet_config(),
        ec2::subnet_route_table_association::ec2_subnet_route_table_association_config(),
        ec2::transit_gateway::ec2_transit_gateway_config(),
        ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config(),
        ec2::vpc::ec2_vpc_config(),
        ec2::vpc_endpoint::ec2_vpc_endpoint_config(),
        ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config(),
        ec2::vpc_peering_connection::ec2_vpc_peering_connection_config(),
        ec2::vpn_gateway::ec2_vpn_gateway_config(),
        iam::role::iam_role_config(),
        identitystore::user::identitystore_user_config(),
        logs::log_group::logs_log_group_config(),
        organizations::account::organizations_account_config(),
        organizations::organization::organizations_organization_config(),
        s3::bucket::s3_bucket_config(),
        sts::caller_identity::sts_caller_identity_config(),
    ]
}

/// Get valid enum values for a given resource type and attribute name.
/// Used during read-back to normalize AWS-returned values to canonical DSL form.
///
/// Auto-generated from schema enum constants.
#[allow(clippy::type_complexity)]
pub fn get_enum_valid_values(
    resource_type: &str,
    attr_name: &str,
) -> Option<&'static [&'static str]> {
    let modules: &[(&str, &[(&str, &[&str])])] = &[
        ec2::egress_only_internet_gateway::enum_valid_values(),
        ec2::eip::enum_valid_values(),
        ec2::flow_log::enum_valid_values(),
        ec2::internet_gateway::enum_valid_values(),
        ec2::nat_gateway::enum_valid_values(),
        ec2::route::enum_valid_values(),
        ec2::route_table::enum_valid_values(),
        ec2::security_group::enum_valid_values(),
        ec2::security_group_egress::enum_valid_values(),
        ec2::security_group_ingress::enum_valid_values(),
        ec2::subnet::enum_valid_values(),
        ec2::subnet_route_table_association::enum_valid_values(),
        ec2::transit_gateway::enum_valid_values(),
        ec2::transit_gateway_attachment::enum_valid_values(),
        ec2::vpc::enum_valid_values(),
        ec2::vpc_endpoint::enum_valid_values(),
        ec2::vpc_gateway_attachment::enum_valid_values(),
        ec2::vpc_peering_connection::enum_valid_values(),
        ec2::vpn_gateway::enum_valid_values(),
        iam::role::enum_valid_values(),
        identitystore::user::enum_valid_values(),
        logs::log_group::enum_valid_values(),
        organizations::account::enum_valid_values(),
        organizations::organization::enum_valid_values(),
        s3::bucket::enum_valid_values(),
        sts::caller_identity::enum_valid_values(),
    ];
    for (rt, attrs) in modules {
        if *rt == resource_type {
            for (attr, values) in *attrs {
                if *attr == attr_name {
                    return Some(values);
                }
            }
            return None;
        }
    }
    None
}

/// Maps DSL alias values back to canonical AWS values.
/// Dispatches to per-module enum_alias_reverse() functions.
pub fn get_enum_alias_reverse(
    resource_type: &str,
    attr_name: &str,
    value: &str,
) -> Option<&'static str> {
    if resource_type == "ec2.egress_only_internet_gateway" {
        return ec2::egress_only_internet_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.eip" {
        return ec2::eip::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.flow_log" {
        return ec2::flow_log::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.internet_gateway" {
        return ec2::internet_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.nat_gateway" {
        return ec2::nat_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.route" {
        return ec2::route::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.route_table" {
        return ec2::route_table::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.security_group" {
        return ec2::security_group::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.security_group_egress" {
        return ec2::security_group_egress::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.security_group_ingress" {
        return ec2::security_group_ingress::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.subnet" {
        return ec2::subnet::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.subnet_route_table_association" {
        return ec2::subnet_route_table_association::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.transit_gateway" {
        return ec2::transit_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.transit_gateway_attachment" {
        return ec2::transit_gateway_attachment::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.vpc" {
        return ec2::vpc::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.vpc_endpoint" {
        return ec2::vpc_endpoint::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.vpc_gateway_attachment" {
        return ec2::vpc_gateway_attachment::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.vpc_peering_connection" {
        return ec2::vpc_peering_connection::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.vpn_gateway" {
        return ec2::vpn_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "iam.role" {
        return iam::role::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "identitystore.user" {
        return identitystore::user::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "logs.log_group" {
        return logs::log_group::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "organizations.account" {
        return organizations::account::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "organizations.organization" {
        return organizations::organization::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.bucket" {
        return s3::bucket::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "sts.caller_identity" {
        return sts::caller_identity::enum_alias_reverse(attr_name, value);
    }
    None
}

/// Build a complete enum aliases map for all resource types.
/// Returns: resource_type -> attr_name -> alias -> canonical_value.
/// Used by CarinaProvider::enum_aliases() for the WASM host cache.
pub fn build_enum_aliases_map() -> std::collections::HashMap<
    String,
    std::collections::HashMap<String, std::collections::HashMap<String, String>>,
> {
    let mut map = std::collections::HashMap::new();
    for (attr, alias, canonical) in ec2::egress_only_internet_gateway::enum_alias_entries() {
        map.entry("ec2.egress_only_internet_gateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::eip::enum_alias_entries() {
        map.entry("ec2.eip".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::flow_log::enum_alias_entries() {
        map.entry("ec2.flow_log".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::internet_gateway::enum_alias_entries() {
        map.entry("ec2.internet_gateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::nat_gateway::enum_alias_entries() {
        map.entry("ec2.nat_gateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::route::enum_alias_entries() {
        map.entry("ec2.route".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::route_table::enum_alias_entries() {
        map.entry("ec2.route_table".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group::enum_alias_entries() {
        map.entry("ec2.security_group".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group_egress::enum_alias_entries() {
        map.entry("ec2.security_group_egress".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group_ingress::enum_alias_entries() {
        map.entry("ec2.security_group_ingress".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::subnet::enum_alias_entries() {
        map.entry("ec2.subnet".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::subnet_route_table_association::enum_alias_entries() {
        map.entry("ec2.subnet_route_table_association".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::transit_gateway::enum_alias_entries() {
        map.entry("ec2.transit_gateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::transit_gateway_attachment::enum_alias_entries() {
        map.entry("ec2.transit_gateway_attachment".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc::enum_alias_entries() {
        map.entry("ec2.vpc".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_endpoint::enum_alias_entries() {
        map.entry("ec2.vpc_endpoint".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_gateway_attachment::enum_alias_entries() {
        map.entry("ec2.vpc_gateway_attachment".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_peering_connection::enum_alias_entries() {
        map.entry("ec2.vpc_peering_connection".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpn_gateway::enum_alias_entries() {
        map.entry("ec2.vpn_gateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in iam::role::enum_alias_entries() {
        map.entry("iam.role".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in identitystore::user::enum_alias_entries() {
        map.entry("identitystore.user".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in logs::log_group::enum_alias_entries() {
        map.entry("logs.log_group".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in organizations::account::enum_alias_entries() {
        map.entry("organizations.account".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in organizations::organization::enum_alias_entries() {
        map.entry("organizations.organization".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket::enum_alias_entries() {
        map.entry("s3.bucket".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in sts::caller_identity::enum_alias_entries() {
        map.entry("sts.caller_identity".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    map
}
