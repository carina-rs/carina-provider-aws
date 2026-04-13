//! AWS-specific type definitions and validators
//!
//! This module re-exports shared AWS type validators from `carina-aws-types`
//! and defines provider-specific types (region, availability zone, schema config).

pub use carina_aws_types::*;

use std::collections::HashMap;

use carina_core::resource::Value;
use carina_core::schema::{AttributeType, ResourceSchema};
use carina_core::utils::{extract_enum_value, validate_enum_namespace};

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

/// AWS region type with custom validation
/// Accepts:
/// - DSL format: aws.Region.ap_northeast_1
/// - AWS string format: "ap-northeast-1"
/// - Shorthand: ap_northeast_1
pub fn aws_region() -> AttributeType {
    AttributeType::Custom {
        name: "Region".to_string(),
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_enum_namespace(s, "Region", "aws")
                    .map_err(|reason| format!("Invalid region '{}': {}", s, reason))?;
                // Normalize the input to AWS format (hyphens)
                let normalized = extract_enum_value(s).replace('_', "-");
                if is_valid_region(&normalized) {
                    Ok(())
                } else {
                    Err(format!(
                        "Invalid region '{}', expected one of: {} or DSL format like aws.Region.ap_northeast_1",
                        s,
                        valid_regions_display()
                    ))
                }
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: Some("aws".to_string()),
        to_dsl: Some(|s: &str| s.replace('-', "_")),
    }
}

/// Availability zone type with validation (e.g., "us-east-1a")
/// Accepts:
/// - DSL format: aws.AvailabilityZone.us_east_1a
/// - AWS string format: "us-east-1a"
/// - Shorthand: us_east_1a
pub fn availability_zone() -> AttributeType {
    AttributeType::Custom {
        name: "AvailabilityZone".to_string(),
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_enum_namespace(s, "AvailabilityZone", "aws")
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))?;
                let extracted = extract_enum_value(s);
                let normalized = extracted.replace('_', "-");
                validate_availability_zone(&normalized)
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: Some("aws".to_string()),
        to_dsl: Some(|s: &str| s.replace('-', "_")),
    }
}

/// S3 grantee specification type with validation
///
/// Validates that the value contains at least one grantee spec in the format:
/// - `id="canonical-user-id"`
/// - `emailAddress="user@example.com"`
/// - `uri="http://acs.amazonaws.com/groups/global/AllUsers"`
///
/// Multiple grantees can be comma-separated.
pub fn s3_grantee() -> AttributeType {
    AttributeType::Custom {
        name: "S3Grantee".to_string(),
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                if s.is_empty() {
                    return Err("Grantee specification must not be empty".to_string());
                }
                // Split by comma and validate each grantee spec
                for part in s.split(',') {
                    let trimmed = part.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let valid_prefixes = ["id=", "emailAddress=", "uri="];
                    if !valid_prefixes.iter().any(|p| trimmed.starts_with(p)) {
                        return Err(format!(
                            "Invalid grantee spec '{}': must start with id=, emailAddress=, or uri=",
                            trimmed
                        ));
                    }
                }
                Ok(())
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// iam_policy_document() is provided by `pub use carina_aws_types::*` above

/// Type alias for custom type validator functions.
pub type CustomValidatorFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

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

/// Return all AWS type validators for use in `validate_custom_type`.
///
/// These validators are keyed by type name (matching the names used in schema
/// `AttributeType::Custom` definitions) and wrap the validation functions from
/// `carina-aws-types`.
pub fn aws_validators() -> HashMap<String, CustomValidatorFn> {
    register_validators! {
        simple {
            arn => validate_arn,
            availability_zone => validate_availability_zone,
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
            iam_role_arn => |s: &str| validate_service_arn(s, "iam", Some("role/")),
            iam_policy_arn => |s: &str| validate_service_arn(s, "iam", Some("policy/")),
            kms_key_arn => |s: &str| validate_kms_key_id(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Region validation tests

    #[test]
    fn region_accepts_aws_format() {
        let region_type = aws_region();
        assert!(
            region_type
                .validate(&Value::String("ap-northeast-1".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format() {
        let region_type = aws_region();
        assert!(
            region_type
                .validate(&Value::String("aws.Region.ap_northeast_1".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format_without_aws_prefix() {
        let region_type = aws_region();
        assert!(
            region_type
                .validate(&Value::String("Region.ap_northeast_1".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn region_rejects_invalid_region() {
        let region_type = aws_region();
        let result = region_type.validate(&Value::String("invalid-region".to_string()));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid region"));
        assert!(err.contains("ap-northeast-1")); // Should suggest valid regions
    }

    #[test]
    fn region_rejects_availability_zone() {
        let region_type = aws_region();
        // ap-northeast-1a is an AZ, not a region
        assert!(
            region_type
                .validate(&Value::String("ap-northeast-1a".to_string()))
                .is_err()
        );
    }

    #[test]
    fn region_validates_all_valid_regions() {
        let region_type = aws_region();
        for (region, _) in REGIONS {
            assert!(
                region_type
                    .validate(&Value::String(region.to_string()))
                    .is_ok(),
                "Region {} should be valid",
                region
            );
        }
    }

    // Availability zone validation tests

    #[test]
    fn az_accepts_aws_format() {
        let az_type = availability_zone();
        assert!(
            az_type
                .validate(&Value::String("us-east-1a".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_dsl_format() {
        let az_type = availability_zone();
        assert!(
            az_type
                .validate(&Value::String(
                    "aws.AvailabilityZone.us_east_1a".to_string()
                ))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_shorthand_format() {
        let az_type = availability_zone();
        assert!(
            az_type
                .validate(&Value::String("us_east_1a".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn az_rejects_invalid_az() {
        let az_type = availability_zone();
        assert!(
            az_type
                .validate(&Value::String("invalid-zone".to_string()))
                .is_err()
        );
    }

    #[test]
    fn az_rejects_wrong_namespace() {
        let az_type = availability_zone();
        assert!(
            az_type
                .validate(&Value::String(
                    "gcp.AvailabilityZone.us_east_1a".to_string()
                ))
                .is_err()
        );
    }

    #[test]
    fn az_has_namespace() {
        let az_type = availability_zone();
        if let AttributeType::Custom { namespace, .. } = &az_type {
            assert_eq!(namespace.as_deref(), Some("aws"));
        } else {
            panic!("Expected Custom type");
        }
    }

    #[test]
    fn az_has_to_dsl() {
        let az_type = availability_zone();
        if let AttributeType::Custom { to_dsl, .. } = &az_type {
            assert!(to_dsl.is_some());
            let convert = to_dsl.unwrap();
            assert_eq!(convert("us-east-1a"), "us_east_1a");
        } else {
            panic!("Expected Custom type");
        }
    }

    // S3 grantee validation tests

    #[test]
    fn grantee_accepts_id_format() {
        let t = s3_grantee();
        assert!(
            t.validate(&Value::String(
                "id=\"79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be\""
                    .to_string()
            ))
            .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_email_format() {
        let t = s3_grantee();
        assert!(
            t.validate(&Value::String(
                "emailAddress=\"user@example.com\"".to_string()
            ))
            .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_uri_format() {
        let t = s3_grantee();
        assert!(
            t.validate(&Value::String(
                "uri=\"http://acs.amazonaws.com/groups/global/AllUsers\"".to_string()
            ))
            .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_multiple_specs() {
        let t = s3_grantee();
        assert!(
            t.validate(&Value::String(
                "id=\"abc123\", emailAddress=\"user@example.com\"".to_string()
            ))
            .is_ok()
        );
    }

    #[test]
    fn grantee_rejects_empty_string() {
        let t = s3_grantee();
        assert!(t.validate(&Value::String("".to_string())).is_err());
    }

    #[test]
    fn grantee_rejects_invalid_prefix() {
        let t = s3_grantee();
        let result = t.validate(&Value::String("foo=\"bar\"".to_string()));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must start with id=, emailAddress=, or uri="));
    }

    #[test]
    fn region_rejects_wrong_namespace() {
        let region_type = aws_region();
        assert!(
            region_type
                .validate(&Value::String("gcp.Region.ap_northeast_1".to_string()))
                .is_err()
        );
        assert!(
            region_type
                .validate(&Value::String("aws.Location.ap_northeast_1".to_string()))
                .is_err()
        );
        assert!(
            region_type
                .validate(&Value::String("foo.bar.baz.ap_northeast_1".to_string()))
                .is_err()
        );
        assert!(
            region_type
                .validate(&Value::String("Location.ap_northeast_1".to_string()))
                .is_err()
        );
    }

    // aws_validators tests

    #[test]
    fn aws_validators_all_registered() {
        let validators = aws_validators();

        let expected_simple = [
            "arn",
            "availability_zone",
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

        let expected_arn = ["iam_role_arn", "iam_policy_arn", "kms_key_arn"];

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
}
