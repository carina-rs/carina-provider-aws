use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::schema::{
    AttributeSchema, RawShape, ResourceSchema, SchemaRegistry, ValidatedResourceSchema,
};
use carina_provider_aws::schemas::generated::organizations::organization::organizations_organization_config;

fn organizations_organization_schemas() -> [ValidatedResourceSchema; 2] {
    let native = organizations_organization_config()
        .schema
        .validate()
        .expect("Organization schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode Organization schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode Organization schema");
    [native, decoded]
}

#[test]
fn organizations_organization_id_is_anonymous_and_validates_smithy_bounds() {
    for schema in organizations_organization_schemas() {
        let id = schema.effective_attribute("id").unwrap();
        let RawShape::String {
            identity,
            pattern,
            length,
            ..
        } = id.field_type().raw_shape()
        else {
            panic!("id must be String");
        };
        assert!(identity.is_none());
        assert_eq!(pattern, Some(r"^o-[a-z0-9]{10,32}\z"));
        assert_eq!(length, Some((Some(12), Some(34))));
        for valid in ["o-0123456789".to_string(), format!("o-{}", "a".repeat(32))] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(
                id.validate(&value).is_ok(),
                "valid organization id: {valid}"
            );
        }
        for invalid in [
            "".to_string(),
            "o-012345678".to_string(),
            format!("o-{}", "a".repeat(33)),
            "o-ABCDEFGHIJ".to_string(),
            "o-012345678_".to_string(),
            "o-0123456789\n".to_string(),
            "x-o-0123456789".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(
                id.validate(&value).is_err(),
                "invalid organization id: {invalid:?}"
            );
        }
    }
}

#[test]
fn organizations_organization_account_outputs_use_matching_catalog_identities() {
    for schema in organizations_organization_schemas() {
        for (field, row) in [
            (
                "arn",
                carina_aws_types::AwsType::OrganizationsOrganizationArn,
            ),
            (
                "master_account_arn",
                carina_aws_types::AwsType::OrganizationsAccountArn,
            ),
            ("master_account_id", carina_aws_types::AwsType::AccountId),
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
fn organizations_organization_output_reference_types_distinguish_accounts_from_organizations() {
    let mut registry = SchemaRegistry::new();
    registry.insert(
        "probe",
        ResourceSchema::new("organizations.Consumer")
            .attribute(
                AttributeSchema::optional("account_id", carina_aws_types::aws_account_id()).build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "account_arn",
                    carina_aws_types::organizations_account_arn(),
                )
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
    for (field, expression, expected_error) in [
        ("account_id", "account.id", None),
        ("account_id", "organization.master_account_id", None),
        ("account_arn", "account.arn", None),
        ("account_arn", "organization.master_account_arn", None),
        (
            "account_arn",
            "organization.arn",
            Some("expected aws.organizations.Account.Arn, got aws.organizations.Organization.Arn"),
        ),
    ] {
        let source = format!(
            r#"
            let account = aws.organizations.Account {{
                account_name = 'registry-prod'
                email = 'user+carina-rs-registry-prod@example.com'
            }}
            let organization = aws.organizations.Organization {{}}
            probe.organizations.Consumer {{ {field} = {expression} }}
        "#
        );
        let parsed = carina_core::parser::parse_file(
            &source,
            Path::new("organizations-identifiers.crn"),
            context,
        )
        .unwrap();
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string(), "probe".to_string()]),
            context,
        )
        .unwrap();
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
        let result = carina_core::validation::validate_resource_ref_types(
            configuration.judged(),
            &HashSet::new(),
            &bindings,
        );
        if let Some(expected) = expected_error {
            let error = result
                .expect_err("Organization ARN cannot satisfy Account ARN")
                .to_string();
            assert!(error.contains(expected), "{error}");
        } else {
            assert!(result.is_ok(), "{expression}: {result:?}");
        }
    }
}

#[test]
fn organizations_organization_email_uses_output_bounds_without_creation_only_restrictions() {
    for schema in organizations_organization_schemas() {
        let email = schema.effective_attribute("master_account_email").unwrap();
        let RawShape::String {
            identity,
            pattern,
            length,
            ..
        } = email.field_type().raw_shape()
        else {
            panic!("master_account_email must share Organizations Account Email");
        };
        assert_eq!(
            identity.unwrap().to_string(),
            "aws.organizations.Account.Email"
        );
        assert!(pattern.is_none());
        assert_eq!(length, Some((Some(6), Some(64))));
        // organizations.json: Organization$MasterAccountEmail -> Email, whose
        // pattern does not impose CreateAccountRequest$Email's prose rules.
        for valid in [
            "ab@c.d".to_string(),
            format!("{}@b.com", "a".repeat(58)),
            "user+carina-rs-registry-prod@example.com".to_string(),
            "user+carina-rs-registry-dev@example.com".to_string(),
            ".owner@example.com".to_string(),
            "owner@-example.com".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(email.validate(&value).is_ok(), "read email: {valid}");
        }
        for invalid in ["a@b.c".to_string(), format!("{}@b.com", "a".repeat(59))] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(
                email.validate(&value).is_err(),
                "out-of-bounds read: {invalid}"
            );
        }
    }
}
