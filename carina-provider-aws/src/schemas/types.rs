//! AWS-specific type definitions and validators
//!
//! This module re-exports shared AWS type validators from `carina-aws-types`
//! and defines provider-specific types (region, availability zone, schema config).

pub use carina_aws_types::*;

use std::collections::HashMap;

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, ResourceSchema, TypeIdentity, legacy_validator};
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

/// CloudFront's fixed Route 53 hosted zone ID, surfaced as a namespaced
/// constant.
///
/// AWS publishes `Z2FDTNDATAQYW2` as the global hosted zone ID for every
/// CloudFront distribution — it never varies by region or account.
/// Users were hard-coding the magic string in their `.crn` files; this
/// type lets them write `aws.cloudfront.HostedZoneId.global` instead,
/// which the normalizer resolves to the literal value before reaching
/// the SDK (aws#302).
///
/// Accepts:
/// - Namespaced identifier: `aws.cloudfront.HostedZoneId.global`
/// - Literal AWS spelling: `Z2FDTNDATAQYW2`
///
/// Any other value is a type error — `alias_target.hosted_zone_id`
/// expects an AWS-published constant for the alias target's service,
/// not an arbitrary string.
pub fn cloudfront_hosted_zone_id() -> AttributeType {
    AttributeType::string_enum(
        "HostedZoneId".to_string(),
        vec!["Z2FDTNDATAQYW2".to_string()],
        Some(carina_core::schema::string_enum_identity(
            "HostedZoneId",
            Some("aws.cloudfront"),
        )),
        vec![("Z2FDTNDATAQYW2".to_string(), "global".to_string())],
    )
}

/// AWS region type with custom validation
/// Accepts:
/// - DSL format: aws.Region.ap_northeast_1
/// - AWS string format: "ap-northeast-1"
/// - Shorthand: ap_northeast_1
pub fn aws_region() -> AttributeType {
    AttributeType::custom_enum(
        TypeIdentity::new(Some("aws"), Vec::<String>::new(), "Region"),
        AttributeType::string(),
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                let id = TypeIdentity::new(Some("aws"), Vec::<String>::new(), "Region");
                validate_enum_namespace(s, &id)
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
        }),
        Some(|s: &str| s.replace('-', "_")),
    )
}

/// Availability zone type with validation (e.g., "us-east-1a")
/// Accepts:
/// - DSL format: aws.AvailabilityZone.us_east_1a
/// - AWS string format: "us-east-1a"
/// - Shorthand: us_east_1a
pub fn availability_zone() -> AttributeType {
    AttributeType::custom_enum(
        TypeIdentity::new(Some("aws"), ["AvailabilityZone"], "ZoneName"),
        AttributeType::string(),
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                let id = TypeIdentity::new(Some("aws"), ["AvailabilityZone"], "ZoneName");
                validate_enum_namespace(s, &id)
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))?;
                let extracted = extract_enum_value(s);
                let normalized = extracted.replace('_', "-");
                validate_availability_zone(&normalized)
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        Some(|s: &str| s.replace('-', "_")),
    )
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
    AttributeType::custom(
        Some(TypeIdentity::new(Some("aws"), ["s3"], "Grantee")),
        AttributeType::string(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
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
        }),
        None,
    )
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
            // namespaced form (`aws.AvailabilityZone.ap_northeast_1a`) and the
            // raw AWS string format (`"ap-northeast-1a"`). The single-arg
            // `validate_availability_zone` from carina-aws-types only knows
            // the raw form, so we wrap it: strip the namespace prefix if
            // present, then convert underscores to hyphens before validating.
            availability_zone => |s: &str| {
                if s.contains('.') {
                    let id = TypeIdentity::new(Some("aws"), ["AvailabilityZone"], "ZoneName");
                    carina_core::utils::validate_enum_namespace(s, &id)
                        .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))?;
                }
                let extracted = carina_core::utils::extract_enum_value(s);
                let normalized = extracted.replace('_', "-");
                validate_availability_zone(&normalized)
            },
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
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ap-northeast-1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.Region.ap_northeast_1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format_without_aws_prefix() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "Region.ap_northeast_1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_rejects_invalid_region() {
        let region_type = aws_region();
        let result = carina_core::schema::Schema::flat(region_type.clone()).validate(
            &Value::Concrete(ConcreteValue::String("invalid-region".to_string())),
        );
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
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ap-northeast-1a".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn region_validates_all_valid_regions() {
        let region_type = aws_region();
        for (region, _) in REGIONS {
            assert!(
                carina_core::schema::Schema::flat(region_type.clone())
                    .validate(&Value::Concrete(ConcreteValue::String(region.to_string())))
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
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "us-east-1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_dsl_format() {
        let az_type = availability_zone();
        // The full DSL form for the zone-name variant carries the
        // `ZoneName` kind segment — `aws.AvailabilityZone.ZoneName.<v>` —
        // matching the structured identity's dotted display.
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.AvailabilityZone.ZoneName.us_east_1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_shorthand_format() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "us_east_1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_rejects_invalid_az() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "invalid-zone".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn az_rejects_wrong_namespace() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "gcp.AvailabilityZone.us_east_1a".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn az_has_namespace() {
        // Post-#3222: AZ is a `CustomEnum`, not a `Custom`. The legacy
        // `namespace: Some("aws")` field is now derived from the
        // structured identity via `dotted_prefix()`.
        let az_type = availability_zone();
        if let carina_core::schema::Shape::CustomEnum { identity, .. } =
            az_type.shape(carina_core::schema::empty_defs())
        {
            assert_eq!(
                identity.dotted_prefix().as_deref(),
                Some("aws.AvailabilityZone")
            );
        } else {
            panic!("Expected CustomEnum type");
        }
    }

    #[test]
    fn az_has_to_dsl() {
        let az_type = availability_zone();
        if let carina_core::schema::Shape::CustomEnum { to_dsl, .. } =
            az_type.shape(carina_core::schema::empty_defs())
        {
            assert!(to_dsl.is_some());
            let convert = to_dsl.unwrap();
            assert_eq!(convert("us-east-1a"), "us_east_1a");
        } else {
            panic!("Expected CustomEnum type");
        }
    }

    // S3 grantee validation tests

    #[test]
    fn grantee_accepts_id_format() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "id=\"79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be\""
                        .to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_email_format() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "emailAddress=\"user@example.com\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_uri_format() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "uri=\"http://acs.amazonaws.com/groups/global/AllUsers\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_multiple_specs() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "id=\"abc123\", emailAddress=\"user@example.com\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_rejects_empty_string() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String("".to_string())))
                .is_err()
        );
    }

    #[test]
    fn grantee_rejects_invalid_prefix() {
        let t = s3_grantee();
        let result = carina_core::schema::Schema::flat(t.clone()).validate(&Value::Concrete(
            ConcreteValue::String("foo=\"bar\"".to_string()),
        ));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must start with id=, emailAddress=, or uri="));
    }

    #[test]
    fn region_rejects_wrong_namespace() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "gcp.Region.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.Location.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "foo.bar.baz.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "Location.ap_northeast_1".to_string()
                )))
                .is_err()
        );
    }

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
