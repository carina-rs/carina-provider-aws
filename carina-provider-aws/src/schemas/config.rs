//! AWS schema configuration and validator registry
//!
//! This module re-exports shared AWS type validators from `carina-aws-types`
//! and defines provider-side schema configuration.

pub use carina_aws_types::*;

use std::collections::HashMap;
use std::sync::OnceLock;

use carina_core::schema::{AttributeType, RawShape, ResourceSchema, TypeIdentity};

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

fn attribute_type_identity(attribute_type: &AttributeType) -> TypeIdentity {
    match attribute_type.raw_shape() {
        RawShape::String {
            identity: Some(identity),
            ..
        }
        | RawShape::Int {
            identity: Some(identity),
            ..
        }
        | RawShape::Float {
            identity: Some(identity),
            ..
        } => identity.clone(),
        RawShape::Enum { identity, .. } => identity.clone(),
        _ => panic!("custom validator registration requires a type identity"),
    }
}

fn register_validator(
    validators: &mut HashMap<TypeIdentity, CustomValidatorFn>,
    identity: TypeIdentity,
    validator: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static,
) {
    assert!(
        validators
            .insert(identity.clone(), Box::new(validator))
            .is_none(),
        "duplicate AWS custom validator for {identity}"
    );
}

fn register_type_validator(
    validators: &mut HashMap<TypeIdentity, CustomValidatorFn>,
    attribute_type: AttributeType,
    validator: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static,
) {
    register_validator(
        validators,
        attribute_type_identity(&attribute_type),
        validator,
    );
}

/// Return all AWS type validators for use in `validate_custom_type`.
///
/// Keys come directly from the same `AttributeType` constructors used by the
/// schemas. Keeping the full structured identity prevents resource-scoped
/// types such as `aws.ec2.Vpc.Id` and `aws.iam.Role.Id` from collapsing to the
/// same `Id` lookup key.
pub fn aws_validators() -> HashMap<TypeIdentity, CustomValidatorFn> {
    let mut validators = HashMap::new();

    register_type_validator(&mut validators, arn(), validate_arn);
    register_type_validator(&mut validators, aws_resource_id(), validate_aws_resource_id);
    register_type_validator(&mut validators, iam_role_id(), validate_iam_role_id);
    register_type_validator(&mut validators, aws_account_id(), validate_aws_account_id);
    register_type_validator(&mut validators, kms_key_id(), validate_kms_key_id);
    register_type_validator(&mut validators, ipam_pool_id(), validate_ipam_pool_id);
    register_type_validator(
        &mut validators,
        availability_zone_id(),
        validate_availability_zone_id,
    );
    register_type_validator(
        &mut validators,
        availability_zone(),
        validate_availability_zone,
    );
    register_type_validator(&mut validators, aws_region(), validate_aws_region);

    for (attribute_type, prefix) in [
        (vpc_id(), "vpc"),
        (subnet_id(), "subnet"),
        (security_group_id(), "sg"),
        (internet_gateway_id(), "igw"),
        (route_table_id(), "rtb"),
        (nat_gateway_id(), "nat"),
        (transit_gateway_id(), "tgw"),
        (vpn_gateway_id(), "vgw"),
        (network_interface_id(), "eni"),
        (allocation_id(), "eipalloc"),
        (vpc_endpoint_id(), "vpce"),
        (vpc_peering_connection_id(), "pcx"),
        (instance_id(), "i"),
        (prefix_list_id(), "pl"),
        (carrier_gateway_id(), "cagw"),
        (local_gateway_id(), "lgw"),
        (network_acl_id(), "acl"),
        (transit_gateway_attachment_id(), "tgw-attach"),
        (flow_log_id(), "fl"),
        (ipam_id(), "ipam"),
        (subnet_route_table_association_id(), "rtbassoc"),
        (security_group_rule_id(), "sgr"),
        (vpc_cidr_block_association_id(), "vpc-cidr-assoc"),
        (tgw_route_table_id(), "tgw-rtb"),
        (egress_only_internet_gateway_id(), "eigw"),
    ] {
        register_type_validator(&mut validators, attribute_type, move |value| {
            validate_prefixed_resource_id(value, prefix)
        });
    }

    register_type_validator(&mut validators, iam_role_arn(), |value| {
        validate_iam_arn(value, "role/")
    });
    register_type_validator(&mut validators, iam_policy_arn(), |value| {
        validate_iam_arn(value, "policy/")
    });
    register_validator(
        &mut validators,
        provider_type("kms", "Key", "Arn"),
        |value| validate_service_arn(value, "kms", Some("key/")),
    );

    validators
}

/// Validate provider-facing API spelling for one AWS custom type.
///
/// Both the in-process factory and the component guest call this function, so
/// lookup behavior cannot drift between the two provider entry points.
pub fn validate_aws_custom_type(identity: &TypeIdentity, value: &str) -> Result<(), String> {
    static VALIDATORS: OnceLock<HashMap<TypeIdentity, CustomValidatorFn>> = OnceLock::new();
    let validators = VALIDATORS.get_or_init(aws_validators);
    validators
        .get(identity)
        .map_or(Ok(()), |validator| validator(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    // aws_validators tests

    #[test]
    fn aws_validators_all_registered() {
        let validators = aws_validators();

        let simple_types = [
            arn(),
            aws_resource_id(),
            iam_role_id(),
            aws_account_id(),
            kms_key_id(),
            ipam_pool_id(),
            availability_zone_id(),
            availability_zone(),
            aws_region(),
        ];
        let prefixed_types = [
            vpc_id(),
            subnet_id(),
            security_group_id(),
            internet_gateway_id(),
            route_table_id(),
            nat_gateway_id(),
            transit_gateway_id(),
            vpn_gateway_id(),
            network_interface_id(),
            allocation_id(),
            vpc_endpoint_id(),
            vpc_peering_connection_id(),
            instance_id(),
            prefix_list_id(),
            carrier_gateway_id(),
            local_gateway_id(),
            network_acl_id(),
            transit_gateway_attachment_id(),
            flow_log_id(),
            ipam_id(),
            subnet_route_table_association_id(),
            security_group_rule_id(),
            vpc_cidr_block_association_id(),
            tgw_route_table_id(),
            egress_only_internet_gateway_id(),
        ];
        let service_arn_identities = [
            attribute_type_identity(&iam_role_arn()),
            attribute_type_identity(&iam_policy_arn()),
            provider_type("kms", "Key", "Arn"),
        ];
        let all_expected: Vec<TypeIdentity> = simple_types
            .iter()
            .chain(&prefixed_types)
            .map(attribute_type_identity)
            .chain(service_arn_identities)
            .collect();

        for identity in &all_expected {
            assert!(
                validators.contains_key(identity),
                "missing validator for schema identity {identity}"
            );
        }

        assert_eq!(
            validators.len(),
            all_expected.len(),
            "validator count mismatch: expected {}, got {}; extra identities: {:?}",
            all_expected.len(),
            validators.len(),
            validators
                .keys()
                .filter(|identity| !all_expected.contains(identity))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aws_validators_produce_correct_results() {
        let validators = aws_validators();

        // Test a prefixed resource ID validator
        let vpc_identity = attribute_type_identity(&vpc_id());
        let vpc_validator = validators.get(&vpc_identity).unwrap();
        assert!(vpc_validator("vpc-12345678").is_ok());
        assert!(vpc_validator("subnet-12345678").is_err());

        // Test a single-arg validator
        let arn_identity = attribute_type_identity(&arn());
        let arn_validator = validators.get(&arn_identity).unwrap();
        assert!(arn_validator("arn:aws:s3:::my-bucket").is_ok());
        assert!(arn_validator("not-an-arn").is_err());

        // Test a service ARN validator
        let iam_role_arn_identity = attribute_type_identity(&iam_role_arn());
        let iam_role_arn_validator = validators.get(&iam_role_arn_identity).unwrap();
        assert!(iam_role_arn_validator("arn:aws:iam::123456789012:role/my-role").is_ok());
        assert!(iam_role_arn_validator("arn:aws:s3:::my-bucket").is_err());

        // Test unknown type returns no validator (not an error)
        assert!(!validators.contains_key(&provider_bare_type(&[], "UnknownType")));
    }

    // The host resolves enum DSL spelling before custom type validation, so
    // the provider validates only the AWS API spelling it receives.
    #[test]
    fn aws_validators_availability_zone_accepts_api_spelling_only() {
        let validators = aws_validators();
        let az_identity = attribute_type_identity(&availability_zone());
        let az = validators.get(&az_identity).unwrap();

        assert!(az("us-east-1a").is_ok());
        assert!(az("ap-northeast-1c").is_ok());

        assert!(az("aws.AvailabilityZone.ZoneName.ap_northeast_1a").is_err());
        assert!(az("ZoneName.ap_northeast_1a").is_err());
        assert!(az("ap_northeast_1a").is_err());
        assert!(az("us-east-1").is_err()); // region, not AZ
        assert!(az("not-an-az").is_err());
    }
}
