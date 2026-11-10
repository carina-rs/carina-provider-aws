use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{
    attribute_type_to_protocol, protocol_to_resource_schema, resource_schema_to_protocol,
};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{RawShape, SchemaRegistry, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::iam::role::iam_role_config;
use carina_provider_aws::schemas::generated::iam::roles::iam_roles_config;

fn iam_roles_schemas() -> [ValidatedResourceSchema; 2] {
    let native = iam_roles_config()
        .schema
        .validate()
        .expect("Roles schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode Roles schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode Roles schema");
    [native, decoded]
}

#[test]
fn iam_roles_names_element_refinement_is_byte_identical_to_role_name() {
    let role = iam_role_config().schema.validate().expect("Role schema");
    let expected =
        attribute_type_to_protocol(role.parts().attributes["role_name"].attr_type()).unwrap();
    for schema in iam_roles_schemas() {
        let RawShape::List {
            element_type,
            ordered,
            ..
        } = schema.parts().attributes["names"].attr_type().raw_shape()
        else {
            panic!("names must remain a List");
        };
        assert!(!ordered, "names remain unordered");
        let actual = attribute_type_to_protocol(element_type).unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
}

#[test]
fn iam_roles_names_validate_aws_reads_and_reject_aws_probe_invalid_names() {
    // Protocol callbacks are covered by the SDK bridge test in src/main.rs.
    let schema = iam_roles_config().schema.validate().expect("Roles schema");
    let names = schema.effective_attribute("names").unwrap();
    for (name, expected) in [
        ("carina-588-probe".to_string(), true),
        (
            "AWSReservedSSO_AdministratorAccess_0123456789abcdef".to_string(),
            true,
        ),
        ("AZaz09_+=,.@-".to_string(), true),
        ("a".repeat(64), true),
        ("".to_string(), false),
        ("bad name".to_string(), false),
        ("a".repeat(65), false),
        ("valid\n".to_string(), false),
        ("rôle".to_string(), false),
    ] {
        let value: Value = Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
            ConcreteValue::String(name.clone().into()),
        )]));
        assert_eq!(
            names.validate(&value).is_ok(),
            expected,
            "names[] = {name:?}"
        );
    }
}

#[test]
fn iam_roles_path_prefix_accepts_infra_prefixes_without_requiring_a_trailing_slash() {
    // carina-test-004, 2026-11-03: ListRoles rejects path_prefix="carina"
    // because it does not begin with '/'. pathPrefixType differs from pathType.
    for schema in iam_roles_schemas() {
        let RawShape::String {
            identity_layer,
            member_layer,
        } = schema.parts().attributes["path_prefix"]
            .attr_type()
            .raw_shape()
        else {
            panic!("path_prefix must remain a String");
        };
        let identity = identity_layer.identity;
        let pattern = member_layer.pattern.or(identity_layer.pattern);
        let length =
            carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length);
        assert!(identity.is_none());
        assert_eq!(
            pattern.map(carina_core::schema::PatternSpec::as_str),
            Some(r"^\u002F[\u0021-\u007F]*$")
        );
        assert_eq!(length, Some((Some(1), Some(512))));
        let prefix = schema.effective_attribute("path_prefix").unwrap();
        for valid in [
            "/".to_string(),
            "//".to_string(),
            "/carina".to_string(),
            "/no-trailing".to_string(),
            // pathPrefixType ends at U+007F; pathType ends at U+007E.
            "/\u{7f}".to_string(),
            "/aws-reserved/sso.amazonaws.com/".to_string(),
            "/aws-service-role/elasticloadbalancing.amazonaws.com/".to_string(),
            format!("/{}", "a".repeat(511)),
            format!("/{}", (b'!'..=b'~').map(char::from).collect::<String>()),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(prefix.validate(&value).is_ok(), "valid prefix: {valid:?}");
        }
        for invalid in [
            "".to_string(),
            "carina".to_string(),
            "prefix/valid".to_string(),
            "/has space".to_string(),
            "/valid\n".to_string(),
            "/tab\t".to_string(),
            "/\u{80}".to_string(),
            "/é".to_string(),
            format!("/{}", "a".repeat(512)),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(
                prefix.validate(&value).is_err(),
                "invalid prefix: {invalid:?}"
            );
        }
    }
}

fn validate_iam_roles_name_reference(
    reference: &str,
) -> Result<(), carina_core::validation::ResourceRefTypeValidationError> {
    let source = format!(
        r#"
        let roles = read aws.iam.Roles {{
            path_prefix = '/aws-reserved/sso.amazonaws.com/'
            name_regex = '^AWSReservedSSO_AdministratorAccess_[0-9a-f]{{16}}$'
        }}
        aws.iam.Role {{
            role_name = {reference}
            assume_role_policy_document = {{
                statement = [{{
                    effect = aws.iam.PolicyDocument.Statement.Effect.allow
                    principal = {{ service = 'lambda.amazonaws.com' }}
                    action = 'sts:AssumeRole'
                }}]
            }}
        }}
    "#
    );
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let parsed =
        carina_core::parser::parse_file(&source, Path::new("iam-roles-reference.crn"), context)
            .expect("parse Roles -> Role reference");
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".to_string()]),
        context,
        &mut Vec::new(),
    )
    .expect("reference fixture must pass non-reference validation");
    // Value validation skips deferred references; check their declared types too.
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
}

#[test]
fn iam_roles_names_reference_flows_into_role_name() {
    validate_iam_roles_name_reference("roles.names[0]")
        .expect("Roles.names[] must be assignable to Role.role_name");
}

#[test]
fn iam_roles_names_list_reference_is_rejected_for_role_name() {
    let error = validate_iam_roles_name_reference("roles.names")
        .expect_err("the reference type checker must reject List -> String");
    let error = match error {
        carina_core::validation::ResourceRefTypeValidationError::Validation(message) => message,
        carina_core::validation::ResourceRefTypeValidationError::AnalysisInvariant(error) => {
            panic!("reference fixture must retain parser provenance: {error}")
        }
    };
    assert!(error.contains("role_name"), "{error}");
    assert!(
        error.contains("List<aws.iam.Role.Name>") && error.contains("expected aws.iam.Role.Name"),
        "{error}"
    );
}
