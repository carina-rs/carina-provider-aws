//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_BUCKET_NAMESPACE: &[&str] = &["account-regional", "global"];

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: true,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("s3.Bucket")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("The name of the bucket to create. General purpose buckets - For information about bucket naming restrictions, see Bucket naming rules in the Amazon S3...")
                .with_provider_name("Bucket")
                .build(),
        )
        // Read contract: CreateBucket accepts the namespace selector but HeadBucket does not return it
        .attribute(
            AttributeSchema::optional("bucket_namespace", AttributeType::enum_(carina_core::schema::enum_identity("BucketNamespace", Some("aws.s3.Bucket")), Some(vec!["account-regional".to_string(), "global".to_string()]), vec![("account-regional".to_string(), "account_regional".to_string()), ("global".to_string(), "global".to_string())], None, None))
                .create_only()
                .write_only()
                .with_description("Specifies the namespace where you want to create your general purpose bucket. When you create a general purpose bucket, you can choose to create a buc...")
                .with_provider_name("BucketNamespace")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::S3BucketArn.attribute_type()))
                .with_description("ARN of the bucket. (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("region", AttributeType::string())
                .with_description("AWS region the bucket is in. (read-only)")
                .with_provider_name("Region")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("bucket_domain_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(1024))))))
                .with_description("Bucket domain name (`<bucket>.s3.amazonaws.com`). (read-only)")
                .with_provider_name("BucketDomainName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("bucket_regional_domain_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(1024))))))
                .with_description("Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`). (read-only)")
                .with_provider_name("BucketRegionalDomainName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("hosted_zone_id", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::Route53HostedZoneId.attribute_type()))
                .with_description("Route 53 Hosted Zone ID for the bucket's region. (read-only)")
                .with_provider_name("HostedZoneId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::map_with_key(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), None)))), AttributeType::string()))
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.length").expect("valid input rule code"), carina_core::schema::InputPredicate::Length { min: Some(3), max: Some(63) }))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.allowed_chars").expect("valid input rule code"), carina_core::schema::InputPredicate::AllowedChars(carina_core::schema::CharSet::new([('-', '.'), ('0', '9'), ('a', 'z')]).expect("valid input character set"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.first_char").expect("valid input rule code"), carina_core::schema::InputPredicate::FirstChar(carina_core::schema::CharSet::new([('0', '9'), ('a', 'z')]).expect("valid input character set"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.last_char").expect("valid input rule code"), carina_core::schema::InputPredicate::LastChar(carina_core::schema::CharSet::new([('0', '9'), ('a', 'z')]).expect("valid input character set"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_adjacent_dots").expect("valid input rule code"), carina_core::schema::InputPredicate::NoAdjacent { left: carina_core::schema::CharSet::new([('.', '.')]).expect("valid input character set"), right: carina_core::schema::CharSet::new([('.', '.')]).expect("valid input character set") }))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.not_ipv4").expect("valid input rule code"), carina_core::schema::InputPredicate::NotIpv4))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_prefix_xn").expect("valid input rule code"), carina_core::schema::InputPredicate::NoPrefix(carina_core::schema::NonEmptyString::new("xn--").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_prefix_sthree").expect("valid input rule code"), carina_core::schema::InputPredicate::NoPrefix(carina_core::schema::NonEmptyString::new("sthree-").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_prefix_amzn_s3_demo").expect("valid input rule code"), carina_core::schema::InputPredicate::NoPrefix(carina_core::schema::NonEmptyString::new("amzn-s3-demo-").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_suffix_s3alias").expect("valid input rule code"), carina_core::schema::InputPredicate::NoSuffix(carina_core::schema::NonEmptyString::new("-s3alias").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_suffix_ol_s3").expect("valid input rule code"), carina_core::schema::InputPredicate::NoSuffix(carina_core::schema::NonEmptyString::new("--ol-s3").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_suffix_mrap").expect("valid input rule code"), carina_core::schema::InputPredicate::NoSuffix(carina_core::schema::NonEmptyString::new(".mrap").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_suffix_x_s3").expect("valid input rule code"), carina_core::schema::InputPredicate::NoSuffix(carina_core::schema::NonEmptyString::new("--x-s3").expect("nonempty input token"))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("bucket").expect("valid input path")), carina_core::schema::InputRuleCode::new("s3.bucket.no_suffix_table_s3").expect("valid input rule code"), carina_core::schema::InputPredicate::NoSuffix(carina_core::schema::NonEmptyString::new("--table-s3").expect("nonempty input token"))))
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.Bucket", &[("bucket_namespace", VALID_BUCKET_NAMESPACE)])
}
