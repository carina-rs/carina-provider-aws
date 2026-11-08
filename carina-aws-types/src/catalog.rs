//! Closed catalog of reusable AWS schema identities.
//!
//! Each row below owns the identity axes and the `AttributeType` constructor
//! that carries that identity. Public constructor functions are generated from
//! the same row, so a wrapper cannot accidentally pair one identity with
//! another type's validation.

use aws_smithy_types::{DateTime, date_time::Format};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeType, DslTransform, PatternMatch, PatternSpec, TypeIdentity, legacy_validator,
};

use crate::{
    arn_partition_pattern, arn_regex, validate_arn, validate_availability_zone,
    validate_availability_zone_id, validate_aws_account_id, validate_aws_region,
    validate_aws_resource_id, validate_iam_arn, validate_iam_policy_name, validate_iam_role_id,
    validate_iam_role_name, validate_ipam_pool_id, validate_kms_key_arn, validate_kms_key_id,
    validate_prefixed_resource_id,
};

// IAM roleNameType and policyNameType use ^[\w+=,.@-]+$.
// Spell out ASCII \w and require the match to consume the entire input.
const IAM_NAME_PATTERN: &str = r"^[A-Za-z0-9_+=,.@-]+\z";

/// ACM DomainNameString, translated without Smithy's unsupported lookahead.
/// Shared by request refinements and the broader read-side domain refinements.
pub const ACM_CERTIFICATE_DOMAIN_NAME_PATTERN: &str = r"^(?:\*\.)?(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z0-9][A-Za-z0-9-]{0,61}[A-Za-z0-9]\z";

/// Additional public-certificate request rules, included in generated docs.
pub const ACM_CERTIFICATE_REQUEST_DOMAIN_RULE: &str = "Labels with -- in positions 3–4 must start with xn--; xn-- labels must decode as valid IDNA labels.";

fn acm_request_domain_type(identity: TypeIdentity, maximum: usize) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(PatternSpec::new(
            ACM_CERTIFICATE_DOMAIN_NAME_PATTERN,
            PatternMatch::Full,
        )),
        Some((Some(1), Some(maximum as u64))),
        legacy_validator(move |value| {
            let Value::Concrete(ConcreteValue::String(domain)) = value else {
                return Err("Expected string".into());
            };
            let domain = domain.expose_plaintext();
            static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
                regex::Regex::new(ACM_CERTIFICATE_DOMAIN_NAME_PATTERN).expect("ACM domain pattern")
            });
            if domain.len() > maximum || !PATTERN.is_match(domain) {
                return Err(
                    format!("expected an ACM wildcard FQDN of 1..={maximum} octets").into(),
                );
            }
            for label in domain.strip_prefix("*.").unwrap_or(domain).split('.') {
                if label.as_bytes().get(2..4) == Some(b"--") {
                    if !label[..2].eq_ignore_ascii_case("xn") {
                        return Err(ACM_CERTIFICATE_REQUEST_DOMAIN_RULE.into());
                    }
                    // idna 1.1.0 ToASCII validates the decoded A-label; merely
                    // decoding Punycode would also accept invalid IDNA labels.
                    idna::domain_to_ascii_strict(label)
                        .map_err(|_| ACM_CERTIFICATE_REQUEST_DOMAIN_RULE.to_string())?;
                }
            }
            Ok(())
        }),
        None,
    )
}

// HttpRedirect URLs include a domain/authority for HTTP GET validation, but
// specify neither a scheme restriction nor a numeric length cap.
// https://docs.aws.amazon.com/acm/latest/APIReference/API_HttpRedirect.html
const ACM_VALIDATION_URL_PATTERN: &str =
    r"^[A-Za-z][A-Za-z0-9+.-]*://[^\s\x00-\x1F\x7F/?#]+(?:[/?#][^\s\x00-\x1F\x7F]*)?\z";

fn validate_acm_validation_url(value: &str) -> Result<(), String> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("validation URL must not contain whitespace or control characters".into());
    }
    let (_, rest) = value
        .split_once("://")
        .ok_or("expected an absolute URL with an authority")?;
    if rest.starts_with(['/', '?', '#']) {
        return Err("validation URL must have a nonempty authority".into());
    }
    let violation = std::cell::Cell::new(None);
    let parsed = url::Url::options()
        .syntax_violation_callback(Some(&|reason| violation.set(Some(reason))))
        .parse(value)
        .map_err(|error| format!("invalid absolute URL: {error}"))?;
    if let Some(reason) = violation.get() {
        return Err(format!("invalid URL syntax: {reason}"));
    }
    if !parsed.has_host() {
        return Err("validation URL must have a host".into());
    }
    Ok(())
}

fn validated_string(
    identity: TypeIdentity,
    pattern: PatternSpec,
    length: Option<(Option<u64>, Option<u64>)>,
    validator: fn(&str) -> Result<(), String>,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern),
        length,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validator(value.expose_plaintext()).map_err(Into::into)
            } else {
                Err("Expected string".into())
            }
        }),
        None,
    )
}

fn metadata_string(
    identity: TypeIdentity,
    pattern: PatternSpec,
    length: Option<(Option<u64>, Option<u64>)>,
) -> AttributeType {
    AttributeType::refined_string(Some(identity), Some(pattern), length, None)
}

/// Construct an identity-bearing Int restricted to the given allowed values.
///
/// Catalog rows supply `values:` so both providers can expose the same set
/// through `allowed_int_values()` and use this validator for native and SDK
/// custom-type dispatch.
pub fn validated_int_enum(identity: TypeIdentity, valid_values: &'static [i64]) -> AttributeType {
    AttributeType::refined_int_with_validator(
        identity,
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::Int(value)) = value {
                if valid_values.contains(value) {
                    Ok(())
                } else {
                    Err(
                        format!("Value {value} is not valid; allowed values are: {valid_values:?}")
                            .into(),
                    )
                }
            } else {
                Err("Expected integer".into())
            }
        }),
    )
}

fn ec2_int_enum(identity: TypeIdentity, values: &'static [i64]) -> AttributeType {
    AttributeType::refined_int_with_validator(
        identity,
        Some((values.first().copied(), values.last().copied())),
        legacy_validator(move |value| match value {
            Value::Concrete(ConcreteValue::Int(value)) if values.contains(value) => Ok(()),
            _ => Err(format!("expected one of the integer values {values:?}").into()),
        }),
    )
}

fn is_private_amazon_side_asn(value: i64) -> bool {
    (64512..=65534).contains(&value) || (4200000000..=4294967294).contains(&value)
}

fn ec2_amazon_side_asn_type(identity: TypeIdentity) -> AttributeType {
    AttributeType::refined_int_with_validator(
        identity,
        Some((Some(64512), Some(4294967294))),
        legacy_validator(|value| match value {
            Value::Concrete(ConcreteValue::Int(value)) if is_private_amazon_side_asn(*value) => {
                Ok(())
            }
            _ => Err("expected a private ASN in 64512..=65534 or 4200000000..=4294967294".into()),
        }),
    )
}

// https://aws.amazon.com/vpn/faqs/ — Virtual private gateway legacy ASNs:
// 9059 (Dublin), 17493 (Singapore), 10124 (Tokyo), 7224 (other regions).
const VPN_GATEWAY_LEGACY_AMAZON_ASNS: &[i64] = &[7224, 9059, 10124, 17493];

fn ec2_vpn_gateway_amazon_side_asn_type(identity: TypeIdentity) -> AttributeType {
    AttributeType::refined_int_with_validator(
        identity,
        Some((Some(VPN_GATEWAY_LEGACY_AMAZON_ASNS[0]), Some(4294967294))),
        legacy_validator(|value| {
            match value {
            Value::Concrete(ConcreteValue::Int(value))
                if is_private_amazon_side_asn(*value)
                    || VPN_GATEWAY_LEGACY_AMAZON_ASNS.contains(value) => Ok(()),
            _ => Err("expected a private ASN in 64512..=65534 or 4200000000..=4294967294, or legacy Amazon ASN 7224, 9059, 10124, or 17493".into()),
        }
        }),
    )
}

fn ec2_cidr_type(
    identity: TypeIdentity,
    validate: fn(&str) -> Result<(), String>,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        None,
        None,
        legacy_validator(move |value| match value {
            Value::Concrete(ConcreteValue::String(value)) => {
                validate(value.expose_plaintext()).map_err(Into::into)
            }
            _ => Err("expected a CIDR string".into()),
        }),
        None,
    )
}

fn ipv4_cidr_parts(value: &str) -> Result<([u8; 4], u8), String> {
    // Preserve the established parser's accepted spellings and host bits.
    // These validators never rewrite an authored or stored CIDR.
    carina_core::schema::validate_ipv4_cidr(value)?;
    let (address, prefix) = value.split_once('/').expect("validated CIDR");
    let mut octets = address
        .split('.')
        .map(|part| part.parse::<u8>().expect("validated IPv4 octet"));
    Ok((
        std::array::from_fn(|_| octets.next().expect("four IPv4 octets")),
        prefix.parse().expect("validated prefix"),
    ))
}

fn validate_ec2_subnet_ipv4_cidr(value: &str) -> Result<(), String> {
    let (_, prefix) = ipv4_cidr_parts(value)?;
    if (16..=28).contains(&prefix) {
        Ok(())
    } else {
        Err("IPv4 subnet CIDR prefix must be 16..=28".into())
    }
}

fn validate_ec2_vpc_cidr(value: &str) -> Result<(), String> {
    validate_ec2_subnet_ipv4_cidr(value)?;
    let (octets, _) = ipv4_cidr_parts(value)?;
    if matches!(octets, [0 | 127, ..] | [169, 254, ..] | [224..=239, ..]) {
        Err("VPC CIDR must not use 0/8, 127/8, 169.254/16, or 224/4".into())
    } else {
        Ok(())
    }
}

fn validate_ec2_subnet_ipv6_cidr(value: &str) -> Result<(), String> {
    carina_core::schema::validate_ipv6_cidr(value)?;
    let (_, prefix) = value.split_once('/').expect("validated IPv6 CIDR");
    if [44, 48, 52, 56, 60, 64].contains(&prefix.parse::<u8>().expect("validated prefix")) {
        Ok(())
    } else {
        Err("IPv6 subnet CIDR prefix must be 44, 48, 52, 56, 60, or 64".into())
    }
}

fn validate_ec2_transit_gateway_cidr(value: &str) -> Result<(), String> {
    if value.contains(':') {
        carina_core::schema::validate_ipv6_cidr(value)?;
        let (_, prefix) = value.split_once('/').expect("validated IPv6 CIDR");
        if prefix.parse::<u8>().expect("validated prefix") <= 64 {
            return Ok(());
        }
    } else {
        let (octets, prefix) = ipv4_cidr_parts(value)?;
        if prefix <= 24 {
            // Reject CIDRs intersecting the documented link-local exclusion,
            // including supernets of 169.254/16. Host bits remain accepted.
            let mask = u32::MAX
                .checked_shl(u32::from(32 - prefix.min(16)))
                .unwrap_or(0);
            if (u32::from_be_bytes(octets) & mask) != (u32::from_be_bytes([169, 254, 0, 0]) & mask)
            {
                return Ok(());
            }
            return Err("transit gateway CIDR must not overlap 169.254.0.0/16".into());
        }
    }
    Err("transit gateway CIDR must be IPv4 /24 or larger, or IPv6 /64 or larger".into())
}

const EC2_SECURITY_GROUP_NAME_PATTERN: &str = r"^[A-Za-z0-9 ._:/()#,@\[\]+=\&;{}!$*\-]+\z";

fn validate_ec2_security_group_name(value: &str) -> Result<(), String> {
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(EC2_SECURITY_GROUP_NAME_PATTERN).expect("EC2 security group name pattern")
    });
    if value.len() <= 255 && PATTERN.is_match(value) && !value.starts_with("sg-") {
        Ok(())
    } else {
        Err("security group name must be 1..=255 permitted ASCII characters and must not start with sg-".into())
    }
}

fn prefixed_resource_id(identity: TypeIdentity, prefix: &'static str) -> AttributeType {
    let pattern = format!("^{prefix}-[0-9a-f]{{8,}}$");
    AttributeType::refined_string_with_validator(
        identity,
        Some(PatternSpec::new(pattern, PatternMatch::Full)),
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_prefixed_resource_id(value.expose_plaintext(), prefix).map_err(Into::into)
            } else {
                Err("Expected string".into())
            }
        }),
        None,
    )
}

fn iam_arn(
    identity: TypeIdentity,
    pattern: PatternSpec,
    resource_prefix: &'static str,
) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        Some(pattern),
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_iam_arn(value.expose_plaintext(), resource_prefix).map_err(Into::into)
            } else {
                Err("Expected string".into())
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
                validator(value.expose_plaintext()).map_err(Into::into)
            } else {
                Err("Expected string".into())
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
                    .map_err(Into::into)
            } else {
                Err("Expected string".into())
            }
        })),
        Some(DslTransform::HyphenToUnderscore),
    )
}

// Include Gregorian calendar validity in the transported pattern as well as
// the native validator. Protocol schemas cannot serialize Rust callbacks.
const S3_LIFECYCLE_DATE_PATTERN: &str = concat!(
    r"^(?:[0-9]{4}-(?:(?:01|03|05|07|08|10|12)-(?:0[1-9]|[12][0-9]|3[01])|",
    r"(?:04|06|09|11)-(?:0[1-9]|[12][0-9]|30)|02-(?:0[1-9]|1[0-9]|2[0-8]))|",
    r"(?:[0-9]{2}(?:0[48]|[2468][048]|[13579][26])|(?:[02468][048]|[13579][26])00)-02-29)",
    r"T00:00:00Z\z",
);

fn s3_lifecycle_date_type(identity: TypeIdentity) -> AttributeType {
    validated_string(
        identity,
        // LifecycleExpiration$Date and Transition$Date always use midnight UTC.
        // https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleExpiration.html
        // https://docs.aws.amazon.com/AmazonS3/latest/API/API_Transition.html
        PatternSpec::new(S3_LIFECYCLE_DATE_PATTERN, PatternMatch::Full),
        Some((Some(20), Some(20))),
        |value| {
            let parsed = DateTime::from_str(value, Format::DateTime)
                .map_err(|error| format!("invalid UTC RFC 3339 timestamp: {error}"))?;
            let canonical = parsed
                .fmt(Format::DateTime)
                .map_err(|error| format!("cannot format UTC RFC 3339 timestamp: {error}"))?;
            // Smithy's parser can preserve an impossible day. Check the
            // Gregorian calendar before comparing the timestamp round trip.
            let components = value
                .split('T')
                .next()
                .unwrap_or("")
                .split('-')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "invalid lifecycle calendar date".to_string())?;
            let [year, month, day] = components.as_slice() else {
                return Err("invalid lifecycle calendar date".to_string());
            };
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let maximum = match month {
                2 => {
                    if leap {
                        29
                    } else {
                        28
                    }
                }
                4 | 6 | 9 | 11 => 30,
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                _ => 0,
            };
            if *day == 0 || *day > maximum {
                return Err("invalid lifecycle calendar date".to_string());
            }
            if canonical == value && value.ends_with("T00:00:00Z") {
                Ok(())
            } else {
                Err(format!(
                    "timestamp must use canonical UTC RFC 3339 form {canonical}"
                ))
            }
        },
    )
}

fn organizations_account_email_type(identity: TypeIdentity) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        None,
        Some((Some(6), Some(64))),
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                let email = value.expose_plaintext();
                if !(6..=64).contains(&email.chars().count()) {
                    return Err(
                        "Organizations account email must be between 6 and 64 characters".into(),
                    );
                }
                // This is the same public function used by core's Email type.
                carina_core::schema::validate_email(email).map_err(Into::into)
            } else {
                Err("Expected string".into())
            }
        }),
        None,
    )
}

const DNS_RECORD_TYPE_VALUES: &[&str] = &[
    "A", "AAAA", "CAA", "CNAME", "DS", "HTTPS", "MX", "NAPTR", "NS", "PTR", "SOA", "SPF", "SRV",
    "SSHFP", "SVCB", "TLSA", "TXT",
];

fn dns_record_type_type(identity: TypeIdentity) -> AttributeType {
    crate::common::enum_with_dsl_aliases(DNS_RECORD_TYPE_VALUES, identity)
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

            /// Rust variant name used by schema code generation.
            #[doc(hidden)]
            $visibility const fn rust_variant_name(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant)),*
                }
            }

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
        constructor_path_prefix: "carina_aws_types::",
        rows: {
    /// Generic AWS ARN.
    Arn => arn {
        segments: [],
        kind: "Arn",
        valid_sample: "arn:aws:s3:::example-bucket",
        has_validator: true,
        constructor: |identity| validated_string(
            identity,
            PatternSpec::new(arn_regex("[^:]+:.*$"), PatternMatch::Full),
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
            PatternSpec::new("^[a-z-]+-[0-9a-f]{8,}$", PatternMatch::Full),
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
            PatternSpec::new("^\\d{12}$", PatternMatch::Full),
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
            PatternSpec::new("^[a-z]+[0-9]+-az[0-9]+$", PatternMatch::Full),
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
    /// IAM role name. Smithy com.amazonaws.iam#roleNameType in
    /// carina-provider-aws/tests/fixtures/smithy/iam.json: length 1..64.
    /// All 22 RoleName operation input members use this shape (2026-11-03 audit).
    IamRoleName => iam_role_name {
        segments: ["iam", "Role"],
        kind: "Name",
        valid_sample: "example-role",
        has_validator: true,
        constructor: |identity| validated_string(
            identity, PatternSpec::new(IAM_NAME_PATTERN, PatternMatch::Full), Some((Some(1), Some(64))), validate_iam_role_name,
        ),
    },
    /// IAM policy name, including inline policies. Smithy policyNameType in
    /// carina-provider-aws/tests/fixtures/smithy/iam.json: length 1..128.
    /// All 11 PolicyName input members use this shape, including nested inputs
    /// (2026-11-03 audit). Shared with AWSCC's RolePolicy and Role.Policies.
    IamPolicyName => iam_policy_name {
        segments: ["iam", "Policy"],
        kind: "Name",
        valid_sample: "example-policy",
        has_validator: true,
        constructor: |identity| validated_string(
            identity, PatternSpec::new(IAM_NAME_PATTERN, PatternMatch::Full), Some((Some(1), Some(128))), validate_iam_policy_name,
        ),
    },
    /// IAM role ID.
    IamRoleId => iam_role_id {
        segments: ["iam", "Role"],
        kind: "Id",
        valid_sample: "AROA1234567890EXAMPLE",
        has_validator: true,
        constructor: |identity| validated_string(
            identity,
            PatternSpec::new("^AROA[A-Z0-9]+$", PatternMatch::Full),
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
            PatternSpec::new(arn_regex("iam::[^:]*:role/.+$"), PatternMatch::Full),
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
            PatternSpec::new(arn_regex("iam::[^:]*:policy/.+$"), PatternMatch::Full),
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
            PatternSpec::new(arn_regex("iam::[^:]*:oidc-provider/.+$"), PatternMatch::Full),
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
            PatternSpec::new([
                "^((arn:",
                arn_partition_pattern(),
                ":kms:[^:]*:[^:]*:(key|alias)/.+)|(alias/.+)|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}))$",
            ].concat(), PatternMatch::Full),
            None,
            validate_kms_key_id,
        ),
    },
    /// Comparison-only KMS reference. Authored inputs remain strings.
    KmsKeyReference => kms_key_reference {
        segments: ["kms", "Key"],
        kind: "Reference",
        valid_sample: "alias/aws/sqs",
        constructor: |identity| AttributeType::reference(
            identity,
            Some(PatternSpec::new(crate::kms_key_reference_pattern(), PatternMatch::Full)),
            Some((Some(1), Some(2048))),
        ),
    },
    /// KMS key ARN (aliases are intentionally excluded).
    KmsKeyArn => kms_key_arn {
        segments: ["kms", "Key"],
        kind: "Arn",
        valid_sample: "arn:aws:kms:ap-northeast-1:123456789012:key/12345678-1234-1234-1234-123456789abc",
        has_validator: true,
        constructor: |identity| validated_string(
            identity,
            PatternSpec::new(crate::kms_key_arn_pattern(), PatternMatch::Full),
            None,
            validate_kms_key_arn,
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
            PatternSpec::new("^ipam-pool-[0-9a-f]{8,}$", PatternMatch::Full),
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
            PatternSpec::new("^Z[A-Z0-9]+$", PatternMatch::Full),
            Some((None, Some(32))),
        ),
    },
    /// Route 53 DNS record type.
    ///
    /// The identity intentionally keeps the Route 53 name
    /// `aws.route53.RecordSet.Type`: carina-core has no identity aliases, so
    /// renaming it would break existing configurations that use that identity.
    DnsRecordType => dns_record_type {
        segments: ["route53", "RecordSet"],
        kind: "Type",
        valid_sample: "CNAME",
        has_validator: false,
        constructor: dns_record_type_type,
    },
    /// Public RequestCertificate Common Name: DomainNameString plus the
    /// request's 64-octet cap and IDNA rules. Never use on read-side domains.
    /// https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html
    /// https://docs.aws.amazon.com/acm/latest/userguide/acm-certificate-characteristics.html
    AcmCertificateDomainName => acm_certificate_domain_name {
        segments: ["acm", "Certificate"],
        kind: "DomainName",
        valid_sample: "xn--r8jz45g.carina-rs.dev",
        has_validator: true,
        constructor: |identity| acm_request_domain_type(identity, 64),
    },
    /// Additional public certificate request domain: same IDNA rules, 253 cap.
    /// https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html
    /// https://docs.aws.amazon.com/acm/latest/userguide/acm-certificate-characteristics.html
    AcmCertificateSubjectAlternativeName => acm_certificate_subject_alternative_name {
        segments: ["acm", "Certificate"],
        kind: "SubjectAlternativeName",
        valid_sample: "xn--r8jz45g.carina-rs.dev",
        has_validator: true,
        constructor: |identity| acm_request_domain_type(identity, 253),
    },
    /// ACM certificate resource ARN:
    /// arn:${Partition}:acm:${Region}:${Account}:certificate/${CertificateId}.
    /// https://docs.aws.amazon.com/service-authorization/latest/reference/list_acm.html#awscertificatemanager-resources-for-iam-policies
    /// Arn in cached acm.json supplies the 20..2048 bound and ID character set;
    /// the resource grammar excludes other ACM resources and extra path parts.
    /// The authorization reference does not require a UUID-shaped CertificateId.
    /// Use the repo-wide partitions and 12-digit accounts, as for SQS (#638):
    /// https://docs.aws.amazon.com/IAM/latest/UserGuide/reference-arns.html
    AcmCertificateArn => acm_certificate_arn {
        segments: ["acm", "Certificate"],
        kind: "Arn",
        valid_sample: "arn:aws:acm:us-east-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new(arn_regex(r"acm:[a-z0-9-]+:[0-9]{12}:certificate/[A-Za-z0-9_+=,.@-]+\z"), PatternMatch::Full),
            Some((Some(20), Some(2048))),
        ),
    },
    /// RequestCertificate supports only these three algorithms. The other
    /// KeyAlgorithm enum members in acm.json are imported-certificate-only.
    /// Keep the existing AWS identity and aliases; AWSCC has its own catalog.
    /// https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html#ACM-RequestCertificate-request-KeyAlgorithm
    AcmCertificateKeyAlgorithm => acm_certificate_key_algorithm {
        segments: ["acm", "Certificate"],
        kind: "KeyAlgorithm",
        valid_sample: "RSA_2048",
        has_validator: false,
        constructor: |identity| crate::common::enum_with_dsl_aliases(
            &["EC_prime256v1", "EC_secp384r1", "RSA_2048"], identity,
        ),
    },
    /// Absolute URLs returned by HttpRedirect.{RedirectFrom,RedirectTo}.
    /// Reject every parser syntax violation without forcing HTTPS or capping
    /// the total URL length.
    /// https://docs.aws.amazon.com/acm/latest/APIReference/API_HttpRedirect.html
    AcmCertificateValidationUrl => acm_certificate_validation_url {
        segments: ["acm", "Certificate"],
        kind: "ValidationUrl",
        valid_sample: "http://example.com/.well-known/pki-validation/token.txt",
        has_validator: true,
        constructor: |identity| validated_string(
            identity, PatternSpec::new(ACM_VALIDATION_URL_PATTERN, PatternMatch::Full), None, validate_acm_validation_url,
        ),
    },
    /// ACM's com.amazonaws.acm#RecordType enum in acm.json contains only CNAME.
    /// Keep its identity separate from Route 53's broader DNS record type enum.
    AcmCertificateResourceRecordType => acm_certificate_resource_record_type {
        segments: ["acm", "Certificate"],
        kind: "ResourceRecordType",
        valid_sample: "CNAME",
        has_validator: false,
        constructor: |identity| crate::common::enum_with_dsl_aliases(&["CNAME"], identity),
    },
    /// ELBv2 HTTP matcher codes, independent of load-balancer protocol constraints.
    /// https://docs.aws.amazon.com/elasticloadbalancing/latest/APIReference/API_Matcher.html
    Elbv2TargetGroupHttpCode => elbv2_target_group_http_code {
        segments: ["elasticloadbalancingv2", "TargetGroup"],
        kind: "HttpCode",
        valid_sample: "200",
        has_validator: true,
        constructor: |identity| crate::elbv2_matcher_codes(identity, 200, 599),
    },
    /// ELBv2 gRPC matcher codes.
    /// https://docs.aws.amazon.com/elasticloadbalancing/latest/APIReference/API_Matcher.html
    Elbv2TargetGroupGrpcCode => elbv2_target_group_grpc_code {
        segments: ["elasticloadbalancingv2", "TargetGroup"],
        kind: "GrpcCode",
        valid_sample: "12",
        has_validator: true,
        constructor: |identity| crate::elbv2_matcher_codes(identity, 0, 99),
    },
    /// CloudWatch Logs retention days, sharing AWSCC's established identity.
    /// Source: PutRetentionPolicyRequest$retentionInDays -> Days.
    /// https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutRetentionPolicy.html
    LogsLogGroupRetentionInDays => logs_log_group_retention_in_days {
        segments: ["logs", "LogGroup"],
        kind: "RetentionInDays",
        valid_sample: "30",
        values: [
            1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096,
            1827, 2192, 2557, 2922, 3288, 3653,
        ],
        has_validator: true,
        constructor: validated_int_enum,
    },
    /// SQS queue ARN, including the queue-name grammar and FIFO suffix limit.
    /// The service authorization reference's queue resource is
    /// arn:${Partition}:sqs:${Region}:${Account}:${QueueName}:
    /// https://docs.aws.amazon.com/service-authorization/latest/reference/list_sqs.html#amazonsqs-resources-for-iam-policies
    /// QueueName: CreateQueueRequest$QueueName in the cached sqs.json; the
    /// .fifo suffix counts within 80 characters:
    /// https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-fifo-queue-message-identifiers.html
    /// Partition and account components:
    /// https://docs.aws.amazon.com/IAM/latest/UserGuide/reference-arns.html
    SqsQueueArn => sqs_queue_arn {
        segments: ["sqs", "Queue"],
        kind: "Arn",
        valid_sample: "arn:aws:sqs:ap-northeast-1:123456789012:example-queue",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new(arn_regex(r"sqs:[a-z0-9-]+:[0-9]{12}:(?:[A-Za-z0-9_-]{1,80}|[A-Za-z0-9_-]{1,75}\.fifo)\z"), PatternMatch::Full),
            None,
        ),
    },
    /// Closed SQS deduplication scope; aliases retain exact API spellings.
    /// sqs.json CreateQueueRequest$Attributes; API_CreateQueue.html.
    /// https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
    SqsQueueDeduplicationScope => sqs_queue_deduplication_scope {
        segments: ["sqs", "Queue"],
        kind: "DeduplicationScope",
        valid_sample: "messageGroup",
        has_validator: false,
        constructor: |identity| crate::common::enum_with_dsl_aliases(&["messageGroup", "queue"], identity),
    },
    /// Closed SQS FIFO throughput limit; aliases retain exact API spellings.
    /// sqs.json CreateQueueRequest$Attributes; API_CreateQueue.html.
    /// https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
    SqsQueueFifoThroughputLimit => sqs_queue_fifo_throughput_limit {
        segments: ["sqs", "Queue"],
        kind: "FifoThroughputLimit",
        valid_sample: "perQueue",
        has_validator: false,
        constructor: |identity| crate::common::enum_with_dsl_aliases(&["perQueue", "perMessageGroupId"], identity),
    },
    /// Existing redrive-permission identity, owned by the catalog so struct
    /// refinements and native constructors retain the same alias metadata.
    /// https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
    SqsQueueRedrivePermission => sqs_redrive_permission {
        segments: ["sqs", "Queue", "RedriveAllowPolicy"],
        kind: "RedrivePermission",
        valid_sample: "allowAll",
        has_validator: false,
        constructor: |identity| crate::common::enum_with_dsl_aliases(&["allowAll", "denyAll", "byQueue"], identity),
    },
    /// Identity Store ID.
    IdentityStoreId => identity_store_id {
        segments: ["identitystore", "Store"],
        kind: "Id",
        valid_sample: "d-1234567890",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new("^(d-[0-9a-fA-F]{10}|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$", PatternMatch::Full),
            None,
        ),
    },
    /// IAM Identity Center principal ID.
    SsoPrincipalId => sso_principal_id {
        segments: ["sso", "Principal"],
        kind: "Id",
        valid_sample: "12345678-1234-1234-1234-123456789abc",
        has_validator: false,
        constructor: |identity| metadata_string(identity, PatternSpec::new("^.{1,64}$", PatternMatch::Full), None),
    },
    /// IAM Identity Center instance ARN.
    SsoInstanceArn => sso_instance_arn {
        segments: ["sso", "Instance"],
        kind: "Arn",
        valid_sample: "arn:aws:sso:::instance/ssoins-1234567890abcdef",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new(arn_regex("sso:::instance/.+$"), PatternMatch::Full),
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
            PatternSpec::new(arn_regex("sso:::permissionSet/.+$"), PatternMatch::Full),
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
            PatternSpec::new(r"^(id|emailAddress|uri)=.+(\s*,\s*(id|emailAddress|uri)=.+)*$", PatternMatch::Full),
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
            PatternSpec::new(arn_regex("s3:::.+$"), PatternMatch::Full),
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
            PatternSpec::new(arn_regex("sts:.*$"), PatternMatch::Full),
            None,
        ),
    },
    /// Organizations email: length 6..64 plus core's existing Email validator.
    /// The cached organizations.json Email shape is shared by
    /// CreateAccountRequest$Email, Account$Email, and Organization$MasterAccountEmail.
    /// Length sources (including the read outputs):
    /// https://docs.aws.amazon.com/organizations/latest/APIReference/API_CreateAccount.html#organizations-CreateAccount-request-Email
    /// https://docs.aws.amazon.com/organizations/latest/APIReference/API_Account.html#organizations-Type-Account-Email
    /// https://docs.aws.amazon.com/organizations/latest/APIReference/API_Organization.html#organizations-Type-Organization-MasterAccountEmail
    /// Do not impose CreateAccount's stricter prose rules on existing account reads.
    OrganizationsAccountEmail => organizations_account_email {
        segments: ["organizations", "Account"],
        kind: "Email",
        valid_sample: "user@example.com",
        has_validator: true,
        constructor: organizations_account_email_type,
    },
    /// Organizations account ARN. Cached organizations.json AccountArn, used by
    /// Account$Arn and Organization$MasterAccountArn. Keep the model's empty
    /// region and resource grammar, spelling out ECMA-262's ASCII \d and
    /// anchoring the entire value. Smithy's aws-only partition restriction is
    /// narrower than the service (#638): Organizations is available in
    /// [GovCloud](https://docs.aws.amazon.com/govcloud-us/latest/UserGuide/govcloud-organizations.html)
    /// and [China (Beijing and Ningxia)](https://docs.amazonaws.cn/en_us/aws/latest/userguide/organizations.html).
    /// [GovCloud ARNs](https://docs.aws.amazon.com/govcloud-us/latest/UserGuide/using-govcloud-arns.html)
    /// use `arn:aws-us-gov:`;
    /// [China organization ARNs](https://docs.amazonaws.cn/en_us/AWSEC2/latest/UserGuide/get-org-ou-ARN.html)
    /// use `arn:aws-cn:`. The
    /// [Service Authorization Reference](https://docs.amazonaws.cn/en_us/service-authorization/latest/reference/list_awsorganizations.html)
    /// uses `arn:${Partition}:organizations::${Account}:account/o-${OrganizationId}/${AccountId}`.
    OrganizationsAccountArn => organizations_account_arn {
        segments: ["organizations", "Account"],
        kind: "Arn",
        valid_sample: "arn:aws:organizations::123456789012:account/o-exampleorgid/123456789012",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new(arn_regex(r"organizations::[0-9]{12}:account/o-[a-z0-9]{10,32}/[0-9]{12}\z"), PatternMatch::Full),
            None,
        ),
    },
    /// Organizations organization ARN. Cached organizations.json:
    /// Organization$Arn -> OrganizationArn. Use ASCII digits and a whole-value
    /// anchor as for AccountArn above. Smithy's aws-only partition restriction
    /// is narrower than the service (#638): Organizations is available in
    /// [GovCloud](https://docs.aws.amazon.com/govcloud-us/latest/UserGuide/govcloud-organizations.html)
    /// and [China (Beijing and Ningxia)](https://docs.amazonaws.cn/en_us/aws/latest/userguide/organizations.html).
    /// [GovCloud ARNs](https://docs.aws.amazon.com/govcloud-us/latest/UserGuide/using-govcloud-arns.html)
    /// use `arn:aws-us-gov:`;
    /// [China organization ARNs](https://docs.amazonaws.cn/en_us/AWSEC2/latest/UserGuide/get-org-ou-ARN.html)
    /// use `arn:aws-cn:`. Both resource formats use a Partition field in the
    /// [Service Authorization Reference](https://docs.amazonaws.cn/en_us/service-authorization/latest/reference/list_awsorganizations.html).
    OrganizationsOrganizationArn => organizations_organization_arn {
        segments: ["organizations", "Organization"],
        kind: "Arn",
        valid_sample: "arn:aws:organizations::123456789012:organization/o-exampleorgid",
        has_validator: false,
        constructor: |identity| metadata_string(
            identity,
            PatternSpec::new(arn_regex(r"organizations::[0-9]{12}:organization/o-[a-z0-9]{10,32}\z"), PatternMatch::Full),
            None,
        ),
    },
    /// TGW private ASN: ec2.json TransitGatewayRequestOptions$AmazonSideAsn.
    /// Its API and creation guide specify private ranges, with no legacy set.
    /// AWSCC owns its own types.
    /// https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TransitGatewayRequestOptions.html
    /// https://docs.aws.amazon.com/vpc/latest/tgw/create-tgw.html
    Ec2AmazonSideAsn => ec2_amazon_side_asn {
        segments: ["ec2"],
        kind: "AmazonSideAsn",
        valid_sample: "64512",
        has_validator: true,
        constructor: ec2_amazon_side_asn_type,
    },
    /// VGW private and legacy Amazon ASNs, including existing read-back values.
    /// The provider-wide identity is wider than the EC2 private-only identity:
    /// its empty segments preserve TGW -> VGW assignment through core widening.
    /// The reverse assignment lacks containment because VGWs can return legacy ASNs.
    /// https://aws.amazon.com/vpn/faqs/
    Ec2VpnGatewayAmazonSideAsn => ec2_vpn_gateway_amazon_side_asn {
        segments: [],
        kind: "AmazonSideAsn",
        valid_sample: "64512",
        has_validator: true,
        constructor: ec2_vpn_gateway_amazon_side_asn_type,
    },
    /// CreateFlowLogsRequest$MaxAggregationInterval -> Integer (prose).
    /// https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateFlowLogs.html
    Ec2FlowLogMaxAggregationInterval => ec2_flow_log_max_aggregation_interval {
        segments: ["ec2", "FlowLog"],
        kind: "MaxAggregationInterval",
        valid_sample: "60",
        values: [60, 600],
        has_validator: true,
        constructor: ec2_int_enum,
    },
    /// CreateSubnetRequest$Ipv6NetmaskLength -> NetmaskLength (prose).
    /// https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
    Ec2SubnetIpv6NetmaskLength => ec2_subnet_ipv6_netmask_length {
        segments: ["ec2", "Subnet"],
        kind: "Ipv6NetmaskLength",
        valid_sample: "64",
        values: [44, 48, 52, 56, 60, 64],
        has_validator: true,
        constructor: ec2_int_enum,
    },
    /// CreateSecurityGroupRequest$GroupName: ASCII alphabet and reserved prefix.
    /// https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateSecurityGroup.html
    Ec2SecurityGroupName => ec2_security_group_name {
        segments: ["ec2", "SecurityGroup"],
        kind: "Name",
        valid_sample: "carina-security-group",
        has_validator: true,
        constructor: |identity| validated_string(
            identity, PatternSpec::new(EC2_SECURITY_GROUP_NAME_PATTERN, PatternMatch::Full), Some((Some(1), Some(255))),
            validate_ec2_security_group_name,
        ),
    },
    /// CreateVpcRequest$CidrBlock; retain the parser, constrain prefix/ranges.
    /// Keep core's Ipv4Cidr kind so resource values widen to generic sinks.
    /// https://docs.aws.amazon.com/vpc/latest/userguide/vpc-cidr-blocks.html
    Ec2VpcCidrBlock => ec2_vpc_cidr_block {
        segments: ["ec2", "Vpc"],
        kind: "Ipv4Cidr",
        valid_sample: "10.0.0.0/16",
        has_validator: true,
        constructor: |identity| ec2_cidr_type(identity, validate_ec2_vpc_cidr),
    },
    /// CreateSubnetRequest$CidrBlock, preserving noncanonical input spellings.
    /// https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
    Ec2SubnetIpv4CidrBlock => ec2_subnet_ipv4_cidr_block {
        segments: ["ec2", "Subnet"],
        kind: "Ipv4Cidr",
        valid_sample: "10.0.0.0/24",
        has_validator: true,
        constructor: |identity| ec2_cidr_type(identity, validate_ec2_subnet_ipv4_cidr),
    },
    /// CreateSubnetRequest$Ipv6CidrBlock: /44..=/64 in increments of four.
    /// https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
    Ec2SubnetIpv6CidrBlock => ec2_subnet_ipv6_cidr_block {
        segments: ["ec2", "Subnet"],
        kind: "Ipv6Cidr",
        valid_sample: "2001:db8::/64",
        has_validator: true,
        constructor: |identity| ec2_cidr_type(identity, validate_ec2_subnet_ipv6_cidr),
    },
    /// TransitGatewayRequestOptions$TransitGatewayCidrBlocks accepts both families.
    /// This mixed domain cannot honestly widen to core's IPv4-only Ipv4Cidr.
    /// https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TransitGatewayRequestOptions.html
    /// https://docs.aws.amazon.com/vpc/latest/tgw/create-tgw.html
    Ec2TransitGatewayCidrBlock => ec2_transit_gateway_cidr_block {
        segments: ["ec2", "TransitGateway"],
        kind: "CidrBlock",
        valid_sample: "10.0.0.0/24",
        has_validator: true,
        constructor: |identity| ec2_cidr_type(identity, validate_ec2_transit_gateway_cidr),
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
    use std::collections::{BTreeSet, HashSet};

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
        metadata_string(
            identity,
            PatternSpec::new(
                ".*",
                PatternMatch::Substring {
                    reason: carina_core::schema::PatternReason::new(
                        "Preserve the test catalog search pattern, including newline inputs.",
                    )
                    .unwrap(),
                },
            ),
            None,
        )
    }

    fn test_int_constructor(
        identity: TypeIdentity,
        _allowed_values: &'static [i64],
    ) -> AttributeType {
        metadata_string(
            identity,
            PatternSpec::new("^[0-9]+$", PatternMatch::Full),
            None,
        )
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
            RawShape::Reference { identity, .. } | RawShape::Enum { identity, .. } => identity,
            shape => panic!("catalog constructor must carry a scalar identity, got {shape:?}"),
        }
    }

    fn attribute_type_validator(attribute_type: &AttributeType) -> Option<&CustomValidator> {
        match attribute_type.raw_shape() {
            RawShape::Reference { .. } => None,
            RawShape::String { validate, .. }
            | RawShape::Int { validate, .. }
            | RawShape::Float { validate, .. }
            | RawShape::Enum { validate, .. } => validate,
            shape => panic!("catalog constructor must carry a scalar identity, got {shape:?}"),
        }
    }

    #[test]
    fn iam_name_catalog_rows_match_smithy_constraints_and_callbacks() {
        for (resource, maximum, constructor, sample) in [
            (
                "Role",
                64,
                "iam_role_name",
                "AWSReservedSSO_AdministratorAccess_0123456789abcdef",
            ),
            ("Policy", 128, "iam_policy_name", "carina-example-policy"),
        ] {
            let expected = format!("aws.iam.{resource}.Name");
            let row = AwsType::ALL
                .iter()
                .copied()
                .find(|row| row.identity().to_string() == expected)
                .unwrap_or_else(|| panic!("missing shared catalog row {expected}"));
            let identity = row.identity();
            assert_eq!(identity.provider.as_deref(), Some("aws"));
            assert_eq!(identity.segments, ["iam", resource]);
            assert_eq!(identity.kind, "Name");
            assert_eq!(
                row.constructor_path(),
                format!("carina_aws_types::{constructor}()")
            );
            assert!(row.has_validator());
            let attr = row.attribute_type();
            let RawShape::String {
                identity: Some(actual),
                pattern,
                length,
                validate: Some(validate),
                ..
            } = attr.raw_shape()
            else {
                panic!("{expected} must be a validated, identity-bearing String");
            };
            assert_eq!(actual, &identity);
            assert_eq!(
                pattern.map(PatternSpec::as_str),
                Some(r"^[A-Za-z0-9_+=,.@-]+\z")
            );
            assert_eq!(length, Some((Some(1), Some(maximum))));
            for valid in [
                sample.to_string(),
                "AZaz09_+=,.@-".to_string(),
                "a".repeat(maximum as usize),
            ] {
                let value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
                assert!(validate(&value).is_ok(), "{expected}: {valid:?}");
            }
            for invalid in [
                "".to_string(),
                "bad name".to_string(),
                "a".repeat(maximum as usize + 1),
                "valid\n".to_string(),
                "rôle".to_string(),
                "invalid/".to_string(),
            ] {
                let value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
                assert!(validate(&value).is_err(), "{expected}: {invalid:?}");
            }
            assert!(validate(&Value::Concrete(ConcreteValue::Int(1))).is_err());
        }
    }

    #[test]
    fn elbv2_matcher_catalog_rows_report_errors_and_reject_wrong_types() {
        for (row, range) in [
            (AwsType::Elbv2TargetGroupHttpCode, "200..=599"),
            (AwsType::Elbv2TargetGroupGrpcCode, "0..=99"),
        ] {
            let attr = row.attribute_type();
            let validate = attribute_type_validator(&attr).expect("matcher validator");
            let error = validate(&Value::Concrete(ConcreteValue::String("".into())))
                .expect_err("empty matcher codes must fail")
                .to_string();
            assert_eq!(
                error,
                format!(
                    "Validation failed: ELBv2 matcher codes must be comma-separated items N or N-N (canonical decimal digits only, no leading zeros), with every N in {range} and range start <= end; no spaces or empty items"
                )
            );
            assert!(validate(&Value::Concrete(ConcreteValue::Int(200))).is_err());
        }
    }

    #[test]
    fn logs_log_group_retention_integer_enum_matches_awscc() {
        let row = AwsType::ALL
            .iter()
            .copied()
            .find(|row| row.identity().to_string() == "aws.logs.LogGroup.RetentionInDays")
            .expect("shared Logs retention catalog entry");
        let values: &[i64] = &[
            1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096, 1827, 2192, 2557,
            2922, 3288, 3653,
        ];
        assert_eq!(row.identity().segments, ["logs", "LogGroup"]);
        assert_eq!(row.identity().kind, "RetentionInDays");
        assert_eq!(row.allowed_int_values(), Some(values));
        assert_eq!(
            row.constructor_path(),
            "carina_aws_types::logs_log_group_retention_in_days()"
        );
        let attr = row.attribute_type();
        let RawShape::Int {
            identity: Some(identity),
            range: None,
            validate: Some(validate),
        } = attr.raw_shape()
        else {
            panic!("Logs retention must match AWSCC's validated Int shape");
        };
        assert_eq!(identity, &row.identity());
        assert!(row.has_validator());
        assert!(validate(&Value::Concrete(ConcreteValue::Int(30))).is_ok());
        let error = validate(&Value::Concrete(ConcreteValue::Int(2)))
            .expect_err("non-member must fail")
            .to_string();
        assert!(error.contains("allowed values"), "{error}");
        assert!(error.contains(&format!("{values:?}")), "{error}");
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
            AwsType::AcmCertificateDomainName,
            AwsType::AcmCertificateSubjectAlternativeName,
            AwsType::AcmCertificateValidationUrl,
            AwsType::Elbv2TargetGroupHttpCode,
            AwsType::Elbv2TargetGroupGrpcCode,
            AwsType::LogsLogGroupRetentionInDays,
            AwsType::IamRoleName,
            AwsType::IamPolicyName,
            AwsType::OrganizationsAccountEmail,
            AwsType::IamRoleId,
            AwsType::IamRoleArn,
            AwsType::IamPolicyArn,
            AwsType::KmsKeyId,
            AwsType::KmsKeyArn,
            AwsType::IpamPoolId,
            AwsType::Ec2AmazonSideAsn,
            AwsType::Ec2VpnGatewayAmazonSideAsn,
            AwsType::Ec2FlowLogMaxAggregationInterval,
            AwsType::Ec2SubnetIpv6NetmaskLength,
            AwsType::Ec2SecurityGroupName,
            AwsType::Ec2VpcCidrBlock,
            AwsType::Ec2SubnetIpv4CidrBlock,
            AwsType::Ec2SubnetIpv6CidrBlock,
            AwsType::Ec2TransitGatewayCidrBlock,
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
            56,
            "the shared catalog validator contract changed"
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
                let sample = Value::Concrete(match attribute_type.raw_shape() {
                    RawShape::Int { .. } => ConcreteValue::Int(
                        aws_type
                            .valid_sample()
                            .parse()
                            .expect("integer catalog sample"),
                    ),
                    _ => ConcreteValue::String(aws_type.valid_sample().to_string().into()),
                });
                assert!(
                    validator(&sample).is_ok(),
                    "declared sample {:?} must pass the validator for {aws_type:?}",
                    aws_type.valid_sample()
                );
            }
        }
    }

    #[test]
    fn dns_record_type_catalog_identity_values_and_aliases() {
        const VALUES: &[&str] = &[
            "A", "AAAA", "CAA", "CNAME", "DS", "HTTPS", "MX", "NAPTR", "NS", "PTR", "SOA", "SPF",
            "SRV", "SSHFP", "SVCB", "TLSA", "TXT",
        ];

        let attribute_type = dns_record_type();
        let RawShape::Enum {
            identity,
            values: Some(values),
            dsl_aliases,
            ..
        } = attribute_type.raw_shape()
        else {
            panic!("DNS record type must be a closed enum");
        };

        assert_eq!(
            AwsType::DnsRecordType.identity().to_string(),
            "aws.route53.RecordSet.Type"
        );
        assert_eq!(identity, &AwsType::DnsRecordType.identity());
        assert_eq!(
            values,
            &VALUES
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            dsl_aliases,
            &VALUES
                .iter()
                .map(|value| ((*value).to_string(), value.to_ascii_lowercase()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn acm_resource_record_type_catalog_identity_values_and_aliases() {
        let attribute_type = acm_certificate_resource_record_type();
        let RawShape::Enum {
            identity,
            values: Some(values),
            dsl_aliases,
            ..
        } = attribute_type.raw_shape()
        else {
            panic!("ACM resource record type must be a closed enum");
        };

        assert_eq!(
            identity.to_string(),
            "aws.acm.Certificate.ResourceRecordType"
        );
        assert_ne!(identity, &AwsType::DnsRecordType.identity());
        assert_eq!(values, &["CNAME".to_string()]);
        assert_eq!(dsl_aliases, &[("CNAME".to_string(), "cname".to_string())]);
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
    fn arn_catalog_rows_accept_every_partition_and_reject_unknown_partitions() {
        let expected_rows = BTreeSet::from([
            AwsType::Arn,
            AwsType::IamRoleArn,
            AwsType::IamPolicyArn,
            AwsType::IamOidcProviderArn,
            AwsType::KmsKeyId,
            AwsType::KmsKeyReference,
            AwsType::KmsKeyArn,
            AwsType::AcmCertificateArn,
            AwsType::SqsQueueArn,
            AwsType::SsoInstanceArn,
            AwsType::SsoPermissionSetArn,
            AwsType::S3BucketArn,
            AwsType::StsCallerIdentityArn,
            AwsType::OrganizationsAccountArn,
            AwsType::OrganizationsOrganizationArn,
        ]);
        let selected_rows: BTreeSet<_> = AwsType::ALL
            .iter()
            .copied()
            .filter(|row| {
                let attr = row.attribute_type();
                let pattern = match attr.raw_shape() {
                    RawShape::String { pattern, .. } | RawShape::Reference { pattern, .. } => {
                        pattern
                    }
                    _ => return false,
                };
                if expected_rows.contains(row) {
                    assert!(pattern.is_some(), "{row:?} must declare an ARN pattern");
                }
                // Include ARN alternatives in rows such as KMS IDs and references,
                // even when their primary sample is an alias or a bare key ID.
                row.identity().kind == "Arn" || pattern.is_some_and(|p| p.as_str().contains("arn:"))
            })
            .collect();
        let missing: Vec<_> = expected_rows.difference(&selected_rows).copied().collect();
        let extra: Vec<_> = selected_rows.difference(&expected_rows).copied().collect();
        assert_eq!(
            selected_rows, expected_rows,
            "ARN-capable catalog row selection changed; missing: {missing:?}; extra: {extra:?}"
        );

        for row in selected_rows {
            let attr = row.attribute_type();
            let pattern = match attr.raw_shape() {
                RawShape::String { pattern, .. } | RawShape::Reference { pattern, .. } => pattern,
                _ => panic!("{row:?} must be a String or Reference ARN row"),
            };
            let regex =
                regex::Regex::new(pattern.expect("ARN row must declare a pattern").as_str())
                    .expect("valid ARN row pattern");
            let sample = AwsType::ALL
                .iter()
                .map(|candidate| candidate.valid_sample())
                .find(|sample| sample.starts_with("arn:") && regex.is_match(sample))
                .unwrap_or_else(|| panic!("{row:?} must have an ARN sample in the catalog"));
            let rest = sample.splitn(3, ':').nth(2).expect("sample ARN fields");
            let admitted = matches!(attr.raw_shape(), RawShape::String { .. })
                .then(|| crate::tests::rootless(attr.clone()));

            for partition in crate::Partition::ALL {
                let arn = format!("arn:{}:{rest}", partition.id());
                assert!(regex.is_match(&arn), "{row:?} must accept {arn}");
                if let Some(admitted) = &admitted {
                    let value = Value::Concrete(ConcreteValue::String(arn.clone().into()));
                    assert!(
                        admitted.validate_collect(&value).is_empty(),
                        "{row:?} validation must accept {arn}"
                    );
                }
            }

            let arn = format!("arn:aws-xx:{rest}");
            assert!(!regex.is_match(&arn), "{row:?} must reject {arn}");
            if let Some(admitted) = &admitted {
                let value = Value::Concrete(ConcreteValue::String(arn.clone().into()));
                assert!(
                    !admitted.validate_collect(&value).is_empty(),
                    "{row:?} validation must reject {arn}"
                );
            }
        }
    }

    #[test]
    fn organizations_account_arn_matches_smithy_account_resource_and_shared_partitions() {
        let row = AwsType::OrganizationsAccountArn;
        let attr = row.attribute_type();
        let admitted = crate::tests::rootless(attr.clone());
        for valid in [
            "arn:aws:organizations::123456789012:account/o-0123456789/000000000000".to_string(),
            format!(
                "arn:aws:organizations::123456789012:account/o-{}/999999999999",
                "a".repeat(32)
            ),
            row.valid_sample().to_string(),
        ] {
            let value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(
                admitted.validate_collect(&value).is_empty(),
                "valid account ARN: {valid}"
            );
        }
        for invalid in [
            "arn:aws:organizations::123456789012:organization/o-0123456789",
            "arn:aws:organizations::12345678901:account/o-0123456789/123456789012",
            "arn:aws:organizations::123456789012:account/o-0123456789/1234567890123",
            "arn:aws:organizations:us-east-1:123456789012:account/o-0123456789/123456789012",
            "arn:aws:organizations::123456789012:account/o-example/123456789012",
            "arn:aws:organizations::123456789012:account/o-ABCDEFGHIJ/123456789012",
            "arn:aws:organizations::１２３４５６７８９０１２:account/o-0123456789/123456789012",
            "arn:aws:organizations::123456789012:account/o-0123456789/123456789012/extra",
            "arn:aws:organizations::123456789012:account/o-0123456789/123456789012\n",
        ] {
            let value = Value::Concrete(ConcreteValue::String(invalid.into()));
            assert!(
                !admitted.validate_collect(&value).is_empty(),
                "invalid account ARN: {invalid:?}"
            );
        }
        // #638: Organizations ARNs use the shared partitions, including China.
        assert!(
            admitted
                .validate_collect(&Value::Concrete(ConcreteValue::String(
                    "arn:aws-cn:organizations::123456789012:account/o-0123456789/123456789012"
                        .into()
                )))
                .is_empty()
        );
        // #638: Organizations ARNs use the shared partitions, including GovCloud.
        assert!(
            admitted
                .validate_collect(&Value::Concrete(ConcreteValue::String(
                    "arn:aws-us-gov:organizations::123456789012:account/o-0123456789/123456789012"
                        .into()
                )))
                .is_empty()
        );
        let RawShape::String { pattern, .. } = attr.raw_shape() else {
            panic!("account ARN must be a String");
        };
        let expected_pattern =
            crate::arn_regex(r"organizations::[0-9]{12}:account/o-[a-z0-9]{10,32}/[0-9]{12}\z");
        // #638: Organizations pattern text follows the shared Partition set.
        assert_eq!(
            pattern.map(PatternSpec::as_str),
            Some(expected_pattern.as_str())
        );
    }

    #[test]
    fn organizations_organization_arn_matches_smithy_resource_and_id_bounds() {
        let row = AwsType::OrganizationsOrganizationArn;
        let attr = row.attribute_type();
        let admitted = crate::tests::rootless(attr.clone());
        for valid in [
            "arn:aws:organizations::123456789012:organization/o-0123456789".to_string(),
            format!(
                "arn:aws:organizations::123456789012:organization/o-{}",
                "a".repeat(32)
            ),
            row.valid_sample().to_string(),
        ] {
            let value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(
                admitted.validate_collect(&value).is_empty(),
                "valid organization ARN: {valid}"
            );
        }
        for invalid in [
            "arn:aws:organizations::123456789012:account/o-0123456789/123456789012".to_string(),
            "arn:aws:organizations::12345678901:organization/o-0123456789".to_string(),
            "arn:aws:organizations::123456789012:organization/o-012345678".to_string(),
            format!(
                "arn:aws:organizations::123456789012:organization/o-{}",
                "a".repeat(33)
            ),
            "arn:aws:organizations::123456789012:organization/o-ABCDEFGHIJ".to_string(),
            "arn:aws:organizations::１２３４５６７８９０１２:organization/o-0123456789".to_string(),
            "arn:aws:organizations::123456789012:organization/o-0123456789/extra".to_string(),
            "arn:aws:organizations::123456789012:organization/o-0123456789\n".to_string(),
        ] {
            let value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(
                !admitted.validate_collect(&value).is_empty(),
                "invalid organization ARN: {invalid:?}"
            );
        }
        // #638: Organizations ARNs use the shared partitions, including China.
        assert!(
            admitted
                .validate_collect(&Value::Concrete(ConcreteValue::String(
                    "arn:aws-cn:organizations::123456789012:organization/o-0123456789".into()
                )))
                .is_empty()
        );
        // #638: Organizations ARNs use the shared partitions, including GovCloud.
        assert!(
            admitted
                .validate_collect(&Value::Concrete(ConcreteValue::String(
                    "arn:aws-us-gov:organizations::123456789012:organization/o-0123456789".into()
                )))
                .is_empty()
        );
        let RawShape::String { pattern, .. } = attr.raw_shape() else {
            panic!("organization ARN must be a String");
        };
        let expected_pattern =
            crate::arn_regex(r"organizations::[0-9]{12}:organization/o-[a-z0-9]{10,32}\z");
        // #638: Organizations pattern text follows the shared Partition set.
        assert_eq!(
            pattern.map(PatternSpec::as_str),
            Some(expected_pattern.as_str())
        );
    }

    #[test]
    fn organizations_account_email_catalog_exposes_identity_bounds_and_core_validation() {
        let row = AwsType::OrganizationsAccountEmail;
        assert!(row.has_validator());
        let attribute_type = row.attribute_type();
        let RawShape::String {
            identity: Some(identity),
            pattern,
            length,
            validate: Some(validator),
            ..
        } = attribute_type.raw_shape()
        else {
            panic!("Organizations Email must be a validated named String");
        };
        assert_eq!(identity, &row.identity());
        assert!(
            pattern.is_none(),
            "email format belongs to core's validator"
        );
        assert_eq!(length, Some((Some(6), Some(64))));
        let admitted = crate::tests::rootless(attribute_type.clone());
        for valid in [
            "ab@c.d".to_string(),
            format!("{}@b.com", "a".repeat(58)),
            format!("{}@b.com", "é".repeat(58)),
            "user+carina-rs-registry-prod@example.com".to_string(),
            "user+carina-rs-registry-dev@example.com".to_string(),
        ] {
            let value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(admitted.validate_collect(&value).is_empty(), "{valid:?}");
            // SDK custom-type dispatch invokes the callback directly, so it
            // must enforce the length bounds as well as the format check.
            assert!(validator(&value).is_ok(), "{valid:?}");
        }
        for invalid in ["a@b.c".to_string(), format!("{}@b.com", "a".repeat(59))] {
            assert!(carina_core::schema::validate_email(&invalid).is_ok());
            let value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(!admitted.validate_collect(&value).is_empty(), "{invalid:?}");
            assert!(validator(&value).is_err(), "{invalid:?}");
        }
        for invalid in ["not-an-email", "a@@b.com", "user@b..com", "user @b.com"] {
            let value = Value::Concrete(ConcreteValue::String(invalid.into()));
            let expected = carina_core::schema::validate_email(invalid).unwrap_err();
            let error = validator(&value).unwrap_err();
            assert!(
                error.to_string().contains(&expected),
                "delegate to core: {error}"
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
IamRoleName|aws|iam/Role|Name|example-role|true
IamPolicyName|aws|iam/Policy|Name|example-policy|true
IamRoleId|aws|iam/Role|Id|AROA1234567890EXAMPLE|true
IamRoleArn|aws|iam/Role|Arn|arn:aws:iam::123456789012:role/example-role|true
IamPolicyArn|aws|iam/Policy|Arn|arn:aws:iam::123456789012:policy/example-policy|true
IamOidcProviderArn|aws|iam/OidcProvider|Arn|arn:aws:iam::123456789012:oidc-provider/example.com|false
KmsKeyId|aws|kms/Key|Id|12345678-1234-1234-1234-123456789abc|true
KmsKeyReference|aws|kms/Key|Reference|alias/aws/sqs|false
KmsKeyArn|aws|kms/Key|Arn|arn:aws:kms:ap-northeast-1:123456789012:key/12345678-1234-1234-1234-123456789abc|true
IpamPoolId|aws|ec2/IpamPool|Id|ipam-pool-12345678|true
Route53HostedZoneId|aws|route53/HostedZone|Id|Z2FDTNDATAQYW2|false
DnsRecordType|aws|route53/RecordSet|Type|CNAME|false
AcmCertificateDomainName|aws|acm/Certificate|DomainName|xn--r8jz45g.carina-rs.dev|true
AcmCertificateSubjectAlternativeName|aws|acm/Certificate|SubjectAlternativeName|xn--r8jz45g.carina-rs.dev|true
AcmCertificateArn|aws|acm/Certificate|Arn|arn:aws:acm:us-east-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef|false
AcmCertificateKeyAlgorithm|aws|acm/Certificate|KeyAlgorithm|RSA_2048|false
AcmCertificateValidationUrl|aws|acm/Certificate|ValidationUrl|http://example.com/.well-known/pki-validation/token.txt|true
AcmCertificateResourceRecordType|aws|acm/Certificate|ResourceRecordType|CNAME|false
Elbv2TargetGroupHttpCode|aws|elasticloadbalancingv2/TargetGroup|HttpCode|200|true
Elbv2TargetGroupGrpcCode|aws|elasticloadbalancingv2/TargetGroup|GrpcCode|12|true
LogsLogGroupRetentionInDays|aws|logs/LogGroup|RetentionInDays|30|true
SqsQueueArn|aws|sqs/Queue|Arn|arn:aws:sqs:ap-northeast-1:123456789012:example-queue|false
SqsQueueDeduplicationScope|aws|sqs/Queue|DeduplicationScope|messageGroup|false
SqsQueueFifoThroughputLimit|aws|sqs/Queue|FifoThroughputLimit|perQueue|false
SqsQueueRedrivePermission|aws|sqs/Queue/RedriveAllowPolicy|RedrivePermission|allowAll|false
IdentityStoreId|aws|identitystore/Store|Id|d-1234567890|false
SsoPrincipalId|aws|sso/Principal|Id|12345678-1234-1234-1234-123456789abc|false
SsoInstanceArn|aws|sso/Instance|Arn|arn:aws:sso:::instance/ssoins-1234567890abcdef|false
SsoPermissionSetArn|aws|sso/PermissionSet|Arn|arn:aws:sso:::permissionSet/ssoins-1234567890abcdef/ps-1234567890abcdef|false
S3Grantee|aws|s3|Grantee|id=canonical-user-id|false
CloudFrontHostedZoneId|aws|cloudfront|HostedZoneId|Z2FDTNDATAQYW2|false
S3BucketArn|aws|s3/Bucket|Arn|arn:aws:s3:::example-bucket|false
StsCallerIdentityArn|aws|sts/CallerIdentity|Arn|arn:aws:sts::123456789012:assumed-role/example-role/example-session|false
OrganizationsAccountEmail|aws|organizations/Account|Email|user@example.com|true
OrganizationsAccountArn|aws|organizations/Account|Arn|arn:aws:organizations::123456789012:account/o-exampleorgid/123456789012|false
OrganizationsOrganizationArn|aws|organizations/Organization|Arn|arn:aws:organizations::123456789012:organization/o-exampleorgid|false
Ec2AmazonSideAsn|aws|ec2|AmazonSideAsn|64512|true
Ec2VpnGatewayAmazonSideAsn|aws||AmazonSideAsn|64512|true
Ec2FlowLogMaxAggregationInterval|aws|ec2/FlowLog|MaxAggregationInterval|60|true
Ec2SubnetIpv6NetmaskLength|aws|ec2/Subnet|Ipv6NetmaskLength|64|true
Ec2SecurityGroupName|aws|ec2/SecurityGroup|Name|carina-security-group|true
Ec2VpcCidrBlock|aws|ec2/Vpc|Ipv4Cidr|10.0.0.0/16|true
Ec2SubnetIpv4CidrBlock|aws|ec2/Subnet|Ipv4Cidr|10.0.0.0/24|true
Ec2SubnetIpv6CidrBlock|aws|ec2/Subnet|Ipv6Cidr|2001:db8::/64|true
Ec2TransitGatewayCidrBlock|aws|ec2/TransitGateway|CidrBlock|10.0.0.0/24|true
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
        assert_eq!(TestCatalog::Own.rust_variant_name(), "Own");

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
