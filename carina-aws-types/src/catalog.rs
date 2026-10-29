//! Closed catalog of reusable AWS schema identities.
//!
//! Each row below owns the identity axes and the `AttributeType` constructor
//! that carries that identity. Public constructor functions are generated from
//! the same row, so a wrapper cannot accidentally pair one identity with
//! another type's validation.

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, DslTransform, TypeIdentity, legacy_validator};

use crate::{
    validate_arn, validate_availability_zone, validate_availability_zone_id,
    validate_aws_account_id, validate_aws_region, validate_aws_resource_id, validate_iam_arn,
    validate_iam_role_id, validate_ipam_pool_id, validate_kms_key_id,
    validate_prefixed_resource_id, validate_service_arn,
};

fn validated_string(
    identity: TypeIdentity,
    pattern: &'static str,
    length: Option<(Option<u64>, Option<u64>)>,
    validator: fn(&str) -> Result<(), String>,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern.to_string()),
        length,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validator(value.expose_plaintext())
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

fn metadata_string(
    identity: TypeIdentity,
    pattern: &'static str,
    length: Option<(Option<u64>, Option<u64>)>,
) -> AttributeType {
    AttributeType::refined_string(Some(identity), Some(pattern.to_string()), length, None)
}

fn prefixed_resource_id(identity: TypeIdentity, prefix: &'static str) -> AttributeType {
    let pattern = format!("^{prefix}-[0-9a-f]{{8,}}$");
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern),
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_prefixed_resource_id(value.expose_plaintext(), prefix)
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

fn iam_arn(
    identity: TypeIdentity,
    pattern: &'static str,
    resource_prefix: &'static str,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern.to_string()),
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_iam_arn(value.expose_plaintext(), resource_prefix)
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

fn service_arn(
    identity: TypeIdentity,
    pattern: &'static str,
    service: &'static str,
    resource_prefix: Option<&'static str>,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern.to_string()),
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_service_arn(value.expose_plaintext(), service, resource_prefix)
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

fn validated_open_enum(
    identity: TypeIdentity,
    validator: fn(&str) -> Result<(), String>,
) -> AttributeType {
    AttributeType::enum_(
        identity,
        None,
        vec![],
        Some(legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validator(value.expose_plaintext())
            } else {
                Err("Expected string".to_string())
            }
        })),
        Some(DslTransform::HyphenToUnderscore),
    )
}

fn availability_zone_type(identity: TypeIdentity) -> AttributeType {
    AttributeType::enum_(
        identity,
        None,
        vec![],
        Some(legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                let value = value.expose_plaintext();
                validate_availability_zone(value)
                    .map_err(|reason| format!("Invalid availability zone '{value}': {reason}"))
            } else {
                Err("Expected string".to_string())
            }
        })),
        Some(DslTransform::HyphenToUnderscore),
    )
}

macro_rules! define_aws_types {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident => $wrapper:ident {
                segments: [$($segment:literal),* $(,)?],
                kind: $kind:literal,
                constructor: $constructor:expr $(,)?
            }
        ),* $(,)?
    ) => {
        /// Every reusable AWS identity currently declared by this crate.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum AwsType {
            $($variant),*
        }

        impl AwsType {
            /// Closed enumeration of the catalog rows.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// Structured identity declared by this row.
            pub fn identity(self) -> TypeIdentity {
                match self {
                    $(Self::$variant => TypeIdentity::new(
                        Some("aws"),
                        Vec::<&'static str>::from([$($segment),*]),
                        $kind,
                    )),*
                }
            }

            /// Construct this row's identity-bearing attribute type.
            pub fn attribute_type(self) -> AttributeType {
                match self {
                    $(Self::$variant => ($constructor)(self.identity())),*
                }
            }
        }

        $(
            $(#[$meta])*
            pub fn $wrapper() -> AttributeType {
                AwsType::$variant.attribute_type()
            }
        )*
    };
}

define_aws_types! {
    /// Generic AWS ARN.
    Arn => arn {
        segments: [],
        kind: "Arn",
        constructor: |identity| validated_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):[^:]+:.*$",
            None,
            validate_arn,
        ),
    },
    /// Generic AWS resource ID.
    ResourceId => aws_resource_id {
        segments: [],
        kind: "ResourceId",
        constructor: |identity| validated_string(
            identity,
            "^[a-z-]+-[0-9a-f]{8,}$",
            None,
            validate_aws_resource_id,
        ),
    },
    /// Twelve-digit AWS account ID.
    AccountId => aws_account_id {
        segments: [],
        kind: "AccountId",
        constructor: |identity| validated_string(
            identity,
            "^\\d{12}$",
            Some((Some(12), Some(12))),
            validate_aws_account_id,
        ),
    },
    /// AWS Region in provider-facing API spelling.
    Region => aws_region {
        segments: [],
        kind: "Region",
        constructor: |identity| validated_open_enum(identity, validate_aws_region),
    },
    /// AWS Availability Zone in provider-facing API spelling.
    AvailabilityZone => availability_zone {
        segments: ["AvailabilityZone"],
        kind: "ZoneName",
        constructor: availability_zone_type,
    },
    /// AWS Availability Zone ID.
    AvailabilityZoneId => availability_zone_id {
        segments: ["AvailabilityZone"],
        kind: "ZoneId",
        constructor: |identity| validated_string(
            identity,
            "^[a-z]+[0-9]+-az[0-9]+$",
            None,
            validate_availability_zone_id,
        ),
    },
    /// IAM role ID.
    IamRoleId => iam_role_id {
        segments: ["iam", "Role"],
        kind: "Id",
        constructor: |identity| validated_string(
            identity,
            "^AROA[A-Z0-9]+$",
            None,
            validate_iam_role_id,
        ),
    },
    /// IAM role ARN.
    IamRoleArn => iam_role_arn {
        segments: ["iam", "Role"],
        kind: "Arn",
        constructor: |identity| iam_arn(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$",
            "role/",
        ),
    },
    /// IAM policy ARN.
    IamPolicyArn => iam_policy_arn {
        segments: ["iam", "Policy"],
        kind: "Arn",
        constructor: |identity| iam_arn(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$",
            "policy/",
        ),
    },
    /// IAM OIDC provider ARN.
    IamOidcProviderArn => iam_oidc_provider_arn {
        segments: ["iam", "OidcProvider"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:oidc-provider/.+$",
            None,
        ),
    },
    /// KMS key ID, alias, or ARN.
    KmsKeyId => kms_key_id {
        segments: ["kms", "Key"],
        kind: "Id",
        constructor: |identity| validated_string(
            identity,
            "^((arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:(key|alias)/.+)|(alias/.+)|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}))$",
            None,
            validate_kms_key_id,
        ),
    },
    /// KMS key ARN (aliases are intentionally excluded).
    KmsKeyArn => kms_key_arn {
        segments: ["kms", "Key"],
        kind: "Arn",
        constructor: |identity| service_arn(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$",
            "kms",
            Some("key/"),
        ),
    },
    /// EC2 IPAM pool ID.
    IpamPoolId => ipam_pool_id {
        segments: ["ec2", "IpamPool"],
        kind: "Id",
        constructor: |identity| validated_string(
            identity,
            "^ipam-pool-[0-9a-f]{8,}$",
            None,
            validate_ipam_pool_id,
        ),
    },
    /// Route 53 hosted-zone ID.
    Route53HostedZoneId => route53_hosted_zone_id {
        segments: ["route53", "HostedZone"],
        kind: "Id",
        constructor: |identity| metadata_string(
            identity,
            "^Z[A-Z0-9]+$",
            Some((None, Some(32))),
        ),
    },
    /// Identity Store ID.
    IdentityStoreId => identity_store_id {
        segments: ["identitystore", "Store"],
        kind: "Id",
        constructor: |identity| metadata_string(
            identity,
            "^(d-[0-9a-fA-F]{10}|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$",
            None,
        ),
    },
    /// IAM Identity Center principal ID.
    SsoPrincipalId => sso_principal_id {
        segments: ["sso", "Principal"],
        kind: "Id",
        constructor: |identity| metadata_string(identity, "^.{1,64}$", None),
    },
    /// IAM Identity Center instance ARN.
    SsoInstanceArn => sso_instance_arn {
        segments: ["sso", "Instance"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):sso:::instance/.+$",
            None,
        ),
    },
    /// IAM Identity Center permission-set ARN.
    SsoPermissionSetArn => sso_permission_set_arn {
        segments: ["sso", "PermissionSet"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):sso:::permissionSet/.+$",
            None,
        ),
    },
    /// S3 ACL grantee expression.
    S3Grantee => s3_grantee {
        segments: ["s3"],
        kind: "Grantee",
        constructor: |identity| metadata_string(
            identity,
            r"^(id|emailAddress|uri)=.+(\s*,\s*(id|emailAddress|uri)=.+)*$",
            None,
        ),
    },
    /// CloudFront's fixed Route 53 hosted-zone ID.
    CloudFrontHostedZoneId => cloudfront_hosted_zone_id {
        segments: ["cloudfront"],
        kind: "HostedZoneId",
        constructor: |identity| AttributeType::enum_(
            identity,
            Some(vec!["Z2FDTNDATAQYW2".to_string()]),
            vec![("Z2FDTNDATAQYW2".to_string(), "global".to_string())],
            None,
            None,
        ),
    },
    /// S3 bucket ARN.
    S3BucketArn => s3_bucket_arn {
        segments: ["s3", "Bucket"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):s3:::.+$",
            None,
        ),
    },
    /// STS caller-identity ARN.
    StsCallerIdentityArn => sts_caller_identity_arn {
        segments: ["sts", "CallerIdentity"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):sts:.*$",
            None,
        ),
    },
    /// Organizations account ARN.
    OrganizationsAccountArn => organizations_account_arn {
        segments: ["organizations", "Account"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):organizations:.*$",
            None,
        ),
    },
    /// Organizations organization ARN.
    OrganizationsOrganizationArn => organizations_organization_arn {
        segments: ["organizations", "Organization"],
        kind: "Arn",
        constructor: |identity| metadata_string(
            identity,
            "^arn:(aws|aws-cn|aws-us-gov):organizations:.*$",
            None,
        ),
    },
    /// EC2 VPC ID.
    VpcId => vpc_id {
        segments: ["ec2", "Vpc"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "vpc"),
    },
    /// EC2 subnet ID.
    SubnetId => subnet_id {
        segments: ["ec2", "Subnet"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "subnet"),
    },
    /// EC2 security-group ID.
    SecurityGroupId => security_group_id {
        segments: ["ec2", "SecurityGroup"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "sg"),
    },
    /// EC2 internet-gateway ID.
    InternetGatewayId => internet_gateway_id {
        segments: ["ec2", "InternetGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "igw"),
    },
    /// EC2 route-table ID.
    RouteTableId => route_table_id {
        segments: ["ec2", "RouteTable"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "rtb"),
    },
    /// EC2 NAT-gateway ID.
    NatGatewayId => nat_gateway_id {
        segments: ["ec2", "NatGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "nat"),
    },
    /// EC2 VPC-peering-connection ID.
    VpcPeeringConnectionId => vpc_peering_connection_id {
        segments: ["ec2", "VpcPeeringConnection"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "pcx"),
    },
    /// EC2 transit-gateway ID.
    TransitGatewayId => transit_gateway_id {
        segments: ["ec2", "TransitGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "tgw"),
    },
    /// EC2 VPC CIDR-block-association ID.
    VpcCidrBlockAssociationId => vpc_cidr_block_association_id {
        segments: ["ec2", "VpcCidrBlockAssociation"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "vpc-cidr-assoc"),
    },
    /// EC2 transit-gateway route-table ID.
    TransitGatewayRouteTableId => tgw_route_table_id {
        segments: ["ec2", "TransitGatewayRouteTable"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "tgw-rtb"),
    },
    /// EC2 VPN-gateway ID.
    VpnGatewayId => vpn_gateway_id {
        segments: ["ec2", "VpnGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "vgw"),
    },
    /// EC2 egress-only internet-gateway ID.
    EgressOnlyInternetGatewayId => egress_only_internet_gateway_id {
        segments: ["ec2", "EgressOnlyInternetGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "eigw"),
    },
    /// EC2 VPC-endpoint ID.
    VpcEndpointId => vpc_endpoint_id {
        segments: ["ec2", "VpcEndpoint"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "vpce"),
    },
    /// EC2 instance ID.
    InstanceId => instance_id {
        segments: ["ec2", "Instance"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "i"),
    },
    /// EC2 network-interface ID.
    NetworkInterfaceId => network_interface_id {
        segments: ["ec2", "NetworkInterface"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "eni"),
    },
    /// EC2 Elastic IP allocation ID.
    AllocationId => allocation_id {
        segments: ["ec2", "Eip"],
        kind: "AllocationId",
        constructor: |identity| prefixed_resource_id(identity, "eipalloc"),
    },
    /// EC2 prefix-list ID.
    PrefixListId => prefix_list_id {
        segments: ["ec2", "PrefixList"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "pl"),
    },
    /// EC2 carrier-gateway ID.
    CarrierGatewayId => carrier_gateway_id {
        segments: ["ec2", "CarrierGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "cagw"),
    },
    /// EC2 local-gateway ID.
    LocalGatewayId => local_gateway_id {
        segments: ["ec2", "LocalGateway"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "lgw"),
    },
    /// EC2 network-ACL ID.
    NetworkAclId => network_acl_id {
        segments: ["ec2", "NetworkAcl"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "acl"),
    },
    /// EC2 transit-gateway-attachment ID.
    TransitGatewayAttachmentId => transit_gateway_attachment_id {
        segments: ["ec2", "TransitGatewayAttachment"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "tgw-attach"),
    },
    /// EC2 flow-log ID.
    FlowLogId => flow_log_id {
        segments: ["ec2", "FlowLog"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "fl"),
    },
    /// EC2 IPAM ID.
    IpamId => ipam_id {
        segments: ["ec2", "Ipam"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "ipam"),
    },
    /// EC2 subnet/route-table-association ID.
    SubnetRouteTableAssociationId => subnet_route_table_association_id {
        segments: ["ec2", "SubnetRouteTableAssociation"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "rtbassoc"),
    },
    /// EC2 security-group-rule ID.
    SecurityGroupRuleId => security_group_rule_id {
        segments: ["ec2", "SecurityGroupRule"],
        kind: "Id",
        constructor: |identity| prefixed_resource_id(identity, "sgr"),
    },
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use carina_core::schema::RawShape;

    use super::*;

    fn attribute_type_identity(attribute_type: &AttributeType) -> &TypeIdentity {
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
            } => identity,
            RawShape::Enum { identity, .. } => identity,
            shape => panic!("catalog constructor must carry a scalar identity, got {shape:?}"),
        }
    }

    #[test]
    fn all_identities_are_unique_and_match_their_constructors() {
        let mut identities = HashSet::new();

        for aws_type in AwsType::ALL.iter().copied() {
            let identity = aws_type.identity();
            assert!(
                identities.insert(identity.clone()),
                "duplicate AwsType identity {identity}"
            );

            let attribute_type = aws_type.attribute_type();
            assert_eq!(attribute_type_identity(&attribute_type), &identity);
        }

        assert_eq!(identities.len(), AwsType::ALL.len());
    }
}
