use super::*;
use carina_core::schema::{CompiledPattern, InputRuleScope};

#[test]
fn identity_662_round7_07_unused_bucket_resource_arn_helper_is_not_emitted() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("carina-provider-aws/tests/fixtures/smithy/s3.json");
    if !path.exists() {
        return;
    }
    let model = carina_smithy::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let resource = resource_defs::s3_resources()
        .into_iter()
        .find(|r| r.name == "s3.Bucket")
        .unwrap();
    let code = generate_resource(&resource, &model).unwrap();
    assert!(
        !code.contains("pub fn arn()"),
        "the resource helper has no repository callers; keep only the used data-source helper"
    );
}

#[test]
fn identity_662_round7_03_enum_aliases_are_part_of_the_shared_derived_value() {
    let model = tests::enum_fixture_model();
    let mut resource = tests::enum_fixture_resource();
    resource.enum_aliases.push(("type", "address_record", "A"));
    let declarations = TypeDeclarations::from_resource(&resource);
    let member = carina_smithy::ShapeRef {
        target: "com.amazonaws.test#RecordType".into(),
        traits: Default::default(),
    };
    let (value, _) = enum_member_value(
        &model,
        resource.name,
        &member,
        "Type",
        "type",
        "aws.iam.Role",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap()
    .unwrap();
    let RawShape::Enum {
        identity,
        dsl_aliases,
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("enum")
    };
    assert!(
        dsl_aliases.contains(&("A".into(), "address_record".into())),
        "{dsl_aliases:?}"
    );
    assert!(
        value.type_code.contains("address_record"),
        "{}",
        value.type_code
    );
    let rust = generate_resource(&resource, &model).unwrap();
    assert!(rust.contains("address_record"));
    let docs = generate_markdown_resource(
        &resource,
        &model,
        &tests::markdown_resource_example(resource.name),
    )
    .unwrap();
    assert!(
        docs.contains(&format!("{identity}.address_record")),
        "{docs}"
    );
    assert!(!docs.contains(&format!("{identity}.a`")), "{docs}");
}

#[test]
fn identity_662_round7_03_named_enum_presentation_survives_without_a_callback() {
    let model=carina_smithy::parse(r#"{"smithy":"2.0","shapes":{"test#Zone":{"type":"enum","members":{"GLOBAL":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"Z2FDTNDATAQYW2"}}}}}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "test#Zone".into(),
        traits: Default::default(),
    };
    let mut declarations = TypeDeclarations::default();
    declarations.members.insert(
        "Zone",
        TypeOverride::EnumIdentity(AwsType::CloudFrontHostedZoneId),
    );
    let (value, _) = enum_member_value(
        &model,
        "test.Enum",
        &member,
        "Zone",
        "zone",
        "aws.test",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap()
    .unwrap();
    let RawShape::Enum { dsl_aliases, .. } = value.attribute_type.raw_shape() else {
        panic!("enum")
    };
    assert_eq!(dsl_aliases, [("Z2FDTNDATAQYW2".into(), "global".into())]);
    assert!(value.type_code.contains("global"));
}

#[test]
fn identity_662_round7_03_local_enum_identity_is_shared_by_code_and_metadata() {
    let model = tests::enum_fixture_model();
    let member = carina_smithy::ShapeRef {
        target: "com.amazonaws.test#RecordType".into(),
        traits: Default::default(),
    };
    let mut declarations = TypeDeclarations::default();
    declarations.members.insert(
        "Type",
        TypeOverride::LocalEnum {
            namespace: "aws.custom",
            name: "RecordKind",
            values: &["A"],
        },
    );
    let (value, info) = enum_member_value(
        &model,
        "route53.RecordSet",
        &member,
        "Type",
        "type",
        "aws.route53.RecordSet",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap()
    .unwrap();
    let RawShape::Enum { identity, .. } = value.attribute_type.raw_shape() else {
        panic!("enum")
    };
    assert_eq!(
        *identity,
        carina_core::schema::enum_identity("RecordKind", Some("aws.custom"))
    );
    assert!(info.identity_override.is_none());
    assert!(
        value.type_code.contains("aws.custom") && value.type_code.contains("RecordKind"),
        "{}",
        value.type_code
    );
}

#[test]
fn identity_662_round7_04_ecma_class_punctuation_is_literal() {
    for (source, valid, invalid) in [
        ("^[a&&b]+$", vec!["a", "b", "&", "a&b"], vec!["c"]),
        ("^[a-z--]+$", vec!["a", "z", "-"], vec!["&"]),
        ("^[--a]+$", vec!["a", "A", "-", "["], vec!["b", "z"]),
        ("^[a~~b]+$", vec!["a", "b", "~"], vec!["c"]),
        ("^[a[b]+$", vec!["a", "b", "["], vec!["]"]),
    ] {
        let pattern = string_members::smithy_pattern("test.Classes", "text", source).unwrap();
        let matcher = CompiledPattern::try_new(&pattern).unwrap();
        for text in valid {
            assert!(matcher.is_match(text), "{source}: {text:?}");
        }
        for text in invalid {
            assert!(!matcher.is_match(text), "{source}: {text:?}");
        }
    }
}

#[test]
fn identity_662_round7_06_open_enum_send_classification_is_generic() {
    let value = ValueTypeOverride {
        type_code: "arbitrary_open_enum_expression()".into(),
        documentation: None,
        attribute_type: AttributeType::enum_(
            carina_core::schema::enum_identity("FutureIdentity", Some("aws.test")),
            None,
            vec![],
            None,
            None,
        ),
    };
    assert_eq!(send_field_type(&value), SendFieldType::Enum);
}

#[test]
fn identity_662_round7_08_closed_named_values_emit_one_final_list() {
    let model =
        carina_smithy::parse(r#"{"smithy":"2.0","shapes":{"test#Text":{"type":"string"}}}"#)
            .unwrap();
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value = string_members::derive(
        &model,
        &member,
        "test.Values",
        "mode",
        &[TypeOverride::NamedValues {
            identity: AwsType::SqsQueueDeduplicationScope,
            values: &["messageGroup"],
        }],
    )
    .unwrap();
    assert_eq!(
        value.type_code.matches("with_member_values(").count(),
        1,
        "{}",
        value.type_code
    );
    let RawShape::Enum {
        values: Some(values),
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("closed enum")
    };
    assert_eq!(values, ["messageGroup"]);
}

#[test]
fn identity_662_round7_10_decorated_data_source_reference_uses_member_seam() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let source = DataSourceType::named(AwsType::KmsKeyId);
    let result = data_source_type(&model, "logs.LogGroup", "kms_key_id", "KmsKeyId", &source);
    let error = result
        .err()
        .expect("reference pattern cannot attach to a provider output");
    let message = format!("{error:#}");
    assert!(
        message.contains("logs.LogGroup.kms_key_id") && message.contains("InputRule"),
        "{message}"
    );
}

#[test]
fn identity_662_round7_02_nested_input_targets_must_exist_and_be_user_authored() {
    let attr = tests::test_attr(
        "Options",
        "options",
        "super::sqs_redrive_policy()",
        false,
        false,
        false,
        None,
    );
    let patterns = BTreeMap::from([(
        "options.absent".into(),
        vec![PatternSpec::new("^x$", PatternMatch::Full)],
    )]);
    let error = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap_err();
    assert!(error.to_string().contains("test.Nested.options.absent"));
}

#[test]
fn identity_662_round7_02_nested_provider_populated_child_is_rejected() {
    use carina_core::schema::StructField;
    let mut attr = tests::test_attr(
        "Options",
        "options",
        "AttributeType::string()",
        false,
        false,
        false,
        None,
    );
    attr.attribute_type = AttributeType::struct_(
        "Options",
        vec![
            StructField::provider_populated(
                "child",
                AttributeType::struct_(
                    "Child",
                    vec![StructField::optional("label", AttributeType::string()).build()],
                ),
            )
            .build(),
        ],
    );
    let patterns = BTreeMap::from([(
        "options.child.label".into(),
        vec![PatternSpec::new("^x$", PatternMatch::Full)],
    )]);
    let error = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("test.Nested.options.child.label")
            && error.to_string().contains("provider-populated"),
        "{error}"
    );
}

#[test]
fn identity_662_round7_02_create_only_child_controls_the_whole_path_scope() {
    use carina_core::schema::StructField;
    let mut attr = tests::test_attr(
        "Options",
        "options",
        "AttributeType::string()",
        false,
        false,
        false,
        None,
    );
    let child = AttributeType::struct_(
        "Child",
        vec![StructField::optional("label", AttributeType::string()).build()],
    );
    attr.attribute_type = AttributeType::struct_(
        "Options",
        vec![
            StructField::optional("fixed", child.clone())
                .create_only()
                .build(),
            StructField::optional("mutable", child).build(),
        ],
    );
    let patterns = ["options.fixed.label", "options.mutable.label"]
        .into_iter()
        .map(|p| (p.into(), vec![PatternSpec::new("^x$", PatternMatch::Full)]))
        .collect();
    let rules = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap();
    assert_eq!(rules[0].scope(), InputRuleScope::CreateOnly);
    assert_eq!(rules[1].scope(), InputRuleScope::EveryWrite);
}
