use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::schema::{
    AttributeSchema, AttributeType, ResourceSchema, SchemaRegistry, StructField,
};

fn validate_source(source: &str, registry: SchemaRegistry, expected_resources: usize) {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        registry,
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let parsed =
        carina_core::parser::parse_file(source, Path::new("s3-infra.crn"), context).unwrap();
    assert_eq!(
        parsed.resources.len() + parsed.data_sources.len(),
        expected_resources
    );
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".into()]),
        context,
    )
    .unwrap();
    let bindings = carina_core::binding_index::BindingIndex::from_parsed(
        &parsed,
        snapshot.schema_registry(),
        context,
    );
    carina_core::validation::validate_resource_ref_types(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::new(),
        &bindings,
    )
    .unwrap();
}

#[test]
fn s3_documented_cors_example_validates_with_loaded_schemas() {
    validate_source(
        include_str!("../../examples/s3_bucket_cors_configuration/main.crn"),
        SchemaRegistry::new(),
        2,
    );
}

#[test]
fn s3_infra_registry_exact_resource_shapes_and_bucket_references_validate() {
    // Exact usecases/registry/serve/s3.crn: omitted object-lock rule, all four
    // public-access booleans, AES256 with omitted KMS/key flag, lifecycle days=7.
    validate_source(
        &format!(
            "let environment = 'dev'\n{}",
            include_str!("fixtures/s3-infra/registry-s3.crn")
        ),
        SchemaRegistry::new(),
        7,
    );
}

#[test]
fn s3_infra_bootstrap_bucket_shape_validates() {
    validate_source(
        include_str!("fixtures/s3-infra/bootstrap-dev.crn"),
        SchemaRegistry::new(),
        1,
    );
}

#[test]
fn s3_infra_registry_bucket_policy_preserves_all_statements_and_quoted_conditions() {
    let source = format!(
        "let bucket = aws.s3.Bucket {{ bucket = 'carina-registry-dev' }}\nlet distribution = {{ arn = 'arn:aws:cloudfront::123456789012:distribution/EXAMPLE' }}\n{}",
        include_str!("fixtures/s3-infra/registry-policy.crn")
    );
    validate_source(&source, SchemaRegistry::new(), 2);
}

#[test]
fn s3_infra_management_policy_preserves_the_bucket_name_and_upstream_principal_shapes() {
    // The real source bucket is awscc, outside this provider. Supply its
    // String outputs as bindings; do not import or declare awscc identities.
    let source = format!(
        "let bucket = {{ bucket_name = 'carina-rs-state' arn = 'arn:aws:s3:::carina-rs-state' }}\nlet orgs = {{ accounts = {{ registry_dev = '111111111111' registry_prod = '222222222222' }} }}\n{}",
        include_str!("fixtures/s3-infra/state-policy.crn")
    );
    validate_source(&source, SchemaRegistry::new(), 1);
}

fn cloudfront_sink_fixture() -> SchemaRegistry {
    // AWS has no CloudFront Distribution schema on this branch. Project the
    // real Origin.DomainName String contract as a sink fixture, as the Route 53
    // slice does for absent source schemas. Load every actual AWS schema too.
    let mut registry = SchemaRegistry::new();
    registry.insert(
        "aws",
        ResourceSchema::new("cloudfront.Distribution")
            .attribute(
                AttributeSchema::required(
                    "distribution_config",
                    AttributeType::struct_(
                        "DistributionConfig",
                        vec![
                            StructField::required(
                                "origins",
                                AttributeType::list(AttributeType::struct_(
                                    "Origin",
                                    vec![
                                        StructField::required("id", AttributeType::string())
                                            .build(),
                                        StructField::required(
                                            "domain_name",
                                            AttributeType::string(),
                                        )
                                        .build(),
                                    ],
                                )),
                            )
                            .build(),
                        ],
                    ),
                )
                .build(),
            )
            .validate()
            .unwrap(),
    );
    registry
}

#[test]
fn s3_managed_and_read_outputs_flow_into_cloudfront_iam_route53_and_region_parameters() {
    for source in ["aws.s3.Bucket", "read aws.s3.Bucket"] {
        let source = format!(
            r#"
            let bucket = {source} {{ bucket = 'carina-registry-dev' }}
            fn in_region(region: aws.Region) {{ region }}
            let region = in_region(bucket.region)
            aws.cloudfront.Distribution {{
                distribution_config = {{
                    origins = [{{ id = 's3' domain_name = bucket.bucket_regional_domain_name }}]
                }}
            }}
            aws.cloudfront.Distribution {{
                distribution_config = {{
                    origins = [{{ id = 's3-global' domain_name = bucket.bucket_domain_name }}]
                }}
            }}
            aws.s3.BucketPolicy {{
                bucket = bucket.bucket
                policy = {{
                    version = aws.iam.PolicyDocument.Version.2012_10_17
                    statement {{ effect = allow action = 's3:ListBucket' resource = bucket.arn }}
                    statement {{ effect = allow action = 's3:GetObject' resource = "${{bucket.arn}}/*" }}
                }}
            }}
            aws.route53.RecordSet {{
                hosted_zone_id = 'Z123EXAMPLE'
                name = 'registry.example.com'
                type = a
                alias_target = {{ dns_name = bucket.bucket_regional_domain_name hosted_zone_id = bucket.hosted_zone_id evaluate_target_health = false }}
            }}
            aws.route53.RecordSet {{
                hosted_zone_id = 'Z123EXAMPLE'
                name = 'storage.example.com'
                type = cname
                ttl = 60
                resource_records = [bucket.bucket_domain_name]
            }}
        "#
        );
        validate_source(&source, cloudfront_sink_fixture(), 6);
    }
}
