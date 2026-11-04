use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::provider::ProviderNormalizer;
use carina_core::resource::{ConcreteValue, DeferredValue, ProviderResource, Resource, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, EffectiveType, RawShape, ResourceSchema, SchemaRegistry,
    TypeAssignability, TypeIdentity, ValidatedResourceSchema,
};
use carina_provider_aws::AwsNormalizer;
use carina_provider_aws::schemas::generated::route53::record_set::route53_record_set_config;
use indexmap::IndexMap;

fn schemas() -> [ValidatedResourceSchema; 2] {
    let native = route53_record_set_config().schema.validate().unwrap();
    let wire = resource_schema_to_protocol(&native).unwrap();
    [native, protocol_to_resource_schema(&wire).unwrap()]
}

fn field<'a>(schema: &'a ValidatedResourceSchema, path: &str) -> EffectiveType<'a> {
    let names = path.split('.').map(str::to_string).collect::<Vec<_>>();
    schema.effective_type_at_attribute_path(&names).unwrap()
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn list(values: &[&str]) -> Value {
    Value::Concrete(ConcreteValue::List(
        values.iter().map(|value| string(value)).collect(),
    ))
}

fn record(record_type: &str, values: &[&str]) -> Resource {
    let schema = route53_record_set_config().schema.validate().unwrap();
    Resource::with_provider("aws", "route53.RecordSet", "record", None)
        .with_attribute("hosted_zone_id", string("Z123EXAMPLE"))
        .with_attribute("name", string("carina-rs.dev"))
        .with_attribute(
            "type",
            field(&schema, "type")
                .canonicalize_state(string(record_type))
                .into_present()
                .unwrap(),
        )
        .with_attribute("ttl", Value::Concrete(ConcreteValue::Int(300)))
        .with_attribute("resource_records", list(values))
}

fn dns_record(name: &str, alias: bool) -> Resource {
    let mut resource = record("A", &["185.199.108.153"]);
    if alias {
        resource.attributes.shift_remove("ttl");
        resource.attributes.shift_remove("resource_records");
        resource.set_attr(
            "alias_target",
            Value::Concrete(ConcreteValue::Map(IndexMap::from([
                ("dns_name".into(), string(name)),
                ("hosted_zone_id".into(), string("Z123EXAMPLE")),
                (
                    "evaluate_target_health".into(),
                    Value::Concrete(ConcreteValue::Bool(false)),
                ),
            ]))),
        );
    } else {
        resource.set_attr("name", string(name));
    }
    resource
}

async fn normalize(resource: Resource) -> Result<ProviderResource, String> {
    let mut resources = vec![resource.into()];
    AwsNormalizer
        .normalize_desired(&mut resources)
        .await
        .map_err(|e| e.to_string())?;
    Ok(resources.remove(0))
}

// DNSName's Smithy 0..1024 carrier does not express DNS syntax. The guide
// permits punctuation, underscores, and escaped octets, including label dots.
// https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/DomainNameFormat.html
#[tokio::test]
async fn route53_dns_fields_refine_grammar_without_introducing_nominal_sinks() {
    for schema in schemas() {
        for path in ["name", "alias_target.dns_name"] {
            let dns = field(&schema, path);
            let RawShape::String {
                identity,
                pattern,
                length,
                ..
            } = dns.as_type().raw_shape()
            else {
                panic!("DNS name String");
            };
            assert!(
                identity.is_none(),
                "keep ACM/CloudFront/ALB and module references assignable"
            );
            assert!(pattern.is_some());
            assert_eq!(length, Some((Some(1), Some(1024))));
            for valid in [
                "carina-rs.dev",
                "CARINA-RS.DEV.",
                "_token.carina-rs.dev.",
                "-label_.dev",
                "a",
                r"a\056b.dev",
                r"\000\040\377.dev",
                "a!b.dev",
                // Actual AWS spellings also pass the provider-local validator
                // below, for both DNS fields and both schema transports.
                r"\052.example.com.",
                r"a.\052.example.com.",
                r"\101bc.example.com.",
                "abc.example.com.",
                "_acme.example.com.",
                "d123.cloudfront.net.",
                "dualstack.name-123.ap-northeast-1.elb.amazonaws.com.",
            ] {
                assert!(dns.validate(&string(valid)).is_ok(), "{path}: {valid:?}");
                normalize(dns_record(valid, path == "alias_target.dns_name"))
                    .await
                    .unwrap();
            }
            for invalid in [
                "",
                ".",
                ".dev",
                "a..dev",
                "a.dev..",
                "a b.dev",
                "é.dev",
                "a.dev\n",
                r"a\999.dev",
                r"a\12.dev",
                r"a\400.dev",
                r"a\x.dev",
            ] {
                assert!(
                    dns.validate(&string(invalid)).is_err(),
                    "{path}: {invalid:?}"
                );
            }
            assert!(
                dns.validate(&string(&format!("{}.dev", r"\141".repeat(63))))
                    .is_ok()
            );
            assert!(
                dns.validate(&string(&format!("{}.dev", r"\141".repeat(64))))
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn route53_dns_names_enforce_label_and_wildcard_rules() {
    for name in [
        &format!("{}.dev", "a".repeat(63)),
        &format!("{}.dev", r"\141".repeat(63)),
        "*.example.com",
        r"\052.example.com",
        "prod.*.example.com",
        "prod.a*b.example.com",
        r"a\056b.example.com",
    ] {
        let resource = record("A", &["185.199.108.153"]).with_attribute("name", string(name));
        normalize(resource)
            .await
            .unwrap_or_else(|e| panic!("{name:?}: {e}"));
    }
    for name in [
        format!("{}.dev", "a".repeat(64)),
        format!("{}.dev", r"\141".repeat(64)),
        "*prod.example.com".into(),
        "prod*.example.com".into(),
        r"\052prod.example.com".into(),
        "bad..example.com".into(),
    ] {
        let error =
            normalize(record("A", &["185.199.108.153"]).with_attribute("name", string(&name)))
                .await
                .expect_err("invalid decoded name");
        assert!(error.contains("name"), "{error}");
        assert!(
            !error.contains(&name),
            "diagnostics must not disclose DNS input"
        );
    }
}

#[tokio::test]
async fn route53_aws_probe_dns_wire_length_matches_253_characters_without_root_dot() {
    // Each label is at most 63 bytes. Four label-length octets plus the final
    // zero root octet take the 250 label-data bytes up to 255 wire bytes.
    let longest = [
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61),
    ]
    .join(".");
    let escaped = [
        r"\141".repeat(63),
        r"\142".repeat(63),
        r"\143".repeat(63),
        r"\144".repeat(61),
    ]
    .join(".");
    assert_eq!(longest.len(), 253);
    for alias in [false, true] {
        for (name, valid) in [
            (longest.clone(), true),
            (format!("{longest}."), true),
            (format!("{longest}d"), false),
            (format!("{longest}d."), false),
            (escaped.clone(), true),
            (format!("{escaped}."), true),
            (format!("{escaped}{}", r"\144"), false),
            (format!("{escaped}{}.", r"\144"), false),
        ] {
            assert_eq!(
                normalize(dns_record(&name, alias)).await.is_ok(),
                valid,
                "alias={alias}, name={name}"
            );
        }
    }
}

#[tokio::test]
async fn route53_aws_probe_dns_canonicalization_preserves_label_octets_and_is_idempotent() {
    for (name, expected) in [
        (
            "T4.Carina-588-Probe2.Example",
            "t4.carina-588-probe2.example",
        ),
        (r"\101bc.Example.COM.", "abc.example.com"),
        (r"\052.Example.COM.", r"\052.example.com"),
        (r"A.\052.Example.COM.", r"a.\052.example.com"),
        (r"A\056B.Example.COM.", r"a\056b.example.com"),
        (r"A\134101b.Example.COM.", r"a\134101b.example.com"),
        (r"\000\377.Example.COM.", r"\000\377.example.com"),
        ("a!b.Example.COM.", r"a\041b.example.com"),
    ] {
        let normalized =
            normalize(record("A", &["185.199.108.153"]).with_attribute("name", string(name)))
                .await
                .unwrap();
        assert_eq!(
            normalized.get_attr("name"),
            Some(&string(expected)),
            "{name}"
        );
        let repeated = normalize(
            record("A", &["185.199.108.153"])
                .with_attribute("name", normalized.get_attr("name").unwrap().clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            repeated.get_attr("name"),
            normalized.get_attr("name"),
            "canonicalization must be idempotent"
        );
    }
}

// Real AWS (carina-test-004): all three values are accepted and returned
// verbatim, including adjacent chunks and the exact whitespace/quote escape.
#[tokio::test]
async fn route53_txt_aws_probe_accepts_exact_chunk_spellings() {
    for value in [r#""a\"b""#, r#""a"  "b""#, r#""a""b""#] {
        let normalized = normalize(record("TXT", &[value]))
            .await
            .unwrap_or_else(|error| panic!("{value:?}: {error}"));
        assert_eq!(
            normalized.get_attr("resource_records"),
            Some(&list(&[value]))
        );
    }
}

// ResourceRecord.Value -> RData max 4000; ResourceRecords min 1.
// https://docs.aws.amazon.com/Route53/latest/APIReference/API_ResourceRecord.html
#[test]
fn route53_record_values_are_bounded_strings_in_a_nonempty_list() {
    for schema in schemas() {
        let records = field(&schema, "resource_records");
        let item = records.list_element().unwrap();
        let RawShape::String {
            identity, length, ..
        } = item.as_type().raw_shape()
        else {
            panic!("String value");
        };
        assert!(identity.is_none());
        assert_eq!(length, Some((Some(0), Some(4000))));
        for (length, valid) in [(0, true), (4000, true), (4001, false)] {
            assert_eq!(item.validate(&string(&"x".repeat(length))).is_ok(), valid);
            assert_eq!(
                item.validate(&string(&"é".repeat(length))).is_ok(),
                valid,
                "carrier cap counts characters"
            );
        }
        assert!(records.validate(&list(&[])).is_err());
        assert!(records.validate(&list(&["185.199.108.153"])).is_ok());
    }
}

#[test]
fn route53_hosted_zone_id_reuses_the_existing_catalog_row() {
    for schema in schemas() {
        let id = field(&schema, "hosted_zone_id");
        let RawShape::String {
            identity: Some(identity),
            pattern,
            length,
            ..
        } = id.as_type().raw_shape()
        else {
            panic!("catalog hosted-zone ID");
        };
        assert_eq!(
            identity,
            &carina_aws_types::AwsType::Route53HostedZoneId.identity()
        );
        assert_eq!(pattern, Some("^Z[A-Z0-9]+$"));
        assert_eq!(length, Some((None, Some(32))));
        for value in ["Z1", "Z123EXAMPLE", &format!("Z{}", "A".repeat(31))] {
            assert!(id.validate(&string(value)).is_ok(), "{value}");
        }
        for value in [
            "",
            "123",
            "z123",
            "Zabc",
            "/hostedzone/Z123",
            &format!("Z{}", "A".repeat(32)),
        ] {
            assert!(id.validate(&string(value)).is_err(), "{value}");
        }
    }
}

// Cheap cross-field grammars are documented together here. Complex record
// formats retain the bounded RData carrier.
// https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/ResourceRecordTypes.html
#[tokio::test]
async fn route53_record_type_checks_reject_known_invalid_values_at_plan() {
    for (ty, values) in [
        ("A", vec!["999.1.2.3"]),
        ("A", vec!["2001:db8::1"]),
        ("AAAA", vec!["192.0.2.1"]),
        ("AAAA", vec!["2001:db8::g"]),
        ("CNAME", vec!["bad..example.com"]),
        ("CNAME", vec!["one.dev", "two.dev"]),
        ("NS", vec!["ns example.com"]),
        ("PTR", vec!["bad..dev"]),
        ("TXT", vec!["unquoted"]),
        ("TXT", vec!["\"unterminated"]),
        ("TXT", vec!["\"a\" garbage"]),
        ("TXT", vec!["\"bad\\999\""]),
        ("TXT", vec!["\"é\""]),
        (
            "SOA",
            vec![
                "ns.dev hostmaster.dev 1 2 3 4 5",
                "ns.dev hostmaster.dev 1 2 3 4 5",
            ],
        ),
    ] {
        let error = normalize(record(ty, &values))
            .await
            .expect_err("invalid typed RData");
        assert!(error.contains("resource_records"), "{ty}: {error}");
        assert!(
            !error.contains(values[0]),
            "do not echo input values: {error}"
        );
    }
    for ty in ["CNAME", "SOA", "A"] {
        assert!(normalize(record(ty, &[])).await.is_err(), "empty {ty}");
    }
    assert!(
        normalize(
            record("NS", &["ns.example.com"]).with_attribute("name", string("*.example.com"))
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn route53_record_type_checks_accept_infra_values_and_documented_txt_chunks() {
    for (ty, values) in [
        (
            "A",
            vec![
                "185.199.108.153",
                "185.199.109.153",
                "185.199.110.153",
                "185.199.111.153",
            ],
        ),
        (
            "AAAA",
            vec![
                "2606:50c0:8000::153",
                "2606:50c0:8001::153",
                "2606:50c0:8002::153",
                "2606:50c0:8003::153",
            ],
        ),
        ("CNAME", vec!["carina-rs.github.io"]),
        (
            "CNAME",
            vec!["_424c7224e9b0146f9a8808af955727d0.acm-validations.aws."],
        ),
        ("NS", vec!["ns-1.awsdns-00.com.", "_ns.example.com"]),
        ("PTR", vec!["_host.example.com."]),
        ("TXT", vec!["\"5b78b5973e386afa9e64155fe3541d\""]),
        (
            "TXT",
            vec![
                r#""String 1" "String 2" "String 3""#,
                r#""a\"b\\c\351""#,
                "\"\"",
            ],
        ),
        ("MX", vec!["10 mail.example.com"]),
        ("SRV", vec!["0 5 5060 sip.example.com"]),
        ("CAA", vec!["0 issue \"ca.example.net\""]),
    ] {
        normalize(record(ty, &values))
            .await
            .unwrap_or_else(|e| panic!("{ty}: {e}"));
    }
    for (chunk, valid) in [
        ("x".repeat(255), true),
        ("x".repeat(256), false),
        (r"\141".repeat(255), true),
        (r"\141".repeat(256), false),
    ] {
        let value = format!("\"{chunk}\"");
        assert_eq!(normalize(record("TXT", &[&value])).await.is_ok(), valid);
    }
    normalize(record(
        "TXT",
        &[&format!("\"{}\" \"{}\"", "x".repeat(255), "y".repeat(255))],
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn route53_aliases_accept_aws_endpoints_and_validate_decoded_dns_names() {
    for target in [
        "d123456.cloudfront.net",
        "name-123.ap-northeast-1.elb.amazonaws.com",
        "dualstack.name-123.ap-northeast-1.elb.amazonaws.com.",
        "s3-website.us-east-2.amazonaws.com",
        "_another.carina-rs.dev.",
    ] {
        let mut r = Resource::with_provider("aws", "route53.RecordSet", "alias", None);
        r.set_attr(
            "alias_target",
            Value::Concrete(ConcreteValue::Map(IndexMap::from([
                ("dns_name".into(), string(target)),
                ("hosted_zone_id".into(), string("Z2FDTNDATAQYW2")),
                (
                    "evaluate_target_health".into(),
                    Value::Concrete(ConcreteValue::Bool(false)),
                ),
            ]))),
        );
        normalize(r).await.unwrap();
    }
    let mut r = Resource::with_provider("aws", "route53.RecordSet", "alias", None);
    r.set_attr(
        "alias_target",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "dns_name".into(),
            string(&format!("{}.dev", r"\141".repeat(64))),
        )]))),
    );
    let error = normalize(r).await.unwrap_err();
    assert!(error.contains("alias_target.dns_name"), "{error}");
}

#[tokio::test]
async fn route53_unknown_and_deferred_leaves_remain_opaque() {
    for opaque in [
        Value::resource_ref("peer", "value", vec![]).resolve_root_for_test(),
        Value::Deferred(DeferredValue::Secret(Box::new(string("opaque")))),
    ] {
        normalize(record("A", &["not-yet-typed-rdata"]).with_attribute("type", opaque.clone()))
            .await
            .unwrap();
        for path in ["name", "type", "resource_records", "alias_target"] {
            let resource = record("TXT", &["\"valid\""]).with_attribute(path, opaque.clone());
            normalize(resource).await.unwrap();
        }
        let resource = record("CNAME", &["target.dev"]).with_attribute(
            "resource_records",
            Value::Concrete(ConcreteValue::List(vec![opaque.clone()])),
        );
        let normalized = normalize(resource).await.unwrap();
        assert_eq!(
            normalized.get_attr("resource_records"),
            Some(&Value::Concrete(ConcreteValue::List(vec![opaque])))
        );
    }
}

fn validate_source(source: &str, registry: SchemaRegistry) {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        registry,
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let parsed =
        carina_core::parser::parse_file(source, Path::new("route53-infra.crn"), context).unwrap();
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
fn route53_infra_acm_for_loop_preserves_underscore_names_and_record_values() {
    validate_source(
        r#"
        let cert = aws.acm.Certificate { domain_name = 'registry-dev.carina-rs.dev' validation_method = dns }
        let validation_records = for opt in cert.domain_validation_options {
            aws.route53.RecordSet {
                hosted_zone_id = 'Z123EXAMPLE'
                name = opt.resource_record.name
                type = cname
                ttl = 300
                resource_records = [opt.resource_record.value]
            }
        }
    "#,
        SchemaRegistry::new(),
    );
}

// This branch has no generated AWS HostedZone, CloudFront or ALB schemas.
// Source-only fixtures exercise AWS contracts, without assigning AWS identities
// to another provider's outputs or adding resources to the production catalog.
fn aws_source_fixtures() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    for data_source in [false, true] {
        let zone = ResourceSchema::new("route53.HostedZone")
            .attribute(
                AttributeSchema::provider_populated(
                    "id",
                    carina_aws_types::route53_hosted_zone_id(),
                )
                .build(),
            )
            .attribute(
                AttributeSchema::provider_populated(
                    "name_servers",
                    AttributeType::list(AttributeType::string()),
                )
                .build(),
            );
        let zone = if data_source {
            zone.as_data_source()
        } else {
            zone
        };
        registry.insert("aws", zone.validate().unwrap());
    }
    for (resource_type, fields) in [
        (
            "cloudfront.Distribution",
            vec![("domain_name", AttributeType::string())],
        ),
        (
            "elasticloadbalancingv2.LoadBalancer",
            vec![
                ("dns_name", AttributeType::string()),
                (
                    "canonical_hosted_zone_id",
                    carina_aws_types::route53_hosted_zone_id(),
                ),
            ],
        ),
    ] {
        let mut schema = ResourceSchema::new(resource_type);
        for (name, ty) in fields {
            schema = schema.attribute(AttributeSchema::provider_populated(name, ty).build());
        }
        registry.insert("aws", schema.validate().unwrap());
    }
    registry
}

#[test]
fn route53_aws_zone_outputs_and_data_source_ids_flow_into_all_infra_record_shapes() {
    for source in [
        "aws.route53.HostedZone {}",
        "read aws.route53.HostedZone {}",
    ] {
        // aws/management/route53/main.crn: exact GitHub Pages record bodies.
        validate_source(
            &format!(
                r#"
            let zone = {source}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = 'carina-rs.dev'
                type = a
                ttl = 300
                resource_records = ['185.199.108.153', '185.199.109.153', '185.199.110.153', '185.199.111.153']
            }}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = 'carina-rs.dev'
                type = aaaa
                ttl = 300
                resource_records = ['2606:50c0:8000::153', '2606:50c0:8001::153', '2606:50c0:8002::153', '2606:50c0:8003::153']
            }}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = 'www.carina-rs.dev'
                type = cname
                ttl = 300
                resource_records = ['carina-rs.github.io']
            }}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = '_github-pages-challenge-carina-rs.carina-rs.dev'
                type = txt
                ttl = 300
                resource_records = ['"5b78b5973e386afa9e64155fe3541d"']
            }}
        "#
            ),
            aws_source_fixtures(),
        );
    }
}

#[test]
fn route53_infra_ns_delegation_keeps_the_nameserver_list_reference() {
    validate_source(
        r#"
        let zone = aws.route53.HostedZone {}
        let delegation_ns = aws.route53.RecordSet {
            hosted_zone_id = 'Z123EXAMPLE'
            name = 'registry-dev.carina-rs.dev'
            type = ns
            ttl = 300
            resource_records = zone.name_servers
        }
    "#,
        aws_source_fixtures(),
    );
}

#[test]
fn route53_infra_cloudfront_and_alb_alias_output_references_still_type_check() {
    for record_type in ["a", "aaaa"] {
        validate_source(
            &format!(
                r#"
            let zone = aws.route53.HostedZone {{}}
            let distribution = aws.cloudfront.Distribution {{}}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = 'registry-dev.carina-rs.dev'
                type = {record_type}
                alias_target = {{
                    dns_name = distribution.domain_name
                    hosted_zone_id = aws.cloudfront.HostedZoneId.global
                    evaluate_target_health = false
                }}
            }}
            let alb = aws.elasticloadbalancingv2.LoadBalancer {{}}
            aws.route53.RecordSet {{
                hosted_zone_id = zone.id
                name = 'publish.registry-dev.carina-rs.dev'
                type = {record_type}
                alias_target = {{
                    dns_name = alb.dns_name
                    hosted_zone_id = alb.canonical_hosted_zone_id
                    evaluate_target_health = false
                }}
            }}
        "#
            ),
            aws_source_fixtures(),
        );
    }
}

#[test]
fn route53_hosted_zone_identity_does_not_admit_a_foreign_provider_identity() {
    let source = ResourceSchema::new("route53.HostedZone")
        .attribute(
            AttributeSchema::provider_populated(
                "id",
                AttributeType::refined_string(
                    Some(TypeIdentity::new(
                        Some("awscc"),
                        ["route53", "HostedZone"],
                        "Id",
                    )),
                    Some("^Z[A-Z0-9]+$".into()),
                    Some((None, Some(32))),
                    None,
                ),
            )
            .build(),
        )
        .validate()
        .unwrap();
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let sink = snapshot
        .schema_registry()
        .get(
            "aws",
            "route53.RecordSet",
            carina_core::schema::SchemaKind::Resource,
        )
        .unwrap();
    assert_eq!(
        source
            .as_schema()
            .type_in_schema(source.as_schema().attributes()["id"].attr_type())
            .assignment_to(sink.assignment_sink(
                sink.as_schema().attributes()["hosted_zone_id"].attr_type(),
                &snapshot.provider_context().custom_types
            )),
        TypeAssignability::Mismatch
    );
}

fn directory_module_diagnostics(
    domain_type: &str,
    zone_type: &str,
    zone_expression: &str,
) -> Vec<String> {
    use carina_core::binding_index::BindingIndex;
    use carina_core::config_loader::load_configuration_with_config;
    use carina_core::module_resolver::{
        load_module_from_directory, load_resolved_module_signatures, resolve_modules_with_config,
    };
    use carina_core::paths::ResolvedPath;
    use carina_core::validation::{
        deferred_types::TypeCheckSession, resolve_file_type_exprs, validate_resource_ref_types,
        validate_resources,
    };

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let module = temp.path().join("serve");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&module).unwrap();
    std::fs::write(
        module.join("main.crn"),
        format!("arguments {{ domain_name : {domain_type}\n parent_zone_id : {zone_type}\n }}\n"),
    )
    .unwrap();
    // Same bare domain argument and file separation as serve/route53.crn.
    std::fs::write(
        module.join("route53.crn"),
        r#"
        let delegation_ns = aws.route53.RecordSet {
            hosted_zone_id = ZONE_ID_EXPRESSION
            name = domain_name
            type = ns
            ttl = 300
            resource_records = ['ns-1.awsdns-00.com.']
        }
    "#
        .replace("ZONE_ID_EXPRESSION", zone_expression),
    )
    .unwrap();
    std::fs::write(root.join("main.crn"), "provider aws { region = aws.Region.ap_northeast_1 }\n let serve = use { source = '../serve' }\n let r = serve { domain_name = 'registry-dev.carina-rs.dev' parent_zone_id = 'Z123EXAMPLE' }\n").unwrap();
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let schemas = snapshot.schema_registry();
    let root = ResolvedPath::new(&root).unwrap();
    let module = ResolvedPath::new(&module).unwrap();
    let loaded = load_configuration_with_config(&root, context, schemas).unwrap();
    let mut template = load_module_from_directory(&module, context).unwrap();
    let mut deferred = Vec::new();
    let diagnostics = TypeCheckSession::run(context, |session| {
        assert!(
            resolve_file_type_exprs(&mut template, session)
                .unwrap()
                .is_empty()
        );
        let signatures = load_resolved_module_signatures(&template, &module, context);
        let bindings =
            BindingIndex::from_parsed_with_upstream_exports_and_module_signatures_in_session(
                &template,
                schemas,
                None,
                &signatures,
                session,
            );
        // Check the authored template before the caller's literal is substituted.
        let diagnostics =
            validate_resource_ref_types(&template, schemas, &HashSet::new(), &bindings)
                .err()
                .map(|e| e.to_string().lines().map(str::to_string).collect())
                .unwrap_or_default();
        let expanded = resolve_modules_with_config(loaded.parsed, &root, session, schemas).unwrap();
        validate_resources(&expanded, schemas, &HashSet::from(["aws".into()]), context).unwrap();
        let signatures = load_resolved_module_signatures(&expanded, &root, context);
        let bindings =
            BindingIndex::from_parsed_with_upstream_exports_and_module_signatures_in_session(
                &expanded,
                schemas,
                None,
                &signatures,
                session,
            );
        validate_resource_ref_types(&expanded, schemas, &HashSet::new(), &bindings).unwrap();
        diagnostics
    })
    .publish(&mut deferred);
    assert!(deferred.is_empty(), "{deferred:?}");
    diagnostics
}

#[test]
fn route53_directory_module_accepts_plain_string_and_acm_domain_annotations() {
    for domain_type in ["String", "aws.acm.Certificate.DomainName"] {
        assert!(
            directory_module_diagnostics(
                domain_type,
                "aws.route53.HostedZone.Id",
                "parent_zone_id"
            )
            .is_empty()
        );
        let diagnostics = directory_module_diagnostics(domain_type, "String", "parent_zone_id");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].contains(
                "cannot assign String to 'hosted_zone_id': expected aws.route53.HostedZone.Id"
            ),
            "{diagnostics:?}"
        );
        assert!(
            directory_module_diagnostics(domain_type, "String", r#""${parent_zone_id}""#)
                .is_empty()
        );
    }
}

#[test]
fn route53_infra_foreign_zone_ids_require_an_explicit_string_boundary() {
    // Exact record reference shapes from infra. Foreign IDs intentionally
    // retain their own provider identity; string interpolation makes a new
    // AWS input value which will be checked after its references resolve.
    for (zone_expression, alb_expression, valid) in [
        ("zone.id", "alb.canonical_hosted_zone_id", false),
        (
            r#""${zone.id}""#,
            r#""${alb.canonical_hosted_zone_id}""#,
            true,
        ),
    ] {
        let foreign_id = AttributeType::refined_string(
            Some(TypeIdentity::new(
                Some("awscc"),
                ["route53", "HostedZone"],
                "Id",
            )),
            Some("^Z[A-Z0-9]+$".into()),
            Some((None, Some(32))),
            None,
        );
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "awscc",
            ResourceSchema::new("route53.HostedZone")
                .attribute(AttributeSchema::required("name", AttributeType::string()).build())
                .attribute(AttributeSchema::provider_populated("id", foreign_id.clone()).build())
                .attribute(
                    AttributeSchema::provider_populated(
                        "name_servers",
                        AttributeType::list(AttributeType::string()),
                    )
                    .build(),
                )
                .validate()
                .unwrap(),
        );
        registry.insert(
            "awscc",
            ResourceSchema::new("cloudfront.Distribution")
                .attribute(
                    AttributeSchema::provider_populated("domain_name", AttributeType::string())
                        .build(),
                )
                .validate()
                .unwrap(),
        );
        registry.insert(
            "awscc",
            ResourceSchema::new("elasticloadbalancingv2.LoadBalancer")
                .attribute(
                    AttributeSchema::provider_populated("dns_name", AttributeType::string())
                        .build(),
                )
                .attribute(
                    AttributeSchema::provider_populated("canonical_hosted_zone_id", foreign_id)
                        .build(),
                )
                .validate()
                .unwrap(),
        );
        let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
            Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
            registry,
            carina_core::EnvironmentSnapshot::empty(),
        );
        let context = snapshot.provider_context();
        let source = r#"
            let zone = awscc.route53.HostedZone { name = 'carina-rs.dev' }
            let cert = aws.acm.Certificate { domain_name = 'registry-dev.carina-rs.dev' validation_method = dns }
            let validation_records = for opt in cert.domain_validation_options {
                aws.route53.RecordSet {
                    hosted_zone_id = ZONE_ID_EXPRESSION
                    name = opt.resource_record.name
                    type = cname
                    ttl = 300
                    resource_records = [opt.resource_record.value]
                }
            }
            aws.route53.RecordSet {
                hosted_zone_id = ZONE_ID_EXPRESSION
                name = 'registry-dev.carina-rs.dev'
                type = ns
                ttl = 300
                resource_records = zone.name_servers
            }
            let distribution = awscc.cloudfront.Distribution {}
            aws.route53.RecordSet {
                hosted_zone_id = ZONE_ID_EXPRESSION
                name = 'registry-dev.carina-rs.dev'
                type = a
                alias_target = {
                    dns_name = distribution.domain_name
                    hosted_zone_id = aws.cloudfront.HostedZoneId.global
                    evaluate_target_health = false
                }
            }
            let alb = awscc.elasticloadbalancingv2.LoadBalancer {}
            aws.route53.RecordSet {
                hosted_zone_id = ZONE_ID_EXPRESSION
                name = 'publish.registry-dev.carina-rs.dev'
                type = aaaa
                alias_target = {
                    dns_name = alb.dns_name
                    hosted_zone_id = ALB_ID_EXPRESSION
                    evaluate_target_health = false
                }
            }
        "#.replace("ZONE_ID_EXPRESSION", zone_expression).replace("ALB_ID_EXPRESSION", alb_expression);
        let parsed = carina_core::parser::parse_file(
            &source,
            Path::new("route53-foreign-infra.crn"),
            context,
        )
        .unwrap();
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".into(), "awscc".into()]),
            context,
        )
        .unwrap();
        let bindings = carina_core::binding_index::BindingIndex::from_parsed(
            &parsed,
            snapshot.schema_registry(),
            context,
        );
        let result = carina_core::validation::validate_resource_ref_types(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::new(),
            &bindings,
        );
        assert_eq!(result.is_ok(), valid, "{result:?}");
        if let Err(error) = result {
            assert!(
                error.to_string().contains("awscc.route53.HostedZone.Id"),
                "{error}"
            );
        }
    }
}

#[test]
fn route53_existing_enum_integer_boolean_and_alias_zone_union_stay_strict() {
    for schema in schemas() {
        let ty = field(&schema, "type");
        let RawShape::Enum {
            values: Some(values),
            ..
        } = ty.as_type().raw_shape()
        else {
            panic!("record type enum");
        };
        assert_eq!(values.len(), 17);
        let ttl = field(&schema, "ttl");
        for (value, valid) in [
            (-1, false),
            (0, true),
            (2147483647, true),
            (2147483648, false),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::Int(value));
            assert_eq!(ttl.validate(&value).is_ok(), valid);
        }
        let health = field(&schema, "alias_target.evaluate_target_health");
        let boolean: Value = Value::Concrete(ConcreteValue::Bool(false));
        assert!(health.validate(&boolean).is_ok());
        assert!(health.validate(&string("false")).is_err());
        let zone = field(&schema, "alias_target.hosted_zone_id");
        assert!(zone.validate(&string("Z123EXAMPLE")).is_ok());
        assert!(matches!(zone.as_type().raw_shape(), RawShape::Union(_)));
    }
}
