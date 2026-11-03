use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{RawShape, SchemaRegistry, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::organizations::account::organizations_account_config;

fn organizations_account_schemas() -> [ValidatedResourceSchema; 2] {
    let native = organizations_account_config()
        .schema
        .validate()
        .expect("Account schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode Account schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode Account schema");
    [native, decoded]
}

#[test]
fn organizations_account_refinements_use_iam_role_identity_and_anonymous_account_name() {
    // tests/fixtures/smithy/organizations.json: CreateAccountRequest members
    // target RoleName (effectively 1..64) and CreateAccountName (1..50).
    // AccountName must also satisfy account.json's PutAccountNameRequest.
    for schema in organizations_account_schemas() {
        for (field, expected_identity, expected_pattern, maximum) in [
            (
                "role_name",
                Some("aws.iam.Role.Name"),
                r"^[A-Za-z0-9_+=,.@-]+\z",
                64,
            ),
            ("account_name", None, r"^[ -;=?-~]+\z", 50),
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
fn organizations_account_id_and_arn_use_shared_catalog_types_after_protocol_round_trip() {
    for schema in organizations_account_schemas() {
        for (field, row) in [
            ("id", carina_aws_types::AwsType::AccountId),
            ("arn", carina_aws_types::AwsType::OrganizationsAccountArn),
        ] {
            let expected = row.attribute_type();
            let RawShape::String {
                identity: expected_identity,
                pattern: expected_pattern,
                length: expected_length,
                ..
            } = expected.raw_shape()
            else {
                panic!("catalog row must be String");
            };
            let attribute = schema.effective_attribute(field).unwrap();
            assert!(attribute.is_read_only());
            let RawShape::String {
                identity,
                pattern,
                length,
                ..
            } = attribute.field_type().raw_shape()
            else {
                panic!("{field} must be a String");
            };
            assert_eq!(identity, expected_identity, "{field}");
            assert_eq!(pattern, expected_pattern, "{field}");
            assert_eq!(length, expected_length, "{field}");
        }
    }
}

#[test]
fn organizations_account_name_validates_create_and_rename_constraints() {
    for schema in organizations_account_schemas() {
        let name = schema.effective_attribute("account_name").unwrap();
        for valid in [
            "registry-prod".to_string(),
            "registry-dev".to_string(),
            "a".repeat(50),
        ]
        .into_iter()
        .chain(
            (b' '..=b'~')
                .filter(|&byte| byte != b'<' && byte != b'>')
                .map(|byte| char::from(byte).to_string()),
        ) {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(
                name.validate(&value).is_ok(),
                "valid account name: {valid:?}"
            );
        }
        for invalid in [
            "".to_string(),
            "a".repeat(51),
            "member<prod>".to_string(),
            "a>b".to_string(),
            "<".to_string(),
            ">".to_string(),
            "registry-prod\n".to_string(),
            "\nregistry-prod".to_string(),
            "has\ttab".to_string(),
            "has\rcarriage-return".to_string(),
            "\u{1f}name".to_string(),
            "name\u{7f}".to_string(),
            "café".to_string(),
            "日本語".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(
                name.validate(&value).is_err(),
                "invalid account name: {invalid:?}"
            );
        }
    }
}

#[test]
fn organizations_account_email_has_named_bounds_and_timestamp_stays_string() {
    for schema in organizations_account_schemas() {
        let email = schema.effective_attribute("email").unwrap();
        let RawShape::String {
            identity,
            pattern,
            length,
            ..
        } = email.field_type().raw_shape()
        else {
            panic!("email must use the shared Organizations Email type");
        };
        assert_eq!(
            identity.unwrap().to_string(),
            "aws.organizations.Account.Email"
        );
        assert!(pattern.is_none());
        assert_eq!(length, Some((Some(6), Some(64))));
        // Native schemas carry callbacks; the SDK bridge tests their dispatch
        // for protocol consumers, which receive identity and length metadata.
        for valid in [
            "user+carina-rs-registry-prod@example.com".to_string(),
            "user+carina-rs-registry-dev@example.com".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(email.validate(&value).is_ok(), "plus addressing: {valid}");
        }
        let RawShape::String {
            identity,
            pattern,
            length,
            ..
        } = schema.parts().attributes["joined_timestamp"]
            .attr_type()
            .raw_shape()
        else {
            panic!("joined_timestamp must remain a plain String pending real-AWS reads");
        };
        assert!(identity.is_none());
        assert!(pattern.is_none());
        assert!(length.is_none());
    }
}

#[test]
fn organizations_account_email_reference_types_require_annotation_or_matching_output() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (declarations, expression, expected_error) in [
        ("", "'user+carina-rs-registry-prod@example.com'", None),
        (
            "arguments { contact: aws.organizations.Account.Email }",
            "contact",
            None,
        ),
        (
            "arguments { contact: String }",
            "contact",
            Some("expected aws.organizations.Account.Email, got String"),
        ),
        (
            "let source = aws.organizations.Organization {}",
            "source.master_account_email",
            None,
        ),
        (
            r#"let source = aws.iam.Role {
                role_name = 'email-source'
                description = 'user@example.com'
                assume_role_policy_document = {
                    statement = [{
                        effect = aws.iam.PolicyDocument.Statement.Effect.allow
                        principal = { service = 'lambda.amazonaws.com' }
                        action = 'sts:AssumeRole'
                    }]
                }
            }"#,
            "source.description",
            Some("expected aws.organizations.Account.Email, got String"),
        ),
    ] {
        let source = format!(
            r#"
            {declarations}
            aws.organizations.Account {{
                account_name = 'registry-prod'
                email = {expression}
            }}
        "#
        );
        let parsed = carina_core::parser::parse_file(
            &source,
            Path::new("organizations-email-reference.crn"),
            context,
        )
        .expect("parse Organizations email reference fixture");
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        )
        .expect("literal validation must defer references");
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
        if let Some(expected) = expected_error {
            let error = result.expect_err("plain String must not satisfy Organizations Email");
            assert!(error.contains(expected), "{error}");
        } else {
            assert!(result.is_ok(), "{declarations}: {result:?}");
        }
    }
}

#[test]
fn organizations_account_reference_types_accept_iam_names_and_reject_plain_strings() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (field, expression, expected_error) in [
        ("role_name", "'OrganizationAccountAccessRole'", None),
        ("role_name", "source.role_name", None),
        (
            "role_name",
            "source.description",
            Some("expected aws.iam.Role.Name, got String"),
        ),
        ("account_name", "source.description", None),
    ] {
        let sink_fields = if field == "role_name" {
            format!("account_name = 'registry-prod'\nrole_name = {expression}")
        } else {
            format!("account_name = {expression}")
        };
        let source = format!(
            r#"
                let source = aws.iam.Role {{
                    role_name = 'OrganizationAccountAccessRole'
                    description = 'registry-prod'
                    assume_role_policy_document = {{
                        statement = [{{
                            effect = aws.iam.PolicyDocument.Statement.Effect.allow
                            principal = {{ service = 'lambda.amazonaws.com' }}
                            action = 'sts:AssumeRole'
                        }}]
                    }}
                }}
                aws.organizations.Account {{
                    {sink_fields}
                    email = 'user+carina-rs-registry-prod@example.com'
                }}
            "#
        );
        let parsed = carina_core::parser::parse_file(
            &source,
            Path::new("organizations-account-reference.crn"),
            context,
        )
        .expect("parse Organizations reference fixture");
        let result = carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        );
        assert!(result.is_ok(), "{field} = {expression}: {result:?}");
        // Deferred values skip literal validation; check declared reference types.
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
            let error = result.expect_err("plain String must not satisfy Role.Name");
            assert!(error.contains(expected_error), "{error}");
        } else {
            assert!(result.is_ok(), "{field} = {expression}: {result:?}");
        }
    }
}
