//! route53.RecordSet schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.route53
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};

const VALID_TYPE: &[&str] = &[
    "A", "AAAA", "CAA", "CNAME", "DS", "HTTPS", "MX", "NAPTR", "NS", "PTR", "SOA", "SPF", "SRV",
    "SSHFP", "SVCB", "TLSA", "TXT",
];

/// Returns the schema config for route53.RecordSet (Smithy: com.amazonaws.route53)
pub fn route53_record_set_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Route53::RecordSet",
        resource_type_name: "route53.RecordSet",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("route53.RecordSet")
        .with_description("Information about the resource record set to create or delete.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("alias_target", AttributeType::struct_(
                    "AliasTarget".to_string(),
                    vec![
                    StructField::required("dns_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(1024)))))).with_description("Alias resource record sets only: The value that you specify depends on where you want to route queries: Amazon API Gateway custom regional APIs and ed...").with_provider_name("DNSName").build(),
                    StructField::required("evaluate_target_health", AttributeType::bool()).with_description("Applies only to alias, failover alias, geolocation alias, latency alias, and weighted alias resource record sets: When EvaluateTargetHealth is true, a...").with_provider_name("EvaluateTargetHealth").build(),
                    StructField::required("hosted_zone_id", AttributeType::union(vec![carina_aws_types::with_member_values(carina_aws_types::AwsType::CloudFrontHostedZoneId.attribute_type(), &["Z2FDTNDATAQYW2"]), carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(0), Some(32))))), carina_aws_types::AwsType::Route53HostedZoneId.definition())])).with_description("Alias resource records sets only: The value used depends on where you want to route traffic: Amazon API Gateway custom regional APIs and edge-optimize...").with_provider_name("HostedZoneId").build()
                    ],
                ))
                .with_description("Alias resource record sets only: Information about the Amazon Web Services resource, such as a CloudFront distribution or an Amazon S3 bucket, that yo...")
                .with_provider_name("AliasTarget")
                .build(),
        )
        .attribute(
            AttributeSchema::required("name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(1024))))))
                .create_only()
                .identity()
                .with_description("For ChangeResourceRecordSets requests, the name of the record that you want to create, update, or delete. For ListResourceRecordSets responses, the na...")
                .with_provider_name("Name")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("resource_records", AttributeType::refined_list(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(0), Some(4000))))), true, Some((Some(1), None)), carina_core::schema::ListElements::AllowDuplicates))
                .with_description("Information about the resource records to act upon. If you're creating an alias resource record set, omit ResourceRecords.")
                .with_provider_name("ResourceRecords")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ttl", AttributeType::int_member(carina_core::schema::IntIdentityLayer::new(None, None), carina_core::schema::IntMemberConstraints::new(Some((Some(0), Some(2147483647))))))
                .with_description("The resource record cache time to live (TTL), in seconds. Note the following: If you're creating or updating an alias resource record set, omit TTL. A...")
                .with_provider_name("TTL")
                .build(),
        )
        .attribute(
            AttributeSchema::required("type", carina_aws_types::with_named_enum_values(carina_aws_types::AwsType::DnsRecordType.definition(), &["A", "AAAA", "CAA", "CNAME", "DS", "HTTPS", "MX", "NAPTR", "NS", "PTR", "SOA", "SPF", "SRV", "SSHFP", "SVCB", "TLSA", "TXT"], &[("A", "a"), ("AAAA", "aaaa"), ("CAA", "caa"), ("CNAME", "cname"), ("DS", "ds"), ("HTTPS", "https"), ("MX", "mx"), ("NAPTR", "naptr"), ("NS", "ns"), ("PTR", "ptr"), ("SOA", "soa"), ("SPF", "spf"), ("SRV", "srv"), ("SSHFP", "sshfp"), ("SVCB", "svcb"), ("TLSA", "tlsa"), ("TXT", "txt")]))
                .create_only()
                .identity()
                .with_description("The DNS record type. For information about different record types and how data is encoded for them, see Supported DNS Resource Record Types in the Ama...")
                .with_provider_name("Type")
                .build(),
        )
        .attribute(
            AttributeSchema::required("hosted_zone_id", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(0), Some(32))))), carina_aws_types::AwsType::Route53HostedZoneId.definition()))
                .create_only()
                .identity()
                .with_description("The ID of the hosted zone that contains this record set.")
                .with_provider_name("HostedZoneId")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::EveryWrite, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("alias_target.dns_name").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^(?:[\\x21-\\x2D\\x2F-\\x5B\\x5D-\\x7E]|\\\\[0-3][0-7]{2}){1,63}(?:\\.(?:[\\x21-\\x2D\\x2F-\\x5B\\x5D-\\x7E]|\\\\[0-3][0-7]{2}){1,63})*\\.?\\z", carina_core::schema::PatternMatch::Full))))
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("name").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^(?:[\\x21-\\x2D\\x2F-\\x5B\\x5D-\\x7E]|\\\\[0-3][0-7]{2}){1,63}(?:\\.(?:[\\x21-\\x2D\\x2F-\\x5B\\x5D-\\x7E]|\\\\[0-3][0-7]{2}){1,63})*\\.?\\z", carina_core::schema::PatternMatch::Full))))
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("route53.RecordSet", &[("type", VALID_TYPE)])
}
