use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{EffectiveType, RawShape, SchemaRegistry, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::acm::certificate::acm_certificate_config;

fn acm_schemas() -> [ValidatedResourceSchema; 2] {
    let native = acm_certificate_config()
        .schema
        .validate()
        .expect("ACM schema");
    let wire = resource_schema_to_protocol(&native).expect("encode ACM schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode ACM schema");
    [native, decoded]
}

fn field<'a>(schema: &'a ValidatedResourceSchema, path: &str) -> EffectiveType<'a> {
    let names = path
        .split('.')
        .map(|name| name.trim_end_matches("[]").to_string())
        .collect::<Vec<_>>();
    let value_type = schema
        .effective_node_at_path(&names)
        .unwrap_or_else(|| panic!("missing {path}"))
        .value_type();
    if path.ends_with("[]") {
        value_type.list_element().unwrap()
    } else {
        value_type
    }
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

const READ_DOMAINS: &[&str] = &[
    "domain_validation_options[].domain_name",
    "domain_validation_options[].validation_domain",
    "renewal_summary.domain_validation_options[].domain_name",
    "renewal_summary.domain_validation_options[].validation_domain",
];

// acm.json DomainNameString and RequestCertificate's DomainName/SAN prose:
// https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html
// https://docs.aws.amazon.com/acm/latest/APIReference/API_DomainValidation.html
#[test]
fn acm_domain_name_enforces_the_request_common_name_octet_limit() {
    for schema in acm_schemas() {
        let domain = field(&schema, "domain_name");
        let RawShape::String {
            identity_layer,
            member_layer,
        } = domain.as_type().raw_shape()
        else {
            panic!("domain_name must remain a refined String");
        };
        let identity = identity_layer.identity;
        let pattern = member_layer.pattern.or(identity_layer.pattern);
        let length =
            carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length);
        assert_eq!(
            identity.unwrap().to_string(),
            "aws.acm.Certificate.DomainName"
        );
        assert!(pattern.is_some());
        assert_eq!(length, Some((Some(1), Some(64))));
        for (name, valid) in [
            (format!("{}.com", "a".repeat(60)), true),
            (format!("{}.com", "a".repeat(61)), false),
            (format!("*.{}.com", "a".repeat(58)), true),
            (format!("*.{}.com", "a".repeat(59)), false),
        ] {
            assert_eq!(domain.validate(&string(&name)).is_ok(), valid, "{name:?}");
        }
    }
}

#[test]
fn acm_domain_patterns_preserve_smithy_wildcards_labels_and_trailing_dot_rules() {
    for schema in acm_schemas() {
        for path in ["domain_name", "subject_alternative_names[]"]
            .into_iter()
            .chain(READ_DOMAINS.iter().copied())
        {
            let domain = field(&schema, path);
            for valid in [
                "a.aa",
                "a.00",
                "*.a.aa",
                "*.x.example.com",
                "a-b.Example.COM",
                "xn--bcher-kva.example",
            ] {
                assert!(domain.validate(&string(valid)).is_ok(), "{path}: {valid:?}");
            }
            for invalid in [
                "",
                "a",
                "a.a",
                "*.com",
                "*",
                ".example.com",
                "example.com.",
                "*.example.com.",
                "a..com",
                "-a.example.com",
                "a-.example.com",
                "example.-co",
                "example.co-",
                "_token.example.com",
                "a_b.example.com",
                "a.*.com",
                "*a.example.com",
                "*.*.example.com",
                "example.com\n",
                "é.example.com",
            ] {
                assert!(
                    domain.validate(&string(invalid)).is_err(),
                    "{path}: {invalid:?}"
                );
            }
        }
    }
}

#[test]
fn acm_request_domains_follow_documented_punycode_rules() {
    // Public request restrictions are narrower than read-side DomainNameString.
    // https://docs.aws.amazon.com/acm/latest/userguide/acm-certificate-characteristics.html
    for (schema, native) in acm_schemas().into_iter().zip([true, false]) {
        for path in ["domain_name", "subject_alternative_names[]"] {
            let domain = field(&schema, path);
            for valid in [
                "xn--r8jz45g.carina-rs.dev",
                "XN--R8JZ45G.CARINA-RS.DEV",
                "*.xn--r8jz45g.carina-rs.dev",
                "carina588-ok.carina-rs.dev",
                "a-b.example.com",
                "a--b.example.com",
                "a--example.com",
                "abc--example.com",
                "xn--xyz.com",
            ] {
                assert!(domain.validate(&string(valid)).is_ok(), "{path}: {valid}");
            }
            // Protocol callbacks run through SDK dispatch, covered by the
            // every-callback bridge test with these same real-AWS probes.
            if !native {
                continue;
            }
            for invalid in [
                "xn--carina588.carina-rs.dev",
                "ab--carina588.carina-rs.dev",
                "xn--example.com",
                "ab--example.com",
                "www.ab--example.com",
                "*.ab--example.com",
                "www.xn--carina588.carina-rs.dev",
            ] {
                assert!(
                    domain.validate(&string(invalid)).is_err(),
                    "{path}: {invalid}"
                );
            }
        }
        for path in READ_DOMAINS {
            for output in [
                "xn--carina588.carina-rs.dev",
                "ab--carina588.carina-rs.dev",
                "xn--example.com",
                "ab--example.com",
            ] {
                assert!(
                    field(&schema, path).validate(&string(output)).is_ok(),
                    "read output {path}: {output}"
                );
            }
        }
    }
}

#[test]
fn acm_sans_and_read_domains_allow_253_octets_and_63_octet_labels() {
    let longest = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    let wildcard_longest = format!(
        "*.{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(59)
    );
    for schema in acm_schemas() {
        for path in ["subject_alternative_names[]"]
            .into_iter()
            .chain(READ_DOMAINS.iter().copied())
        {
            let domain = field(&schema, path);
            let RawShape::String {
                identity_layer,
                member_layer,
            } = domain.as_type().raw_shape()
            else {
                panic!("{path} must be a refined String");
            };
            let identity = identity_layer.identity;
            let pattern = member_layer.pattern.or(identity_layer.pattern);
            let length =
                carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length);
            if path == "subject_alternative_names[]" {
                assert_eq!(
                    identity.unwrap().to_string(),
                    "aws.acm.Certificate.SubjectAlternativeName"
                );
            } else {
                assert!(
                    identity.is_none(),
                    "read-side domains must keep the broader Smithy refinement"
                );
            }
            assert!(pattern.is_some());
            assert_eq!(length, Some((Some(1), Some(253))));
            for valid in [
                &longest,
                &wildcard_longest,
                &format!("{}.com", "a".repeat(63)),
                &format!("a.{}", "a".repeat(63)),
            ] {
                assert!(domain.validate(&string(valid)).is_ok(), "{path}: {valid:?}");
            }
            for invalid in [
                format!("{longest}d"),
                format!("{wildcard_longest}d"),
                format!("{}.com", "a".repeat(64)),
                format!("a.{}", "a".repeat(64)),
            ] {
                assert!(
                    domain.validate(&string(&invalid)).is_err(),
                    "{path}: {invalid:?}"
                );
            }
        }
    }
}

// ResourceRecord has no numeric length trait. AWS's DNS-validation examples
// include underscores and terminal dots; they are not ordinary hostnames.
// https://docs.aws.amazon.com/acm/latest/userguide/dns-validation.html
#[test]
fn acm_validation_records_accept_dns_underscores_and_terminal_dots() {
    for schema in acm_schemas() {
        for prefix in [
            "domain_validation_options[]",
            "renewal_summary.domain_validation_options[]",
        ] {
            for leaf in ["name", "value"] {
                let path = format!("{prefix}.resource_record.{leaf}");
                let record = field(&schema, &path);
                for valid in [
                    "_a79865eb4cd1a6ab990a45779b4e0b96.example.com.",
                    "_424c7224e9b0146f9a8808af955727d0.acm-validations.aws.",
                    "_0123456789abcdef0123456789abcdef.x.example.com.",
                    "_0123456789abcdef0123456789abcdef.abc123.acm-validations.aws.",
                    "_token.example.com",
                    "a-b._token.example.com.",
                    // ACM records are read-only outputs and use the shared Route 53
                    // DNS-name type so they can be assigned to RecordSet.name /
                    // resource_records; wildcard rejection lived only on read validation.
                    // Both spellings are valid Route 53 names, which allow : and / in labels.
                    "*.example.com.",
                    "http://example.com/",
                ] {
                    assert!(record.validate(&string(valid)).is_ok(), "{path}: {valid:?}");
                }
                assert!(record.validate(&string("")).is_err());
                assert!(record.validate(&string(&"a".repeat(1025))).is_err());
                for formerly_rejected in [
                    "_token..example.com.",
                    ".example.com.",
                    "_token.example.com.\n",
                    "_token example.com.",
                ] {
                    assert!(
                        record.validate(&string(formerly_rejected)).is_ok(),
                        "approved anonymous-output pattern removal: {path}"
                    );
                }
                let RawShape::String {
                    identity_layer,
                    member_layer,
                } = record.as_type().raw_shape()
                else {
                    panic!("String")
                };
                assert!(
                    identity_layer.identity.is_none()
                        && identity_layer.pattern.is_none()
                        && member_layer.pattern.is_none()
                );
                assert_eq!(member_layer.length, Some((Some(1), Some(1024))));
            }
        }
    }
}

// HttpRedirect documents URLs only: no HTTPS-only rule or numeric length cap.
// https://docs.aws.amazon.com/acm/latest/APIReference/API_HttpRedirect.html
#[test]
fn acm_http_redirects_require_absolute_urls_without_forcing_https() {
    for (schema, native) in acm_schemas().into_iter().zip([true, false]) {
        for prefix in [
            "domain_validation_options[]",
            "renewal_summary.domain_validation_options[]",
        ] {
            for leaf in ["redirect_from", "redirect_to"] {
                let path = format!("{prefix}.http_redirect.{leaf}");
                let url = field(&schema, &path);
                let RawShape::String {
                    identity_layer:
                        carina_core::schema::StringIdentityParts {
                            identity: Some(identity),
                            length,
                            ..
                        },
                    ..
                } = url.as_type().raw_shape()
                else {
                    panic!("named validation URL String");
                };
                assert_eq!(identity.to_string(), "aws.acm.Certificate.ValidationUrl");
                assert!(length.is_none());
                // Protocol schemas carry the named callback witness; its
                // semantic dispatch is exercised by the SDK bridge test.
                if !native {
                    continue;
                }
                for valid in [
                    "http://example.com/.well-known/pki-validation/token.txt".to_string(),
                    "https://validation.example.com/token?x=1#fragment".to_string(),
                    "HTTP://EXAMPLE.COM/token".to_string(),
                    "http://example.com:0/token".to_string(),
                    "http://[2001:db8::1]:8080/token".to_string(),
                    "custom+scheme://example.com/token".to_string(),
                    format!("http://example.com/{}", "a".repeat(4096)),
                ] {
                    assert!(url.validate(&string(&valid)).is_ok(), "{path}: {valid:?}");
                }
                for invalid in [
                    "",
                    "/token",
                    "//example.com/token",
                    "example.com/token",
                    "http://",
                    "http:///token",
                    "https://?token",
                    "http://:80/token",
                    "https://[broken/token",
                    "https://example.com:invalid/token",
                    r"http://\example.com/token",
                    r"http://example.com\pki/token",
                    "http://example.com/%zz",
                    "http://user:pass@example.com:0/token",
                    "http://example.com/has space",
                    "http://example.com/token\n",
                    "http://example.com/\u{7f}",
                ] {
                    assert!(
                        url.validate(&string(invalid)).is_err(),
                        "{path}: {invalid:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn acm_certificate_arn_has_a_catalog_identity_and_certificate_resource_grammar() {
    for schema in acm_schemas() {
        let arn = field(&schema, "certificate_arn");
        let RawShape::String {
            identity_layer:
                carina_core::schema::StringIdentityParts {
                    identity: Some(identity),
                    ..
                },
            ..
        } = arn.as_type().raw_shape()
        else {
            panic!("ACM certificate ARN must have its own identity");
        };
        assert_eq!(identity.to_string(), "aws.acm.Certificate.Arn");
        assert!(
            carina_aws_types::AwsType::ALL
                .iter()
                .any(|row| row.identity() == *identity)
        );
        assert!(arn.validate(&string("arn:aws:acm:ap-northeast-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef")).is_ok());
        for partition in ["aws", "aws-cn", "aws-us-gov"] {
            let value = format!(
                "arn:{partition}:acm:us-east-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef"
            );
            assert!(arn.validate(&string(&value)).is_ok(), "{value}");
        }
        // #638: ACM ARN syntax accepts every partition in Partition::ALL.
        assert!(
            arn.validate(&string(
                "arn:aws-iso:acm:us-east-1:123456789012:certificate/id"
            ))
            .is_ok()
        );
        let prefix = "arn:aws:acm:us-east-1:123456789012:certificate/";
        for (length, valid) in [(2048, true), (2049, false)] {
            let value = format!("{prefix}{}", "a".repeat(length - prefix.len()));
            assert_eq!(
                arn.validate(&string(&value)).is_ok(),
                valid,
                "ARN length {length}"
            );
        }
        for invalid in [
            "arn:aws:sqs:us-east-1:123456789012:queue",
            "arn:aws:acm-pca:us-east-1:123456789012:certificate/id",
            "arn:aws:acm:us-east-1:123456789012:certificate-authority/id",
            "arn:aws:acm::123456789012:certificate/id",
            "arn:aws:acm:us-east-1:12345678901:certificate/id",
            "arn:aws:acm:us-east-1:1234567890123:certificate/id",
            "arn:aws:acm:us-east-1:123456789012:certificate/",
            "arn:aws:acm:us-east-1:123456789012:certificate/id/extra",
            "arn:aws:acm:us-east-1:123456789012:certificate/id\n",
        ] {
            assert!(arn.validate(&string(invalid)).is_err(), "{invalid:?}");
        }
    }
}

#[test]
fn acm_key_algorithm_allows_only_request_certificate_algorithms_and_aliases() {
    for schema in acm_schemas() {
        let algorithm = field(&schema, "key_algorithm");
        let RawShape::Enum {
            values: Some(values),
            ..
        } = algorithm.as_type().raw_shape()
        else {
            panic!("algorithm enum");
        };
        assert_eq!(values, ["EC_prime256v1", "EC_secp384r1", "RSA_2048"]);
        for wire in ["EC_prime256v1", "EC_secp384r1", "RSA_2048"] {
            let canonical = algorithm
                .canonicalize_state(string(wire))
                .into_present()
                .unwrap();
            assert!(algorithm.validate(&canonical).is_ok(), "SDK value {wire}");
        }
        for valid in [
            "ec_prime256v1",
            "ec_secp384r1",
            "rsa_2048",
            "aws.acm.Certificate.KeyAlgorithm.ec_prime256v1",
            "aws.acm.Certificate.KeyAlgorithm.ec_secp384r1",
            "aws.acm.Certificate.KeyAlgorithm.rsa_2048",
        ] {
            let value: Value = Value::Concrete(ConcreteValue::enum_identifier(valid));
            assert!(algorithm.validate(&value).is_ok(), "{valid}");
        }
        for invalid in [
            "RSA_1024",
            "RSA_3072",
            "RSA_4096",
            "EC_secp521r1",
            "aws.acm.Certificate.KeyAlgorithm.rsa_4096",
        ] {
            let value: Value = Value::Concrete(ConcreteValue::enum_identifier(invalid));
            assert!(algorithm.validate(&value).is_err(), "{invalid}");
        }
    }
}

fn validate_source(source: &str) {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let parsed = carina_core::parser::parse_file(source, Path::new("acm-infra.crn"), context)
        .expect("parse ACM infra fixture");
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".to_string()]),
        context,
        &mut Vec::new(),
    )
    .expect("validate ACM infra fixture");
    let bindings = carina_core::binding_index::BindingIndex::from_parsed(
        &parsed,
        snapshot.schema_registry(),
        context,
    );
    let configuration = carina_core::config_loader::load_buffer_configuration(
        &parsed,
        snapshot.schema_registry(),
        context,
    );
    carina_core::validation::validate_resource_ref_types(
        configuration.judged(),
        &HashSet::new(),
        &bindings,
        &mut Vec::new(),
    )
    .expect("validate ACM infra reference types");
}

#[test]
fn acm_infra_domains_and_for_loop_dns_reference_flow_validate() {
    // serve/acm.crn and publish/certificate_validation.crn use this exact shape.
    // This function call folds to a literal. The directory-module test below
    // separately checks declared argument types before literal substitution.
    for domain in [
        "registry-dev.carina-rs.dev",
        "registry.carina-rs.dev",
        "publish.registry-dev.carina-rs.dev",
        "publish.registry.carina-rs.dev",
    ] {
        validate_source(&format!(
            r#"
            fn domain_argument(value: String) {{ value }}
            let domain_name = domain_argument('{domain}')
            let cert = aws.acm.Certificate {{
                domain_name = domain_name
                validation_method = dns
            }}
            let validation_records = for opt in cert.domain_validation_options {{
                aws.route53.RecordSet {{
                    hosted_zone_id = 'Z123EXAMPLE'
                    name = opt.resource_record.name
                    type = cname
                    ttl = 300
                    resource_records = [opt.resource_record.value]
                }}
            }}
        "#
        ));
    }
}

#[test]
fn acm_named_certificate_arn_still_flows_into_generic_aws_arn_sinks() {
    validate_source(
        r#"
        let cert = aws.acm.Certificate { domain_name = 'example.com' validation_method = dns }
        fn accepts_arn(value: aws.Arn) { value }
        let arn = accepts_arn(cert.certificate_arn)
    "#,
    );
}

fn validate_directory_module_domain_argument(
    module_name: &str,
    argument_type: &str,
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

    let temp = tempfile::tempdir().expect("temporary ACM module fixture");
    let root = temp.path().join("root");
    let module = temp.path().join(module_name);
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&module).unwrap();
    std::fs::write(
        module.join("main.crn"),
        format!("arguments {{\n  domain_name : {argument_type}\n}}\n"),
    )
    .unwrap();

    // Relevant domain wiring from infra/usecases/registry/{serve,publish}.
    // Keep arguments and resources in separate files, and the shared bare
    // argument in both the named ACM sink and Route 53's identity-less sink.
    let (domain, files): (&str, &[(&str, &str)]) = match module_name {
        "serve" => (
            "registry-dev.carina-rs.dev",
            &[
                (
                    "acm.crn",
                    r#"let cert = aws.acm.Certificate {
  domain_name       = domain_name
  validation_method = dns
  directives { provider = us }
}
"#,
                ),
                (
                    "route53.crn",
                    r#"let delegation_ns = aws.route53.RecordSet {
  hosted_zone_id   = 'Z123EXAMPLE'
  name             = domain_name
  type             = ns
  ttl              = 300
  resource_records = ['ns-1.awsdns-00.com.']
}
"#,
                ),
            ],
        ),
        "publish" => (
            "publish.registry-dev.carina-rs.dev",
            &[
                (
                    "cloudfront.crn",
                    r#"let cloudfront_cert = aws.acm.Certificate {
  domain_name       = domain_name
  validation_method = dns
  directives { provider = us }
}
"#,
                ),
                (
                    "alb.crn",
                    r#"let cert = aws.acm.Certificate {
  domain_name       = domain_name
  validation_method = dns
}
aws.route53.RecordSet {
  hosted_zone_id = 'Z123EXAMPLE'
  name           = domain_name
  type           = a
  alias_target = {
    dns_name               = 'publish-alb.ap-northeast-1.elb.amazonaws.com'
    hosted_zone_id         = 'Z14GRHDCWA56QT'
    evaluate_target_health = false
  }
}
"#,
                ),
            ],
        ),
        _ => panic!("unknown ACM module fixture: {module_name}"),
    };
    for (name, source) in files {
        std::fs::write(module.join(name), source).unwrap();
    }
    std::fs::write(
        root.join("providers.crn"),
        "provider aws { region = aws.Region.ap_northeast_1 }\n\
         let us = provider aws { region = aws.Region.us_east_1 }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("main.crn"),
        format!(
            "let {module_name} = use {{ source = '../{module_name}' }}\n\
             let r = {module_name} {{ domain_name = '{domain}' }}\n"
        ),
    )
    .unwrap();

    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let schemas = snapshot.schema_registry();
    let root = ResolvedPath::new(&root).unwrap();
    let module = ResolvedPath::new(&module).unwrap();
    let loaded = load_configuration_with_config(&root, context, schemas).expect("load caller");
    let mut template = load_module_from_directory(&module, context).expect("load module files");
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
        // CLI wiring::validate_module_internal_ref_types checks this authored
        // template BEFORE expansion substitutes the caller's literal (#3798).
        let configuration = carina_core::config_loader::load_buffer_configuration(
            &template,
            schemas,
            session.catalog(),
        );
        let diagnostics = validate_resource_ref_types(
            configuration.judged(),
            &HashSet::new(),
            &bindings,
            &mut Vec::new(),
        )
        .err()
        .map(|error| {
            error
                .to_string()
                .lines()
                .map(|line| format!("../{module_name}: {line}"))
                .collect()
        })
        .unwrap_or_default();

        // Also exercise validate's directory loading / expansion / schema and
        // reference passes. Literal substitution alone cannot replace the
        // preceding module-definition check.
        let expanded = resolve_modules_with_config(loaded.parsed, &root, session, schemas)
            .expect("expand directory module with caller literal");
        validate_resources(
            &expanded,
            schemas,
            &HashSet::from(["aws".into()]),
            context,
            &mut Vec::new(),
        )
        .expect("validate expanded ACM and Route 53 resources");
        let signatures = load_resolved_module_signatures(&expanded, &root, context);
        let bindings =
            BindingIndex::from_parsed_with_upstream_exports_and_module_signatures_in_session(
                &expanded,
                schemas,
                None,
                &signatures,
                session,
            );
        let configuration = carina_core::config_loader::load_buffer_configuration(
            &expanded,
            schemas,
            session.catalog(),
        );
        validate_resource_ref_types(
            configuration.judged(),
            &HashSet::new(),
            &bindings,
            &mut Vec::new(),
        )
        .expect("validate expanded module references");
        diagnostics
    })
    .publish(&mut deferred);
    assert!(
        deferred.is_empty(),
        "loaded catalog must check every type: {deferred:?}"
    );
    diagnostics
}

#[test]
fn acm_directory_module_domain_argument_identity_matches_cli_validation() {
    for module_name in ["serve", "publish"] {
        let plain = validate_directory_module_domain_argument(module_name, "String");
        println!("{module_name}, domain_name : String\n{}", plain.join("\n"));
        let bindings = if module_name == "serve" {
            &["cert"][..]
        } else {
            &["cert", "cloudfront_cert"][..]
        };
        assert_eq!(
            plain,
            bindings
                .iter()
                .map(|binding| format!(
                    "../{module_name}: aws.acm.Certificate.{binding}: cannot assign String to 'domain_name': expected aws.acm.Certificate.DomainName, got String (from domain_name)"
                ))
                .collect::<Vec<_>>()
        );

        let annotated = validate_directory_module_domain_argument(
            module_name,
            "aws.acm.Certificate.DomainName",
        );
        assert!(
            annotated.is_empty(),
            "annotated ACM domain must also flow into Route 53: {annotated:#?}"
        );
    }
}
