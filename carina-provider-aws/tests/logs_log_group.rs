use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, RawShape, ResourceSchema, SchemaRegistry,
    ValidatedResourceSchema,
};
use carina_provider_aws::schemas::generated::logs::log_group::logs_log_group_config;

fn logs_schemas() -> [ValidatedResourceSchema; 2] {
    let native = logs_log_group_config()
        .schema
        .validate()
        .expect("LogGroup schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode LogGroup schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode LogGroup schema");
    [native, decoded]
}

fn awscc_log_group_name_fixture() -> ValidatedResourceSchema {
    // Structural identity from pinned awscc a1bed434, also recorded in the
    // codegen manifest. Its owner-local reserved-prefix callback is separate.
    ResourceSchema::new("logs.LogGroup")
        .attribute(
            AttributeSchema::optional(
                "log_group_name",
                AttributeType::refined_string(
                    Some(carina_aws_types::AwsType::LogsLogGroupName.identity()),
                    Some(carina_core::schema::PatternSpec::new(
                        r"^[.\-_/#A-Za-z0-9]{1,512}\z",
                        carina_core::schema::PatternMatch::Full,
                    )),
                    Some((Some(1), Some(512))),
                    None,
                ),
            )
            .create_only()
            .build(),
        )
        .validate()
        .expect("AWSCC name fixture schema")
}

struct AwsccLogGroupFixture;

impl carina_core::provider::ProviderFactory for AwsccLogGroupFixture {
    fn name(&self) -> &str {
        "awscc"
    }
    fn display_name(&self) -> &str {
        "Pinned AWSCC log-group schema fixture"
    }
    fn provider_config_attribute_types(&self) -> carina_core::schema::ValidatedConfigSchema {
        Default::default()
    }
    fn validate_config(
        &self,
        _: &carina_core::provider::CanonicalProviderConfigAttributes,
        _: &carina_core::provider::ProviderValidationInputs,
    ) -> carina_core::provider::ProviderResult<()> {
        Ok(())
    }
    fn create_provider(
        &self,
        _: &carina_core::provider::ProviderSession,
        _: carina_core::provider::InitializeOutcomeMint,
    ) -> carina_core::provider::BoxFuture<
        '_,
        carina_core::provider::ProviderResult<carina_core::provider::ProviderInitialization>,
    > {
        panic!("schema-only fixture must never create an AWS client")
    }
    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        vec![awscc_log_group_name_fixture()]
    }
    fn schema_witness(&self) -> carina_core::schema::ValidatedProviderSchemaWitness {
        carina_core::schema::ValidatedProviderSchemaWitness::new(
            self.schemas(),
            self.provider_config_attribute_types(),
            Default::default(),
            vec![],
        )
        .unwrap()
    }
}

#[test]
fn logs_log_group_name_refinement_equals_awscc_literal() {
    let awscc = awscc_log_group_name_fixture();
    for schema in logs_schemas() {
        for value in [
            "a".to_string(),
            "/aws/lambda/carina".into(),
            ".-_/#AZaz09".into(),
            "a".repeat(512),
            "".into(),
            "a".repeat(513),
            "has space".into(),
            "valid\n".into(),
            "日本語".into(),
        ] {
            let input: Value = Value::Concrete(ConcreteValue::String(value.clone().into()));
            assert_eq!(
                schema
                    .effective_attribute("log_group_name")
                    .unwrap()
                    .validate(&input)
                    .is_ok(),
                awscc
                    .effective_attribute("log_group_name")
                    .unwrap()
                    .validate(&input)
                    .is_ok(),
                "same language for {value:?}"
            );
        }
    }
}

#[test]
fn logs_log_group_name_typed_references_pass_in_both_provider_directions() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![
            Box::new(carina_provider_aws::AwsProviderFactory),
            Box::new(AwsccLogGroupFixture),
        ]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (source_provider, sink_provider) in [("awscc", "aws"), ("aws", "awscc")] {
        let source = format!(
            "let source = {source_provider}.logs.LogGroup {{ log_group_name = '/carina/source' }}\n\
             {sink_provider}.logs.LogGroup {{ log_group_name = source.log_group_name }}"
        );
        let parsed =
            carina_core::parser::parse_file(&source, Path::new("logs-cross-provider.crn"), context)
                .expect("parse cross-provider LogGroup reference");
        let result = carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string(), "awscc".to_string()]),
            context,
            &mut Vec::new(),
        );
        assert!(
            result.is_ok(),
            "{source_provider} -> {sink_provider}: {result:?}"
        );
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
        .expect("both owners must expose the same nominal log-group name");
    }
}

#[test]
fn logs_log_group_name_validates_length_and_whole_string_pattern() {
    let draft = logs_log_group_config().schema;
    let RawShape::String {
        identity_layer,
        member_layer,
    } = draft.attributes()["log_group_name"].attr_type().raw_shape()
    else {
        panic!("log_group_name must remain a String");
    };
    let identity = identity_layer.identity;
    let pattern = member_layer.pattern.or(identity_layer.pattern);
    let length = carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length);
    assert_eq!(
        identity.unwrap().to_string(),
        "aws.logs.LogGroup.LogGroupName"
    );
    assert_eq!(
        identity_layer.pattern.unwrap().as_str(),
        r"^[.\-_/#A-Za-z0-9]{1,512}\z"
    );
    assert_eq!(
        pattern.map(carina_core::schema::PatternSpec::as_str),
        Some(r"^[\.\-_/#A-Za-z0-9]+$")
    );
    assert_eq!(length, Some((Some(1), Some(512))));

    for schema in logs_schemas() {
        let name = schema.effective_attribute("log_group_name").unwrap();
        for valid in [
            "a".to_string(),
            ".-_/#AZaz09".to_string(),
            "/aws/lambda/carina-588".to_string(),
            "a".repeat(512),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
            assert!(name.validate(&value).is_ok(), "valid name: {valid:?}");
        }
        for invalid in [
            "".to_string(),
            "a".repeat(513),
            "has space".to_string(),
            "colon:name".to_string(),
            "!valid".to_string(),
            "valid!".to_string(),
            "valid\n".to_string(),
            "日本語".to_string(),
        ] {
            let value: Value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
            assert!(name.validate(&value).is_err(), "invalid name: {invalid:?}");
        }
    }
}

#[test]
fn logs_log_group_retention_accepts_only_documented_integers() {
    let allowed = [
        1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096, 1827, 2192, 2557,
        2922, 3288, 3653,
    ];
    let schema = logs_log_group_config()
        .schema
        .validate()
        .expect("LogGroup schema");
    let retention = schema.effective_attribute("retention_in_days").unwrap();
    // Check every gap, including the AWS-probed invalid value 2, plus the bounds.
    // Protocol schemas route this callback through the guest SDK dispatch;
    // logs_log_group_retention_witness_dispatches_through_the_sdk_bridge covers it.
    for days in (-1..=3654).chain([i64::MIN, i64::MAX]) {
        let value: Value = Value::Concrete(ConcreteValue::Int(days));
        assert_eq!(
            retention.validate(&value).is_ok(),
            allowed.contains(&days),
            "retention_in_days = {days}"
        );
    }
    for invalid in [
        ConcreteValue::String("30".into()),
        ConcreteValue::Float(carina_core::FiniteF64::new(30.0).unwrap()),
        ConcreteValue::Bool(true),
    ] {
        let value: Value = Value::Concrete(invalid);
        assert!(retention.validate(&value).is_err(), "{value:?}");
    }
    let invalid: Value = Value::Concrete(ConcreteValue::Int(2));
    let error = retention
        .validate(&invalid)
        .expect_err("2 is not an allowed retention")
        .to_string();
    assert!(error.contains(&format!("{allowed:?}")), "{error}");
}

#[test]
fn logs_log_group_retention_schema_reports_allowed_values() {
    let allowed: &[i64] = &[
        1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096, 1827, 2192, 2557,
        2922, 3288, 3653,
    ];
    for schema in logs_schemas() {
        let attr = &schema.parts().attributes["retention_in_days"];
        let RawShape::Int {
            identity_layer:
                carina_core::schema::IntIdentityParts {
                    identity: Some(identity),
                    range: None,
                    ..
                },
            ..
        } = attr.attr_type().raw_shape()
        else {
            panic!("retention must be an identity-bearing Int, matching AWSCC");
        };
        assert_eq!(identity.to_string(), "aws.logs.LogGroup.RetentionInDays");
        let row = carina_aws_types::AwsType::ALL
            .iter()
            .find(|row| row.identity() == *identity)
            .expect("schema identity must resolve to the shared catalog");
        assert_eq!(row.allowed_int_values(), Some(allowed));
    }
}

#[test]
fn logs_log_group_name_rejects_an_unrelated_typed_string_reference() {
    // #700: log-group names now require their own identity, including when the
    // referenced value is unknown and its literal spelling could be valid.
    let source = r#"
        let role = aws.iam.Role {
            role_name = 'log-name-source'
            assume_role_policy_document = {
                version = aws.iam.PolicyDocument.Version.2012_10_17
                statement = [{
                    effect = aws.iam.PolicyDocument.Statement.Effect.allow
                    principal = { service = 'lambda.amazonaws.com' }
                    action = 'sts:AssumeRole'
                }]
            }
        }
        aws.logs.LogGroup {
            log_group_name = role.role_id
            retention_in_days = 30
        }
    "#;
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let parsed = carina_core::parser::parse_file(source, Path::new("logs-typed-name.crn"), context)
        .expect("parse typed log group name fixture");
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".to_string()]),
        context,
        &mut Vec::new(),
    )
    .expect("resource inputs validate before reference types");
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
    let error = carina_core::validation::validate_resource_ref_types(
        configuration.judged(),
        &HashSet::new(),
        &bindings,
        &mut Vec::new(),
    )
    .expect_err("IAM Role.Id is not a log-group name");
    assert!(
        error.to_string().contains("aws.logs.LogGroup.LogGroupName"),
        "{error}"
    );
}

#[test]
fn logs_log_group_examples_and_acceptance_fixtures_validate() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for fixture in [
        "examples/logs_log_group/main.crn",
        "acceptance-tests/logs_log_group/basic.crn",
        "acceptance-tests/ec2_flow_log/basic.crn",
    ] {
        let path = root.join(fixture);
        let source = std::fs::read_to_string(&path).expect("read log group fixture");
        let parsed = carina_core::parser::parse_file(&source, &path, context)
            .unwrap_or_else(|error| panic!("{fixture}: {error}"));
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
            &mut Vec::new(),
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));
    }
}
