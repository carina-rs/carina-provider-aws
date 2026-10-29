//! Closed catalog of reusable AWS schema identities.
//!
//! Each row below owns the identity axes and the `AttributeType` constructor
//! that carries that identity. Public constructor functions are generated from
//! the same row, so a wrapper cannot accidentally pair one identity with
//! another type's validation.

use aws_smithy_types::{DateTime, date_time::Format};
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

fn s3_lifecycle_date_type(identity: TypeIdentity) -> AttributeType {
    validated_string(
        identity,
        "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$",
        Some((Some(20), Some(20))),
        |value| {
            let parsed = DateTime::from_str(value, Format::DateTime)
                .map_err(|error| format!("invalid UTC RFC 3339 timestamp: {error}"))?;
            let canonical = parsed
                .fmt(Format::DateTime)
                .map_err(|error| format!("cannot format UTC RFC 3339 timestamp: {error}"))?;
            if canonical == value {
                Ok(())
            } else {
                Err(format!(
                    "timestamp must use canonical UTC RFC 3339 form {canonical}"
                ))
            }
        },
    )
}

fn aws_identity(segments: &[&str], kind: &str) -> TypeIdentity {
    if let [service, resource] = segments {
        crate::common::provider_type(service, resource, kind)
    } else {
        crate::common::provider_bare_type(segments, kind)
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_identity {
    (
        $identity_constructor:expr;
        segments: [$($segment:literal),* $(,)?],
        kind: $kind:literal,
    ) => {{
        let segments: &[&str] = &[$($segment),*];
        ($identity_constructor)(segments, $kind)
    }};
    (
        $identity_constructor:expr;
        refines: $refined_type:path,
    ) => {
        ($refined_type).identity()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_valid_sample {
    (valid_sample: $valid_sample:literal,) => {
        $valid_sample
    };
    (refines: $refined_type:path,) => {
        ($refined_type).valid_sample()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_refines {
    () => {
        None
    };
    (refines: $refined_type:path,) => {
        Some($refined_type)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_allowed_int_values {
    () => {
        None
    };
    (values: [$($value:literal),+ $(,)?],) => {
        Some(&[$($value),+])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_has_validator {
    () => {
        false
    };
    (has_validator: $has_validator:literal,) => {
        $has_validator
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_attribute_type {
    (
        $constructor:expr;
        identity: $identity:expr,
    ) => {
        ($constructor)($identity)
    };
    (
        $constructor:expr;
        identity: $identity:expr,
        values: [$($value:literal),+ $(,)?],
    ) => {
        ($constructor)($identity, &[$($value),+])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carina_aws_catalog_constructor_path {
    (
        visibility: $visibility:vis,
        catalog: $catalog:ident,
        rows: {$($variant:ident => $wrapper:ident),* $(,)?},
    ) => {};
    (
        constructor_path_prefix: $constructor_path_prefix:literal,
        visibility: $visibility:vis,
        catalog: $catalog:ident,
        rows: {$($variant:ident => $wrapper:ident),* $(,)?},
    ) => {
        impl $catalog {
            /// Rust constructor expression emitted by schema codegen.
            $visibility const fn constructor_path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!(
                        $constructor_path_prefix,
                        stringify!($wrapper),
                        "()",
                    )),*
                }
            }
        }
    };
}

/// Defines a closed catalog of identity-bearing AWS attribute types.
///
/// Each row uses the same grammar. An own-identity row supplies `segments`,
/// `kind`, and `valid_sample`; a refinement supplies `refines`; and an integer
/// enum additionally supplies `values`. `has_validator` is optional and
/// defaults to `false`. `constructor_path_prefix` is optional at catalog level;
/// when present, the macro generates `constructor_path()` by appending the row's
/// wrapper name and `()` to that literal prefix.
///
/// The fields are intentionally captured once in this matcher. To add a future
/// optional per-row field such as `reference_form`, add one optional capture
/// beside the existing fields and generate its accessor here. Only rows that set
/// the field then need to change; downstream catalogs keep using this macro.
#[macro_export]
macro_rules! define_aws_type_catalog {
    (
        $(#[$catalog_meta:meta])*
        $visibility:vis enum $catalog:ident {
            identity_constructor: $identity_constructor:expr,
            refinement_type: $refinement_type:ty,
            $(constructor_path_prefix: $constructor_path_prefix:literal,)?
            rows: {
                $(
                    $(#[$row_meta:meta])*
                    $variant:ident => $wrapper:ident {
                        $(segments: [$($segment:literal),* $(,)?],)?
                        $(kind: $kind:literal,)?
                        $(refines: $refined_type:path,)?
                        $(valid_sample: $valid_sample:literal,)?
                        $(values: [$($int_value:literal),+ $(,)?],)?
                        $(has_validator: $has_validator:literal,)?
                        constructor: $constructor:expr $(,)?
                    }
                ),* $(,)?
            }
        }
    ) => {
        $(#[$catalog_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $visibility enum $catalog {
            $($variant),*
        }

        impl $catalog {
            /// Closed enumeration of the catalog rows.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// Structured identity declared by this row.
            pub fn identity(self) -> $crate::__private::TypeIdentity {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_identity!(
                        $identity_constructor;
                        $(segments: [$($segment),*],)?
                        $(kind: $kind,)?
                        $(refines: $refined_type,)?
                    )),*
                }
            }

            /// Catalog row whose identity and valid sample this row refines.
            pub const fn refines(self) -> Option<$refinement_type> {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_refines!(
                        $(refines: $refined_type,)?
                    )),*
                }
            }

            /// Known-valid provider-facing value for this row.
            pub const fn valid_sample(self) -> &'static str {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_valid_sample!(
                        $(refines: $refined_type,)?
                        $(valid_sample: $valid_sample,)?
                    )),*
                }
            }

            /// Allowed values for an integer-enum row.
            pub const fn allowed_int_values(self) -> Option<&'static [i64]> {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_allowed_int_values!(
                        $(values: [$($int_value),+],)?
                    )),*
                }
            }

            /// Whether this row's constructor carries a custom validator.
            pub const fn has_validator(self) -> bool {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_has_validator!(
                        $(has_validator: $has_validator,)?
                    )),*
                }
            }

            /// Construct this row's identity-bearing attribute type.
            pub fn attribute_type(self) -> $crate::__private::AttributeType {
                match self {
                    $(Self::$variant => $crate::__carina_aws_catalog_attribute_type!(
                        $constructor;
                        identity: self.identity(),
                        $(values: [$($int_value),+],)?
                    )),*
                }
            }
        }

        $crate::__carina_aws_catalog_constructor_path! {
            $(constructor_path_prefix: $constructor_path_prefix,)?
            visibility: $visibility,
            catalog: $catalog,
            rows: {$($variant => $wrapper),*},
        }

        $(
            $(#[$row_meta])*
            $visibility fn $wrapper() -> $crate::__private::AttributeType {
                $catalog::$variant.attribute_type()
            }
        )*
    };
}

define_aws_type_catalog! {
    /// Every reusable AWS identity currently declared by this crate.
    pub enum AwsType {
        identity_constructor: aws_identity,
        refinement_type: AwsType,
        rows: {
    /// Generic AWS ARN.
    Arn => arn {
        segments: [],
        kind: "Arn",
        valid_sample: "arn:aws:s3:::example-bucket",
        has_validator: true,
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
        valid_sample: "resource-12345678",
        has_validator: true,
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
        valid_sample: "123456789012",
        has_validator: true,
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
        valid_sample: "ap-northeast-1",
        has_validator: true,
        constructor: |identity| validated_open_enum(identity, validate_aws_region),
    },
    /// AWS Availability Zone in provider-facing API spelling.
    AvailabilityZone => availability_zone {
        segments: ["AvailabilityZone"],
        kind: "ZoneName",
        valid_sample: "ap-northeast-1a",
        has_validator: true,
        constructor: availability_zone_type,
    },
    /// AWS Availability Zone ID.
    AvailabilityZoneId => availability_zone_id {
        segments: ["AvailabilityZone"],
        kind: "ZoneId",
        valid_sample: "use1-az1",
        has_validator: true,
        constructor: |identity| validated_string(
            identity,
            "^[a-z]+[0-9]+-az[0-9]+$",
            None,
            validate_availability_zone_id,
        ),
    },
    /// Canonical UTC timestamp used by S3 lifecycle expiration and transition dates.
    S3LifecycleDate => s3_lifecycle_date {
        segments: ["s3", "BucketLifecycleConfiguration"],
        kind: "LifecycleDate",
        valid_sample: "2026-01-01T00:00:00Z",
        has_validator: true,
        constructor: s3_lifecycle_date_type,
    },
    /// IAM role ID.
    IamRoleId => iam_role_id {
        segments: ["iam", "Role"],
        kind: "Id",
        valid_sample: "AROA1234567890EXAMPLE",
        has_validator: true,
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
        valid_sample: "arn:aws:iam::123456789012:role/example-role",
        has_validator: true,
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
        valid_sample: "arn:aws:iam::123456789012:policy/example-policy",
        has_validator: true,
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
        valid_sample: "arn:aws:iam::123456789012:oidc-provider/example.com",
        has_validator: false,
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
        valid_sample: "12345678-1234-1234-1234-123456789abc",
        has_validator: true,
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
        valid_sample: "arn:aws:kms:ap-northeast-1:123456789012:key/12345678-1234-1234-1234-123456789abc",
        has_validator: true,
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
        valid_sample: "ipam-pool-12345678",
        has_validator: true,
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
        valid_sample: "Z2FDTNDATAQYW2",
        has_validator: false,
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
        valid_sample: "d-1234567890",
        has_validator: false,
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
        valid_sample: "12345678-1234-1234-1234-123456789abc",
        has_validator: false,
        constructor: |identity| metadata_string(identity, "^.{1,64}$", None),
    },
    /// IAM Identity Center instance ARN.
    SsoInstanceArn => sso_instance_arn {
        segments: ["sso", "Instance"],
        kind: "Arn",
        valid_sample: "arn:aws:sso:::instance/ssoins-1234567890abcdef",
        has_validator: false,
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
        valid_sample: "arn:aws:sso:::permissionSet/ssoins-1234567890abcdef/ps-1234567890abcdef",
        has_validator: false,
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
        valid_sample: "id=canonical-user-id",
        has_validator: false,
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
        valid_sample: "Z2FDTNDATAQYW2",
        has_validator: false,
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
        valid_sample: "arn:aws:s3:::example-bucket",
        has_validator: false,
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
        valid_sample: "arn:aws:sts::123456789012:assumed-role/example-role/example-session",
        has_validator: false,
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
        valid_sample: "arn:aws:organizations::123456789012:account/o-example/123456789012",
        has_validator: false,
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
        valid_sample: "arn:aws:organizations::123456789012:organization/o-example",
        has_validator: false,
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
        valid_sample: "vpc-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "vpc"),
    },
    /// EC2 subnet ID.
    SubnetId => subnet_id {
        segments: ["ec2", "Subnet"],
        kind: "Id",
        valid_sample: "subnet-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "subnet"),
    },
    /// EC2 security-group ID.
    SecurityGroupId => security_group_id {
        segments: ["ec2", "SecurityGroup"],
        kind: "Id",
        valid_sample: "sg-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "sg"),
    },
    /// EC2 internet-gateway ID.
    InternetGatewayId => internet_gateway_id {
        segments: ["ec2", "InternetGateway"],
        kind: "Id",
        valid_sample: "igw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "igw"),
    },
    /// EC2 route-table ID.
    RouteTableId => route_table_id {
        segments: ["ec2", "RouteTable"],
        kind: "Id",
        valid_sample: "rtb-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "rtb"),
    },
    /// EC2 NAT-gateway ID.
    NatGatewayId => nat_gateway_id {
        segments: ["ec2", "NatGateway"],
        kind: "Id",
        valid_sample: "nat-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "nat"),
    },
    /// EC2 VPC-peering-connection ID.
    VpcPeeringConnectionId => vpc_peering_connection_id {
        segments: ["ec2", "VpcPeeringConnection"],
        kind: "Id",
        valid_sample: "pcx-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "pcx"),
    },
    /// EC2 transit-gateway ID.
    TransitGatewayId => transit_gateway_id {
        segments: ["ec2", "TransitGateway"],
        kind: "Id",
        valid_sample: "tgw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "tgw"),
    },
    /// EC2 VPC CIDR-block-association ID.
    VpcCidrBlockAssociationId => vpc_cidr_block_association_id {
        segments: ["ec2", "VpcCidrBlockAssociation"],
        kind: "Id",
        valid_sample: "vpc-cidr-assoc-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "vpc-cidr-assoc"),
    },
    /// EC2 transit-gateway route-table ID.
    TransitGatewayRouteTableId => tgw_route_table_id {
        segments: ["ec2", "TransitGatewayRouteTable"],
        kind: "Id",
        valid_sample: "tgw-rtb-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "tgw-rtb"),
    },
    /// EC2 VPN-gateway ID.
    VpnGatewayId => vpn_gateway_id {
        segments: ["ec2", "VpnGateway"],
        kind: "Id",
        valid_sample: "vgw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "vgw"),
    },
    /// EC2 egress-only internet-gateway ID.
    EgressOnlyInternetGatewayId => egress_only_internet_gateway_id {
        segments: ["ec2", "EgressOnlyInternetGateway"],
        kind: "Id",
        valid_sample: "eigw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "eigw"),
    },
    /// EC2 VPC-endpoint ID.
    VpcEndpointId => vpc_endpoint_id {
        segments: ["ec2", "VpcEndpoint"],
        kind: "Id",
        valid_sample: "vpce-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "vpce"),
    },
    /// EC2 instance ID.
    InstanceId => instance_id {
        segments: ["ec2", "Instance"],
        kind: "Id",
        valid_sample: "i-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "i"),
    },
    /// EC2 network-interface ID.
    NetworkInterfaceId => network_interface_id {
        segments: ["ec2", "NetworkInterface"],
        kind: "Id",
        valid_sample: "eni-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "eni"),
    },
    /// EC2 Elastic IP allocation ID.
    AllocationId => allocation_id {
        segments: ["ec2", "Eip"],
        kind: "AllocationId",
        valid_sample: "eipalloc-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "eipalloc"),
    },
    /// EC2 prefix-list ID.
    PrefixListId => prefix_list_id {
        segments: ["ec2", "PrefixList"],
        kind: "Id",
        valid_sample: "pl-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "pl"),
    },
    /// EC2 carrier-gateway ID.
    CarrierGatewayId => carrier_gateway_id {
        segments: ["ec2", "CarrierGateway"],
        kind: "Id",
        valid_sample: "cagw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "cagw"),
    },
    /// EC2 local-gateway ID.
    LocalGatewayId => local_gateway_id {
        segments: ["ec2", "LocalGateway"],
        kind: "Id",
        valid_sample: "lgw-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "lgw"),
    },
    /// EC2 network-ACL ID.
    NetworkAclId => network_acl_id {
        segments: ["ec2", "NetworkAcl"],
        kind: "Id",
        valid_sample: "acl-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "acl"),
    },
    /// EC2 transit-gateway-attachment ID.
    TransitGatewayAttachmentId => transit_gateway_attachment_id {
        segments: ["ec2", "TransitGatewayAttachment"],
        kind: "Id",
        valid_sample: "tgw-attach-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "tgw-attach"),
    },
    /// EC2 flow-log ID.
    FlowLogId => flow_log_id {
        segments: ["ec2", "FlowLog"],
        kind: "Id",
        valid_sample: "fl-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "fl"),
    },
    /// EC2 IPAM ID.
    IpamId => ipam_id {
        segments: ["ec2", "Ipam"],
        kind: "Id",
        valid_sample: "ipam-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "ipam"),
    },
    /// EC2 subnet/route-table-association ID.
    SubnetRouteTableAssociationId => subnet_route_table_association_id {
        segments: ["ec2", "SubnetRouteTableAssociation"],
        kind: "Id",
        valid_sample: "rtbassoc-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "rtbassoc"),
    },
    /// EC2 security-group-rule ID.
    SecurityGroupRuleId => security_group_rule_id {
        segments: ["ec2", "SecurityGroupRule"],
        kind: "Id",
        valid_sample: "sgr-12345678",
        has_validator: true,
        constructor: |identity| prefixed_resource_id(identity, "sgr"),
    },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use carina_core::schema::{CustomValidator, RawShape};

    use super::*;

    fn test_catalog_identity(segments: &[&str], kind: &str) -> TypeIdentity {
        TypeIdentity {
            provider: Some("test".to_string()),
            segments: segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect(),
            kind: kind.to_string(),
        }
    }

    fn test_string_constructor(identity: TypeIdentity) -> AttributeType {
        metadata_string(identity, ".*", None)
    }

    fn test_int_constructor(
        identity: TypeIdentity,
        _allowed_values: &'static [i64],
    ) -> AttributeType {
        metadata_string(identity, "^[0-9]+$", None)
    }

    crate::define_aws_type_catalog! {
        enum TestCatalog {
            identity_constructor: test_catalog_identity,
            refinement_type: TestCatalog,
            constructor_path_prefix: "test::catalog::",
            rows: {
                Own => test_own {
                    segments: ["service", "Resource"],
                    kind: "Name",
                    valid_sample: "example",
                    constructor: test_string_constructor,
                },
                ValidatedOwn => test_validated_own {
                    segments: ["service", "ValidatedResource"],
                    kind: "Name",
                    valid_sample: "validated-example",
                    has_validator: true,
                    constructor: test_string_constructor,
                },
                Refinement => test_refinement {
                    refines: TestCatalog::Own,
                    constructor: test_string_constructor,
                },
                IntEnum => test_int_enum {
                    segments: ["service", "Resource"],
                    kind: "Count",
                    valid_sample: "7",
                    values: [7, 9],
                    constructor: test_int_constructor,
                },
            }
        }
    }

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

    fn attribute_type_validator(attribute_type: &AttributeType) -> Option<&CustomValidator> {
        match attribute_type.raw_shape() {
            RawShape::String { validate, .. }
            | RawShape::Int { validate, .. }
            | RawShape::Float { validate, .. }
            | RawShape::Enum { validate, .. } => validate,
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

    #[test]
    fn validator_declarations_match_constructor_shapes_and_the_registry_contract() {
        let expected = HashSet::from([
            AwsType::Arn,
            AwsType::ResourceId,
            AwsType::AccountId,
            AwsType::Region,
            AwsType::AvailabilityZone,
            AwsType::AvailabilityZoneId,
            AwsType::S3LifecycleDate,
            AwsType::IamRoleId,
            AwsType::IamRoleArn,
            AwsType::IamPolicyArn,
            AwsType::KmsKeyId,
            AwsType::KmsKeyArn,
            AwsType::IpamPoolId,
            AwsType::VpcId,
            AwsType::SubnetId,
            AwsType::SecurityGroupId,
            AwsType::InternetGatewayId,
            AwsType::RouteTableId,
            AwsType::NatGatewayId,
            AwsType::VpcPeeringConnectionId,
            AwsType::TransitGatewayId,
            AwsType::VpcCidrBlockAssociationId,
            AwsType::TransitGatewayRouteTableId,
            AwsType::VpnGatewayId,
            AwsType::EgressOnlyInternetGatewayId,
            AwsType::VpcEndpointId,
            AwsType::InstanceId,
            AwsType::NetworkInterfaceId,
            AwsType::AllocationId,
            AwsType::PrefixListId,
            AwsType::CarrierGatewayId,
            AwsType::LocalGatewayId,
            AwsType::NetworkAclId,
            AwsType::TransitGatewayAttachmentId,
            AwsType::FlowLogId,
            AwsType::IpamId,
            AwsType::SubnetRouteTableAssociationId,
            AwsType::SecurityGroupRuleId,
        ]);
        assert_eq!(
            expected.len(),
            38,
            "the pre-catalog validator contract changed"
        );

        let declared = AwsType::ALL
            .iter()
            .copied()
            .filter(|aws_type| aws_type.has_validator())
            .collect::<HashSet<_>>();
        assert_eq!(declared, expected);

        for aws_type in AwsType::ALL.iter().copied() {
            let attribute_type = aws_type.attribute_type();
            let validator = attribute_type_validator(&attribute_type);
            assert_eq!(
                validator.is_some(),
                aws_type.has_validator(),
                "validator declaration disagrees with constructor for {aws_type:?}"
            );
            if let Some(validator) = validator {
                let sample = Value::Concrete(ConcreteValue::String(
                    aws_type.valid_sample().to_string().into(),
                ));
                assert!(
                    validator(&sample).is_ok(),
                    "declared sample {:?} must pass the validator for {aws_type:?}",
                    aws_type.valid_sample()
                );
            }
        }
    }

    #[test]
    fn s3_lifecycle_date_accepts_only_canonical_utc_rfc3339() {
        let attribute_type = AwsType::S3LifecycleDate.attribute_type();
        let validator = attribute_type_validator(&attribute_type)
            .expect("S3 lifecycle dates have a semantic validator");
        let value = |value: &str| Value::Concrete(ConcreteValue::String(value.to_string().into()));

        assert!(validator(&value("2026-01-01T00:00:00Z")).is_ok());
        for noncanonical in [
            "2026-01-01T00:00:00.000Z",
            "2026-01-01t00:00:00Z",
            "2026-01-01",
        ] {
            assert!(
                validator(&value(noncanonical)).is_err(),
                "{noncanonical} must be rejected before an S3 request is built"
            );
        }
    }

    #[test]
    fn catalog_metadata_snapshot() {
        const EXPECTED: &str = r#"Arn|aws||Arn|arn:aws:s3:::example-bucket|true
ResourceId|aws||ResourceId|resource-12345678|true
AccountId|aws||AccountId|123456789012|true
Region|aws||Region|ap-northeast-1|true
AvailabilityZone|aws|AvailabilityZone|ZoneName|ap-northeast-1a|true
AvailabilityZoneId|aws|AvailabilityZone|ZoneId|use1-az1|true
S3LifecycleDate|aws|s3/BucketLifecycleConfiguration|LifecycleDate|2026-01-01T00:00:00Z|true
IamRoleId|aws|iam/Role|Id|AROA1234567890EXAMPLE|true
IamRoleArn|aws|iam/Role|Arn|arn:aws:iam::123456789012:role/example-role|true
IamPolicyArn|aws|iam/Policy|Arn|arn:aws:iam::123456789012:policy/example-policy|true
IamOidcProviderArn|aws|iam/OidcProvider|Arn|arn:aws:iam::123456789012:oidc-provider/example.com|false
KmsKeyId|aws|kms/Key|Id|12345678-1234-1234-1234-123456789abc|true
KmsKeyArn|aws|kms/Key|Arn|arn:aws:kms:ap-northeast-1:123456789012:key/12345678-1234-1234-1234-123456789abc|true
IpamPoolId|aws|ec2/IpamPool|Id|ipam-pool-12345678|true
Route53HostedZoneId|aws|route53/HostedZone|Id|Z2FDTNDATAQYW2|false
IdentityStoreId|aws|identitystore/Store|Id|d-1234567890|false
SsoPrincipalId|aws|sso/Principal|Id|12345678-1234-1234-1234-123456789abc|false
SsoInstanceArn|aws|sso/Instance|Arn|arn:aws:sso:::instance/ssoins-1234567890abcdef|false
SsoPermissionSetArn|aws|sso/PermissionSet|Arn|arn:aws:sso:::permissionSet/ssoins-1234567890abcdef/ps-1234567890abcdef|false
S3Grantee|aws|s3|Grantee|id=canonical-user-id|false
CloudFrontHostedZoneId|aws|cloudfront|HostedZoneId|Z2FDTNDATAQYW2|false
S3BucketArn|aws|s3/Bucket|Arn|arn:aws:s3:::example-bucket|false
StsCallerIdentityArn|aws|sts/CallerIdentity|Arn|arn:aws:sts::123456789012:assumed-role/example-role/example-session|false
OrganizationsAccountArn|aws|organizations/Account|Arn|arn:aws:organizations::123456789012:account/o-example/123456789012|false
OrganizationsOrganizationArn|aws|organizations/Organization|Arn|arn:aws:organizations::123456789012:organization/o-example|false
VpcId|aws|ec2/Vpc|Id|vpc-12345678|true
SubnetId|aws|ec2/Subnet|Id|subnet-12345678|true
SecurityGroupId|aws|ec2/SecurityGroup|Id|sg-12345678|true
InternetGatewayId|aws|ec2/InternetGateway|Id|igw-12345678|true
RouteTableId|aws|ec2/RouteTable|Id|rtb-12345678|true
NatGatewayId|aws|ec2/NatGateway|Id|nat-12345678|true
VpcPeeringConnectionId|aws|ec2/VpcPeeringConnection|Id|pcx-12345678|true
TransitGatewayId|aws|ec2/TransitGateway|Id|tgw-12345678|true
VpcCidrBlockAssociationId|aws|ec2/VpcCidrBlockAssociation|Id|vpc-cidr-assoc-12345678|true
TransitGatewayRouteTableId|aws|ec2/TransitGatewayRouteTable|Id|tgw-rtb-12345678|true
VpnGatewayId|aws|ec2/VpnGateway|Id|vgw-12345678|true
EgressOnlyInternetGatewayId|aws|ec2/EgressOnlyInternetGateway|Id|eigw-12345678|true
VpcEndpointId|aws|ec2/VpcEndpoint|Id|vpce-12345678|true
InstanceId|aws|ec2/Instance|Id|i-12345678|true
NetworkInterfaceId|aws|ec2/NetworkInterface|Id|eni-12345678|true
AllocationId|aws|ec2/Eip|AllocationId|eipalloc-12345678|true
PrefixListId|aws|ec2/PrefixList|Id|pl-12345678|true
CarrierGatewayId|aws|ec2/CarrierGateway|Id|cagw-12345678|true
LocalGatewayId|aws|ec2/LocalGateway|Id|lgw-12345678|true
NetworkAclId|aws|ec2/NetworkAcl|Id|acl-12345678|true
TransitGatewayAttachmentId|aws|ec2/TransitGatewayAttachment|Id|tgw-attach-12345678|true
FlowLogId|aws|ec2/FlowLog|Id|fl-12345678|true
IpamId|aws|ec2/Ipam|Id|ipam-12345678|true
SubnetRouteTableAssociationId|aws|ec2/SubnetRouteTableAssociation|Id|rtbassoc-12345678|true
SecurityGroupRuleId|aws|ec2/SecurityGroupRule|Id|sgr-12345678|true"#;

        let snapshot = AwsType::ALL
            .iter()
            .copied()
            .map(|aws_type| {
                let identity = aws_type.identity();
                format!(
                    "{aws_type:?}|{}|{}|{}|{}|{}",
                    identity.provider.as_deref().unwrap_or("<none>"),
                    identity.segments.join("/"),
                    identity.kind,
                    aws_type.valid_sample(),
                    aws_type.has_validator(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(snapshot, EXPECTED);
    }

    #[test]
    fn exported_macro_supports_each_optional_row_field_present_and_absent() {
        assert_eq!(
            TestCatalog::ALL,
            &[
                TestCatalog::Own,
                TestCatalog::ValidatedOwn,
                TestCatalog::Refinement,
                TestCatalog::IntEnum,
            ]
        );

        assert_eq!(TestCatalog::Own.refines(), None);
        assert_eq!(TestCatalog::Refinement.refines(), Some(TestCatalog::Own));
        assert_eq!(TestCatalog::Refinement.valid_sample(), "example");
        assert_eq!(TestCatalog::Own.allowed_int_values(), None);
        assert_eq!(TestCatalog::IntEnum.allowed_int_values(), Some(&[7, 9][..]));
        assert!(!TestCatalog::Own.has_validator());
        assert!(TestCatalog::ValidatedOwn.has_validator());
        assert_eq!(
            TestCatalog::Refinement.constructor_path(),
            "test::catalog::test_refinement()"
        );

        let own_identity = TestCatalog::Own.identity();
        assert_eq!(own_identity.provider.as_deref(), Some("test"));
        assert_eq!(own_identity.segments, ["service", "Resource"]);
        assert_eq!(own_identity.kind, "Name");
        assert_eq!(TestCatalog::Refinement.identity(), own_identity);

        assert_eq!(
            attribute_type_identity(&test_own()),
            &TestCatalog::Own.identity()
        );
        assert_eq!(
            attribute_type_identity(&test_validated_own()),
            &TestCatalog::ValidatedOwn.identity()
        );
        assert_eq!(
            attribute_type_identity(&test_refinement()),
            &TestCatalog::Refinement.identity()
        );
        assert_eq!(
            attribute_type_identity(&test_int_enum()),
            &TestCatalog::IntEnum.identity()
        );
    }
}
