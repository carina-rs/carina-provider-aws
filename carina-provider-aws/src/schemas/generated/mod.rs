//! Auto-generated AWS provider resource schemas
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh

// Re-export all types and validators from config so that
// generated schema files can use `super::` to access them.
pub use super::config::*;

pub mod acm;
pub mod ec2;
pub mod iam;
pub mod identitystore;
pub mod kms;
pub mod logs;
pub mod organizations;
pub mod route53;
pub mod s3;
pub mod sqs;
pub mod sts;

/// Returns all generated schema configs
pub fn configs() -> Vec<AwsSchemaConfig> {
    vec![
        acm::certificate::acm_certificate_config(),
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
        iam::roles::iam_roles_config(),
        identitystore::user::identitystore_user_config(),
        logs::log_group::logs_log_group_config(),
        organizations::account::organizations_account_config(),
        organizations::organization::organizations_organization_config(),
        route53::record_set::route53_record_set_config(),
        s3::bucket::s3_bucket_config(),
        s3::bucket_acl::s3_bucket_acl_config(),
        s3::bucket_cors_configuration::s3_bucket_cors_configuration_config(),
        s3::bucket_data_source::s3_bucket_data_source_config(),
        s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config(),
        s3::bucket_logging::s3_bucket_logging_config(),
        s3::bucket_notification_configuration::s3_bucket_notification_configuration_config(),
        s3::bucket_object_lock_configuration::s3_bucket_object_lock_configuration_config(),
        s3::bucket_ownership_controls::s3_bucket_ownership_controls_config(),
        s3::bucket_policy::s3_bucket_policy_config(),
        s3::bucket_public_access_block::s3_bucket_public_access_block_config(),
        s3::bucket_replication_configuration::s3_bucket_replication_configuration_config(),
        s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config(),
        s3::bucket_versioning::s3_bucket_versioning_config(),
        s3::bucket_website_configuration::s3_bucket_website_configuration_config(),
        sqs::queue::sqs_queue_config(),
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
        acm::certificate::enum_valid_values(),
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
        iam::roles::enum_valid_values(),
        identitystore::user::enum_valid_values(),
        logs::log_group::enum_valid_values(),
        organizations::account::enum_valid_values(),
        organizations::organization::enum_valid_values(),
        route53::record_set::enum_valid_values(),
        s3::bucket::enum_valid_values(),
        s3::bucket_acl::enum_valid_values(),
        s3::bucket_cors_configuration::enum_valid_values(),
        s3::bucket_data_source::enum_valid_values(),
        s3::bucket_lifecycle_configuration::enum_valid_values(),
        s3::bucket_logging::enum_valid_values(),
        s3::bucket_notification_configuration::enum_valid_values(),
        s3::bucket_object_lock_configuration::enum_valid_values(),
        s3::bucket_ownership_controls::enum_valid_values(),
        s3::bucket_policy::enum_valid_values(),
        s3::bucket_public_access_block::enum_valid_values(),
        s3::bucket_replication_configuration::enum_valid_values(),
        s3::bucket_server_side_encryption_configuration::enum_valid_values(),
        s3::bucket_versioning::enum_valid_values(),
        s3::bucket_website_configuration::enum_valid_values(),
        sqs::queue::enum_valid_values(),
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
