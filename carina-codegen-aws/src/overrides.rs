//! Overrides decorate a derived member. Representation choices are separate.
use carina_aws_types::AwsType;
use carina_core::schema::PatternSpec;

/// Resource and data-source views obtain their ARN identity from this table.
pub const RESOURCE_ARN_IDENTITIES: &[(&str, &str, AwsType)] = &[
    ("iam.Role", "arn", AwsType::IamRoleArn),
    (
        "acm.Certificate",
        "certificate_arn",
        AwsType::AcmCertificateArn,
    ),
    (
        "organizations.Account",
        "arn",
        AwsType::OrganizationsAccountArn,
    ),
    (
        "organizations.Organization",
        "arn",
        AwsType::OrganizationsOrganizationArn,
    ),
    ("s3.Bucket", "arn", AwsType::S3BucketArn),
    ("sts.CallerIdentity", "arn", AwsType::StsCallerIdentityArn),
];

pub fn resource_arn_identity(resource: &str) -> Option<AwsType> {
    RESOURCE_ARN_IDENTITIES
        .iter()
        .find(|(name, _, _)| *name == resource)
        .map(|(_, _, row)| *row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreIdentity {
    Ipv4Address,
    Ipv6Address,
    Ipv4Cidr,
    Ipv6Cidr,
    Email,
}

impl CoreIdentity {
    pub fn code(self) -> &'static str {
        match self {
            Self::Ipv4Address => "types::ipv4_address()",
            Self::Ipv6Address => "types::ipv6_address()",
            Self::Ipv4Cidr => "types::ipv4_cidr()",
            Self::Ipv6Cidr => "types::ipv6_cidr()",
            Self::Email => "types::email()",
        }
    }
    pub fn attribute_type(self) -> carina_core::schema::AttributeType {
        use carina_core::schema::types;
        match self {
            Self::Ipv4Address => types::ipv4_address(),
            Self::Ipv6Address => types::ipv6_address(),
            Self::Ipv4Cidr => types::ipv4_cidr(),
            Self::Ipv6Cidr => types::ipv6_cidr(),
            Self::Email => types::email(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringProfile {
    DnsName,
}

impl StringProfile {
    pub fn constraints(self) -> carina_core::schema::AttributeType {
        match self {
            Self::DnsName => carina_aws_types::route53_dns_name(),
        }
    }
}

/// No constructor, AttributeType, or replacement callback can inhabit this table.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeOverride {
    Identity(AwsType),
    EnumIdentity(AwsType),
    CoreIdentity(CoreIdentity),
    Identities(&'static [AwsType]),
    NamedValues {
        identity: AwsType,
        values: &'static [&'static str],
    },
    LocalEnum {
        namespace: &'static str,
        name: &'static str,
        values: &'static [&'static str],
    },
    IntRange {
        min: i64,
        max: i64,
    },
    /// Intersect structural length bounds and add an InputRule::Matches.
    /// The pattern never replaces the Smithy pattern in the declared type.
    RefinedString {
        pattern: PatternSpec,
        min_length: u64,
        max_length: u64,
    },
    StringBounds {
        min: u64,
        max: u64,
    },
    /// Add InputRule::Matches to an addressable writable string; otherwise error.
    StringPattern(PatternSpec),
    /// Intersect bounds and add the profile's pattern as InputRule::Matches.
    StringProfile(StringProfile),
    ListLength {
        min: u64,
        max: u64,
    },
    /// Select no inferred identity; never remove structural constraints.
    NoIdentity,
}

/// Closed DSL/wire representation declarations. These are not member overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    String,
    Int,
    Bool,
    UnorderedStrings,
    RecordValues,
    EmailValues,
    IamPolicyDocument,
    SqsRedrivePolicy,
    SqsRedriveAllowPolicy,
    NatGatewayAddresses,
    IdentityStoreName,
    BucketEncryptionRules,
    BucketReplicationRules,
    BucketLifecycleRules,
    BucketObjectLockRule,
    IndexDocument,
    ErrorDocument,
    RedirectAllRequests,
    BucketCorsRules,
    TopicConfigurations,
    QueueConfigurations,
    LambdaConfigurations,
    EventBridgeConfiguration,
    TargetObjectKeyFormat,
}

impl Representation {
    pub fn code(self) -> &'static str {
        match self {
            Self::String => "AttributeType::string()",
            Self::Int => "AttributeType::int()",
            Self::Bool => "AttributeType::bool()",
            Self::UnorderedStrings => "AttributeType::unordered_list(AttributeType::string())",
            Self::RecordValues => "AttributeType::list(AttributeType::string())",
            Self::EmailValues => "AttributeType::unordered_list(types::email())",
            Self::IamPolicyDocument => "super::iam_policy_document()",
            Self::SqsRedrivePolicy => "super::sqs_redrive_policy()",
            Self::SqsRedriveAllowPolicy => "super::sqs_redrive_allow_policy()",
            Self::NatGatewayAddresses => {
                "carina_aws_types::nat_gateway_availability_zone_addresses()"
            }
            Self::IdentityStoreName => "super::identitystore_user_name()",
            Self::BucketEncryptionRules => "super::bucket_encryption_rules()",
            Self::BucketReplicationRules => "super::bucket_replication_rules()",
            Self::BucketLifecycleRules => "super::bucket_lifecycle_rules()",
            Self::BucketObjectLockRule => "super::bucket_object_lock_rule()",
            Self::IndexDocument => "super::s3_index_document()",
            Self::ErrorDocument => "super::s3_error_document()",
            Self::RedirectAllRequests => "super::s3_redirect_all_requests_to()",
            Self::BucketCorsRules => "super::bucket_cors_rules()",
            Self::TopicConfigurations => "super::bucket_topic_configurations()",
            Self::QueueConfigurations => "super::bucket_queue_configurations()",
            Self::LambdaConfigurations => "super::bucket_lambda_function_configurations()",
            Self::EventBridgeConfiguration => "super::bucket_event_bridge_configuration()",
            Self::TargetObjectKeyFormat => "super::bucket_target_object_key_format()",
        }
    }
}

/// Data-source declarations select a representation and optional decorations.
#[derive(Debug, Clone, PartialEq)]
pub struct DataSourceType {
    pub representation: Representation,
    pub decoration: Option<TypeOverride>,
}

impl DataSourceType {
    pub const fn plain(representation: Representation) -> Self {
        Self {
            representation,
            decoration: None,
        }
    }
    pub const fn named(identity: AwsType) -> Self {
        Self {
            representation: Representation::String,
            decoration: Some(TypeOverride::Identity(identity)),
        }
    }
}
