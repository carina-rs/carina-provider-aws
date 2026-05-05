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
pub mod route53;
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
        route53::record_set::route53_record_set_config(),
        s3::bucket::s3_bucket_config(),
        s3::bucket_acl::s3_bucket_acl_config(),
        s3::bucket_data_source::s3_bucket_data_source_config(),
        s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config(),
        s3::bucket_ownership_controls::s3_bucket_ownership_controls_config(),
        s3::bucket_policy::s3_bucket_policy_config(),
        s3::bucket_public_access_block::s3_bucket_public_access_block_config(),
        s3::bucket_replication_configuration::s3_bucket_replication_configuration_config(),
        s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config(),
        s3::bucket_versioning::s3_bucket_versioning_config(),
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
        route53::record_set::enum_valid_values(),
        s3::bucket::enum_valid_values(),
        s3::bucket_acl::enum_valid_values(),
        s3::bucket_data_source::enum_valid_values(),
        s3::bucket_lifecycle_configuration::enum_valid_values(),
        s3::bucket_ownership_controls::enum_valid_values(),
        s3::bucket_policy::enum_valid_values(),
        s3::bucket_public_access_block::enum_valid_values(),
        s3::bucket_replication_configuration::enum_valid_values(),
        s3::bucket_server_side_encryption_configuration::enum_valid_values(),
        s3::bucket_versioning::enum_valid_values(),
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
    if resource_type == "ec2.EgressOnlyInternetGateway" {
        return ec2::egress_only_internet_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.Eip" {
        return ec2::eip::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.FlowLog" {
        return ec2::flow_log::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.InternetGateway" {
        return ec2::internet_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.NatGateway" {
        return ec2::nat_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.Route" {
        return ec2::route::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.RouteTable" {
        return ec2::route_table::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.SecurityGroup" {
        return ec2::security_group::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.SecurityGroupEgress" {
        return ec2::security_group_egress::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.SecurityGroupIngress" {
        return ec2::security_group_ingress::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.Subnet" {
        return ec2::subnet::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.SubnetRouteTableAssociation" {
        return ec2::subnet_route_table_association::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.TransitGateway" {
        return ec2::transit_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.TransitGatewayAttachment" {
        return ec2::transit_gateway_attachment::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.Vpc" {
        return ec2::vpc::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.VpcEndpoint" {
        return ec2::vpc_endpoint::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.VpcGatewayAttachment" {
        return ec2::vpc_gateway_attachment::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.VpcPeeringConnection" {
        return ec2::vpc_peering_connection::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "ec2.VpnGateway" {
        return ec2::vpn_gateway::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "iam.Role" {
        return iam::role::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "logs.LogGroup" {
        return logs::log_group::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "organizations.Account" {
        return organizations::account::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "organizations.Organization" {
        return organizations::organization::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "route53.RecordSet" {
        return route53::record_set::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.Bucket" {
        return s3::bucket::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketAcl" {
        return s3::bucket_acl::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketLifecycleConfiguration" {
        return s3::bucket_lifecycle_configuration::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketOwnershipControls" {
        return s3::bucket_ownership_controls::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketPolicy" {
        return s3::bucket_policy::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketPublicAccessBlock" {
        return s3::bucket_public_access_block::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketReplicationConfiguration" {
        return s3::bucket_replication_configuration::enum_alias_reverse(attr_name, value);
    }
    if resource_type == "s3.BucketServerSideEncryptionConfiguration" {
        return s3::bucket_server_side_encryption_configuration::enum_alias_reverse(
            attr_name, value,
        );
    }
    if resource_type == "s3.BucketVersioning" {
        return s3::bucket_versioning::enum_alias_reverse(attr_name, value);
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
        map.entry("ec2.EgressOnlyInternetGateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::eip::enum_alias_entries() {
        map.entry("ec2.Eip".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::flow_log::enum_alias_entries() {
        map.entry("ec2.FlowLog".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::internet_gateway::enum_alias_entries() {
        map.entry("ec2.InternetGateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::nat_gateway::enum_alias_entries() {
        map.entry("ec2.NatGateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::route::enum_alias_entries() {
        map.entry("ec2.Route".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::route_table::enum_alias_entries() {
        map.entry("ec2.RouteTable".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group::enum_alias_entries() {
        map.entry("ec2.SecurityGroup".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group_egress::enum_alias_entries() {
        map.entry("ec2.SecurityGroupEgress".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::security_group_ingress::enum_alias_entries() {
        map.entry("ec2.SecurityGroupIngress".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::subnet::enum_alias_entries() {
        map.entry("ec2.Subnet".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::subnet_route_table_association::enum_alias_entries() {
        map.entry("ec2.SubnetRouteTableAssociation".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::transit_gateway::enum_alias_entries() {
        map.entry("ec2.TransitGateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::transit_gateway_attachment::enum_alias_entries() {
        map.entry("ec2.TransitGatewayAttachment".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc::enum_alias_entries() {
        map.entry("ec2.Vpc".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_endpoint::enum_alias_entries() {
        map.entry("ec2.VpcEndpoint".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_gateway_attachment::enum_alias_entries() {
        map.entry("ec2.VpcGatewayAttachment".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpc_peering_connection::enum_alias_entries() {
        map.entry("ec2.VpcPeeringConnection".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in ec2::vpn_gateway::enum_alias_entries() {
        map.entry("ec2.VpnGateway".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in iam::role::enum_alias_entries() {
        map.entry("iam.Role".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in logs::log_group::enum_alias_entries() {
        map.entry("logs.LogGroup".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in organizations::account::enum_alias_entries() {
        map.entry("organizations.Account".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in organizations::organization::enum_alias_entries() {
        map.entry("organizations.Organization".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in route53::record_set::enum_alias_entries() {
        map.entry("route53.RecordSet".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket::enum_alias_entries() {
        map.entry("s3.Bucket".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_acl::enum_alias_entries() {
        map.entry("s3.BucketAcl".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_lifecycle_configuration::enum_alias_entries() {
        map.entry("s3.BucketLifecycleConfiguration".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_ownership_controls::enum_alias_entries() {
        map.entry("s3.BucketOwnershipControls".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_policy::enum_alias_entries() {
        map.entry("s3.BucketPolicy".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_public_access_block::enum_alias_entries() {
        map.entry("s3.BucketPublicAccessBlock".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_replication_configuration::enum_alias_entries() {
        map.entry("s3.BucketReplicationConfiguration".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in
        s3::bucket_server_side_encryption_configuration::enum_alias_entries()
    {
        map.entry("s3.BucketServerSideEncryptionConfiguration".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    for (attr, alias, canonical) in s3::bucket_versioning::enum_alias_entries() {
        map.entry("s3.BucketVersioning".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .entry(attr.to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(alias.to_string(), canonical.to_string());
    }
    map
}
