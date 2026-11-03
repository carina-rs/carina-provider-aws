use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{RawShape, SchemaRegistry, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::iam::role::iam_role_config;

fn iam_role_schemas() -> [ValidatedResourceSchema; 2] {
    let native = iam_role_config()
        .schema
        .validate()
        .expect("Role schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode Role schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode Role schema");
    [native, decoded]
}

#[test]
fn iam_role_refinements_use_named_name_and_anonymous_path() {
    for schema in iam_role_schemas() {
        for (field, expected_identity, expected_pattern, maximum) in [
            (
                "role_name",
                Some("aws.iam.Role.Name"),
                r"^[A-Za-z0-9_+=,.@-]+\z",
                64,
            ),
            ("path", None, r"^(?:/|/[\x21-\x7E]+/)\z", 512),
        ] {
            let RawShape::String {
                identity,
                pattern,
                length,
                ..
            } = schema.parts().attributes[field].attr_type().raw_shape()
            else {
                panic!("{field} must remain a String");
            };
            assert_eq!(
                identity.map(ToString::to_string).as_deref(),
                expected_identity,
                "{field}"
            );
            assert_eq!(pattern, Some(expected_pattern), "{field}");
            assert_eq!(length, Some((Some(1), Some(maximum))), "{field}");
        }
    }
}

#[test]
fn iam_role_name_accepts_aws_reads_and_rejects_aws_probe_invalid_names() {
    // Protocol callbacks are covered by the SDK bridge test in src/main.rs.
    // carina-test-004, 2026-11-03: CreateRole/GetRole/ListRoles preserve RoleName;
    // "bad name" and a 65-character name are rejected by AWS.
    let schema = iam_role_config().schema.validate().expect("Role schema");
    let name = schema.effective_attribute("role_name").unwrap();
    for valid in [
        "a".to_string(),
        "AZaz09_+=,.@-".to_string(),
        "carina-588-probe".to_string(),
        "AWSReservedSSO_AdministratorAccess_0123456789abcdef".to_string(),
        "a".repeat(64),
    ] {
        let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
        assert!(name.validate(&value).is_ok(), "valid name: {valid:?}");
    }
    for invalid in [
        "".to_string(),
        "a".repeat(65),
        "bad name".to_string(),
        "!valid".to_string(),
        "valid!".to_string(),
        "valid\n".to_string(),
        "/role".to_string(),
        "rôle".to_string(),
        "日本語".to_string(),
    ] {
        let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
        assert!(name.validate(&value).is_err(), "invalid name: {invalid:?}");
    }
}

#[test]
fn iam_role_path_validates_smithy_whole_string_alternation_and_bounds() {
    // carina-test-004, 2026-11-03: CreateRole/GetRole/ListRoles return
    // /carina-588/probe/ verbatim; /no-trailing is rejected. '-' was accepted,
    // so pathType's U+0021..U+007E range takes precedence over error prose.
    for schema in iam_role_schemas() {
        let path = schema.effective_attribute("path").unwrap();
        for valid in [
            "/".to_string(),
            "///".to_string(),
            "/carina-588/probe/".to_string(),
            "/aws-reserved/sso.amazonaws.com/".to_string(),
            "/aws-service-role/elasticloadbalancing.amazonaws.com/".to_string(),
            format!("/{}/", "a".repeat(510)),
            format!("/{}/", (b'!'..=b'~').map(char::from).collect::<String>()),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(path.validate(&value).is_ok(), "valid path: {valid:?}");
        }
        for invalid in [
            "".to_string(),
            "//".to_string(),
            format!("/{}/", "a".repeat(511)),
            "/no-trailing".to_string(),
            "no-leading/".to_string(),
            "prefix/valid/".to_string(),
            "/valid/suffix".to_string(),
            "/has space/".to_string(),
            "/tab\t/".to_string(),
            "/valid/\n".to_string(),
            "/\u{7f}/".to_string(),
            "/é/".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(path.validate(&value).is_err(), "invalid path: {invalid:?}");
        }
    }
}

#[test]
fn iam_role_name_is_nominal_and_path_accepts_string_references() {
    // carina#3798 / aws#504: named sinks accept literals and matching named
    // references, but reject plain String references. Paths remain anonymous (#4230).
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (field, expression, plain_value, expected_error) in [
        (
            "role_name",
            "'carina-588-literal'",
            "carina-588-source",
            None,
        ),
        ("role_name", "source.role_name", "carina-588-source", None),
        (
            "role_name",
            "source.description",
            "carina-588-source",
            Some("expected aws.iam.Role.Name, got String"),
        ),
        ("path", "source.description", "/carina-588/probe/", None),
        ("path", "source.role_id", "/carina-588/probe/", None),
    ] {
        let sink_fields = if field == "role_name" {
            format!("role_name = {expression}")
        } else {
            format!("role_name = 'carina-588-sink'\npath = {expression}")
        };
        let source = format!(
            r#"
                let source = aws.iam.Role {{
                    role_name = 'carina-588-source'
                    description = '{plain_value}'
                    assume_role_policy_document = {{
                        statement = [{{
                            effect = aws.iam.PolicyDocument.Statement.Effect.allow
                            principal = {{ service = 'lambda.amazonaws.com' }}
                            action = 'sts:AssumeRole'
                        }}]
                    }}
                }}
                aws.iam.Role {{
                    {sink_fields}
                    assume_role_policy_document = source.assume_role_policy_document
                }}
                "#
        );
        let parsed =
            carina_core::parser::parse_file(&source, Path::new("iam-role-reference.crn"), context)
                .expect("parse IAM reference fixture");
        let result = carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        );
        assert!(result.is_ok(), "{field} = {expression}: {result:?}");
        // Value validation skips deferred references; check their declared types too.
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
        if let Some(expected_error) = expected_error {
            let error = result.expect_err("plain String references must not satisfy Role.Name");
            assert!(error.contains(expected_error), "{error}");
        } else {
            assert!(result.is_ok(), "{field} = {expression}: {result:?}");
        }
    }
}

#[test]
fn iam_role_examples_and_acceptance_fixtures_validate() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for fixture in [
        "examples/iam_role/main.crn",
        "examples/iam_roles_data_source/main.crn",
        "examples/s3_bucket_replication_configuration/main.crn",
        "acceptance-tests/iam_role/basic.crn",
        "acceptance-tests/iam_roles/basic.crn",
        "acceptance-tests/ec2_flow_log/basic.crn",
        "acceptance-tests/sts_caller_identity/basic.crn",
        "acceptance-tests/s3_bucket_replication_configuration/basic.crn",
        "acceptance-tests/s3_bucket_replication_configuration/with_filter.crn",
    ] {
        let path = root.join(fixture);
        let source = std::fs::read_to_string(&path).expect("read IAM fixture");
        let parsed = carina_core::parser::parse_file(&source, &path, context)
            .unwrap_or_else(|error| panic!("{fixture}: {error}"));
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));
    }
}
