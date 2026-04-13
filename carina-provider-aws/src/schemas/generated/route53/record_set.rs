//! record_set schema definition for AWS Route 53
//!
//! Hand-written: Route 53 RecordSet uses the ChangeResourceRecordSets API
//! which is not available via Cloud Control, so this is SDK-direct.

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};

/// Returns the schema config for route53.record_set
pub fn route53_record_set_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Route53::RecordSet",
        resource_type_name: "route53.record_set",
        has_tags: false,
        schema: ResourceSchema::new("aws.route53.record_set")
            .with_description(
                "A DNS record in a Route 53 hosted zone. \
                 Managed via ChangeResourceRecordSets (SDK-direct).",
            )
            .attribute(
                AttributeSchema::new("hosted_zone_id", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description("The ID of the hosted zone that contains this record set.")
                    .with_provider_name("HostedZoneId"),
            )
            .attribute(
                AttributeSchema::new("name", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description(
                        "The DNS name, e.g. 'example.com' or 'sub.example.com'. \
                         Route 53 appends a trailing dot automatically.",
                    )
                    .with_provider_name("Name"),
            )
            .attribute(
                AttributeSchema::new("type", AttributeType::String)
                    .required()
                    .identity()
                    .with_description(
                        "The DNS record type: A, AAAA, CNAME, MX, NS, PTR, SOA, SPF, SRV, TXT.",
                    )
                    .with_provider_name("Type"),
            )
            .attribute(
                AttributeSchema::new("ttl", AttributeType::Int)
                    .with_description("The time to live (TTL) in seconds.")
                    .with_provider_name("TTL"),
            )
            .attribute(
                AttributeSchema::new(
                    "resource_records",
                    AttributeType::list(AttributeType::String),
                )
                .with_description("The resource record values (e.g., IP addresses for A records).")
                .with_provider_name("ResourceRecords"),
            )
            .attribute(
                AttributeSchema::new(
                    "alias_target",
                    AttributeType::Struct {
                        name: "AliasTarget".to_string(),
                        fields: vec![
                            StructField::new("dns_name", AttributeType::String)
                                .required()
                                .with_provider_name("DNSName"),
                            StructField::new("hosted_zone_id", AttributeType::String)
                                .required()
                                .with_provider_name("HostedZoneId"),
                            StructField::new("evaluate_target_health", AttributeType::Bool)
                                .with_provider_name("EvaluateTargetHealth"),
                        ],
                    },
                )
                .with_description(
                    "Alias target for AWS resources (ELB, CloudFront, S3, etc.). \
                     Mutually exclusive with ttl and resource_records.",
                )
                .with_provider_name("AliasTarget"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("route53.record_set", &[])
}

/// Maps DSL alias values back to canonical AWS values for this module.
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[]
}
