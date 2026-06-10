//! AWS schema configuration and validator registry
//!
//! This module re-exports shared AWS type validators from `carina-aws-types`
//! and defines provider-side schema configuration.

pub use carina_aws_types::*;

use std::collections::HashMap;

use carina_core::schema::ResourceSchema;

/// AWS schema configuration
///
/// Combines the generated ResourceSchema with AWS-specific metadata.
pub struct AwsSchemaConfig {
    /// AWS CloudFormation type name (e.g., "AWS::EC2::VPC")
    pub aws_type_name: &'static str,
    /// Resource type name used in DSL (e.g., "ec2_vpc")
    pub resource_type_name: &'static str,
    /// Whether this resource type uses tags
    pub has_tags: bool,
    /// The resource schema with attribute definitions
    pub schema: ResourceSchema,
}

/// Type alias for custom type validator functions.
pub type CustomValidatorFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

fn strip_availability_zone_prefix(value: &str) -> &str {
    value
        .strip_prefix("aws.AvailabilityZone.ZoneName.")
        .or_else(|| value.strip_prefix("ZoneName."))
        .unwrap_or(value)
}

/// Return all AWS type validators for use in `validate_custom_type`.
///
/// These validators are keyed by type name (matching the names used in schema
/// `AttributeType::Custom` definitions) and wrap the validation functions from
/// `carina-aws-types`.
pub fn aws_validators() -> HashMap<String, CustomValidatorFn> {
    /// Register AWS type validators declaratively.
    ///
    /// Generates a `HashMap<String, CustomValidatorFn>` from three categories:
    /// - `simple`: single-arg validators (`name => function`)
    /// - `prefixed`: prefixed resource ID validators (`name => prefix`)
    /// - `service_arn`: arbitrary closure validators (`name => closure_expr`)
    macro_rules! register_validators {
        (
            simple { $( $s_name:ident => $s_fn:expr ),* $(,)? }
            prefixed { $( $p_name:ident => $p_prefix:expr ),* $(,)? }
            service_arn { $( $a_name:ident => $a_expr:expr ),* $(,)? }
        ) => {{
            let mut m: HashMap<String, CustomValidatorFn> = HashMap::new();
            $( m.insert(stringify!($s_name).to_string(), Box::new(|s: &str| ($s_fn)(s))); )*
            $( m.insert(stringify!($p_name).to_string(), Box::new(|s: &str| validate_prefixed_resource_id(s, $p_prefix))); )*
            $( m.insert(stringify!($a_name).to_string(), Box::new($a_expr)); )*
            m
        }};
    }

    register_validators! {
        simple {
            arn => validate_arn,
            aws_resource_id => validate_aws_resource_id,
            iam_role_id => validate_iam_role_id,
            aws_account_id => validate_aws_account_id,
            kms_key_id => validate_kms_key_id,
            ipam_pool_id => validate_ipam_pool_id,
            availability_zone_id => validate_availability_zone_id,
        }
        prefixed {
            vpc_id => "vpc",
            subnet_id => "subnet",
            security_group_id => "sg",
            internet_gateway_id => "igw",
            route_table_id => "rtb",
            nat_gateway_id => "nat",
            transit_gateway_id => "tgw",
            vpn_gateway_id => "vgw",
            network_interface_id => "eni",
            allocation_id => "eipalloc",
            vpc_endpoint_id => "vpce",
            vpc_peering_connection_id => "pcx",
            instance_id => "i",
            prefix_list_id => "pl",
            carrier_gateway_id => "cagw",
            local_gateway_id => "lgw",
            network_acl_id => "acl",
            transit_gateway_attachment_id => "tgw-attach",
            flow_log_id => "fl",
            ipam_id => "ipam",
            subnet_route_table_association_id => "rtbassoc",
            security_group_rule_id => "sgr",
            vpc_cidr_block_association_id => "vpc-cidr-assoc",
            tgw_route_table_id => "tgw-rtb",
            egress_only_internet_gateway_id => "eigw",
        }
        service_arn {
            iam_role_arn => |s: &str| validate_iam_arn(s, "role/"),
            iam_policy_arn => |s: &str| validate_iam_arn(s, "policy/"),
            kms_key_arn => |s: &str| validate_kms_key_id(s),
            // The availability_zone validator must accept both the DSL
            // namespaced form (`aws.AvailabilityZone.ZoneName.ap_northeast_1a`) and the
            // raw AWS string format (`"ap-northeast-1a"`). The single-arg
            // `validate_availability_zone` from carina-aws-types only knows
            // the raw form, so we wrap it: strip the namespace prefix if
            // present, then convert underscores to hyphens before validating.
            availability_zone => |s: &str| {
                if s.contains('.') {
                    let id = provider_bare_type(&["AvailabilityZone"], "ZoneName");
                    carina_core::utils::validate_enum_namespace(s, &id)
                        .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))?;
                }
                let extracted = strip_availability_zone_prefix(s);
                let normalized = extracted.replace('_', "-");
                validate_availability_zone(&normalized)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // aws_validators tests

    #[test]
    fn aws_validators_all_registered() {
        let validators = aws_validators();

        let expected_simple = [
            "arn",
            "aws_resource_id",
            "iam_role_id",
            "aws_account_id",
            "kms_key_id",
            "ipam_pool_id",
            "availability_zone_id",
        ];

        let expected_prefixed = [
            "vpc_id",
            "subnet_id",
            "security_group_id",
            "internet_gateway_id",
            "route_table_id",
            "nat_gateway_id",
            "transit_gateway_id",
            "vpn_gateway_id",
            "network_interface_id",
            "allocation_id",
            "vpc_endpoint_id",
            "vpc_peering_connection_id",
            "instance_id",
            "prefix_list_id",
            "carrier_gateway_id",
            "local_gateway_id",
            "network_acl_id",
            "transit_gateway_attachment_id",
            "flow_log_id",
            "ipam_id",
            "subnet_route_table_association_id",
            "security_group_rule_id",
            "vpc_cidr_block_association_id",
            "tgw_route_table_id",
            "egress_only_internet_gateway_id",
        ];

        let expected_arn = [
            "iam_role_arn",
            "iam_policy_arn",
            "kms_key_arn",
            "availability_zone",
        ];

        let mut all_expected: Vec<&str> = Vec::new();
        all_expected.extend_from_slice(&expected_simple);
        all_expected.extend_from_slice(&expected_prefixed);
        all_expected.extend_from_slice(&expected_arn);

        for name in &all_expected {
            assert!(
                validators.contains_key(*name),
                "Missing validator: {}",
                name
            );
        }

        assert_eq!(
            validators.len(),
            all_expected.len(),
            "Validator count mismatch: expected {}, got {}. Extra keys: {:?}",
            all_expected.len(),
            validators.len(),
            validators
                .keys()
                .filter(|k| !all_expected.contains(&k.as_str()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aws_validators_produce_correct_results() {
        let validators = aws_validators();

        // Test a prefixed resource ID validator
        let vpc_validator = validators.get("vpc_id").unwrap();
        assert!(vpc_validator("vpc-12345678").is_ok());
        assert!(vpc_validator("subnet-12345678").is_err());

        // Test a single-arg validator
        let arn_validator = validators.get("arn").unwrap();
        assert!(arn_validator("arn:aws:s3:::my-bucket").is_ok());
        assert!(arn_validator("not-an-arn").is_err());

        // Test a service ARN validator
        let iam_role_arn_validator = validators.get("iam_role_arn").unwrap();
        assert!(iam_role_arn_validator("arn:aws:iam::123456789012:role/my-role").is_ok());
        assert!(iam_role_arn_validator("arn:aws:s3:::my-bucket").is_err());

        // Test unknown type returns no validator (not an error)
        assert!(!validators.contains_key("unknown_type"));
    }

    // The provider-side validate_custom_type entry point passes the raw DSL
    // string straight through, so the availability_zone validator must accept
    // both the namespaced DSL form and the raw AWS string form (mirrors the
    // Region validator's accept-both behavior; see CLAUDE.md "Validation
    // Formats"). Reproduces the failure surfaced by acceptance test
    // ec2_subnet/basic.crn against pre-fix main.
    #[test]
    fn aws_validators_availability_zone_accepts_dsl_and_aws_format() {
        let validators = aws_validators();
        let az = validators.get("availability_zone").unwrap();

        // Raw AWS format
        assert!(az("us-east-1a").is_ok());
        assert!(az("ap-northeast-1c").is_ok());

        // Fully-qualified DSL format (the form acceptance tests use)
        // — the structured identity is `aws.AvailabilityZone.ZoneName`
        // for the zone-name variant.
        assert!(az("aws.AvailabilityZone.ZoneName.ap_northeast_1a").is_ok());
        assert!(az("aws.AvailabilityZone.ZoneName.us_east_1a").is_ok());

        // Shorthand DSL form (TypeName.value, where TypeName == kind)
        assert!(az("ZoneName.ap_northeast_1a").is_ok());

        // Rejects non-AZ strings (regions, garbage)
        assert!(az("us-east-1").is_err()); // region, not AZ
        assert!(az("not-an-az").is_err());

        // Rejects wrong namespace
        assert!(az("aws.Region.ap_northeast_1").is_err());
    }
}
