//! route53.record_set schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.route53
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::resource::Value;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};

const VALID_TYPE: &[&str] = &[
    "A", "AAAA", "CAA", "CNAME", "DS", "HTTPS", "MX", "NAPTR", "NS", "PTR", "SOA", "SPF", "SRV",
    "SSHFP", "SVCB", "TLSA", "TXT",
];

fn validate_ttl_range(value: &Value) -> Result<(), String> {
    if let Value::Int(n) = value {
        if *n < 0 || *n > 2147483647 {
            Err(format!("Value {} is out of range 0..=2147483647", n))
        } else {
            Ok(())
        }
    } else {
        Err("Expected integer".to_string())
    }
}

/// Returns the schema config for route53.record_set (Smithy: com.amazonaws.route53)
pub fn route53_record_set_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Route53::RecordSet",
        resource_type_name: "route53.record_set",
        has_tags: false,
        schema: ResourceSchema::new("aws.route53.record_set")
        .with_description("Information about the resource record set to create or delete.")
        .attribute(
            AttributeSchema::new("alias_target", AttributeType::Struct {
                    name: "AliasTarget".to_string(),
                    fields: vec![
                    StructField::new("dns_name", AttributeType::String).required().with_description("Alias resource record sets only: The value that you specify depends on where you want to route queries: Amazon API Gateway custom regional APIs and ed...").with_provider_name("DNSName"),
                    StructField::new("evaluate_target_health", AttributeType::Bool).required().with_description("Applies only to alias, failover alias, geolocation alias, latency alias, and weighted alias resource record sets: When EvaluateTargetHealth is true, a...").with_provider_name("EvaluateTargetHealth"),
                    StructField::new("hosted_zone_id", AttributeType::String).required().with_description("Alias resource records sets only: The value used depends on where you want to route traffic: Amazon API Gateway custom regional APIs and edge-optimize...").with_provider_name("HostedZoneId")
                    ],
                })
                .with_description("Alias resource record sets only: Information about the Amazon Web Services resource, such as a CloudFront distribution or an Amazon S3 bucket, that yo...")
                .with_provider_name("AliasTarget"),
        )
        .attribute(
            AttributeSchema::new("name", AttributeType::String)
                .required()
                .create_only()
                .with_description("For ChangeResourceRecordSets requests, the name of the record that you want to create, update, or delete. For ListResourceRecordSets responses, the na...")
                .with_provider_name("Name"),
        )
        .attribute(
            AttributeSchema::new("resource_records", AttributeType::list(AttributeType::String))
                .with_description("Information about the resource records to act upon. If you're creating an alias resource record set, omit ResourceRecords.")
                .with_provider_name("ResourceRecords"),
        )
        .attribute(
            AttributeSchema::new("ttl", AttributeType::Custom {
                semantic_name: None,
                pattern: None,
                length: Some((Some(0), Some(2147483647))),
                base: Box::new(AttributeType::Int),
                validate: validate_ttl_range,
                namespace: None,
                to_dsl: None,
            })
                .with_description("The resource record cache time to live (TTL), in seconds. Note the following: If you're creating or updating an alias resource record set, omit TTL. A...")
                .with_provider_name("TTL"),
        )
        .attribute(
            AttributeSchema::new("type", AttributeType::StringEnum {
                name: "Type".to_string(),
                values: vec!["A".to_string(), "AAAA".to_string(), "CAA".to_string(), "CNAME".to_string(), "DS".to_string(), "HTTPS".to_string(), "MX".to_string(), "NAPTR".to_string(), "NS".to_string(), "PTR".to_string(), "SOA".to_string(), "SPF".to_string(), "SRV".to_string(), "SSHFP".to_string(), "SVCB".to_string(), "TLSA".to_string(), "TXT".to_string()],
                namespace: Some("aws.route53.record_set".to_string()),
                to_dsl: None,
            })
                .required()
                .identity()
                .with_description("The DNS record type. For information about different record types and how data is encoded for them, see Supported DNS Resource Record Types in the Ama...")
                .with_provider_name("Type"),
        )
        .attribute(
            AttributeSchema::new("hosted_zone_id", AttributeType::String)
                .create_only()
                .with_description("The ID of the hosted zone that contains this record set.")
                .with_provider_name("HostedZoneId"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("route53.record_set", &[("type", VALID_TYPE)])
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[]
}
