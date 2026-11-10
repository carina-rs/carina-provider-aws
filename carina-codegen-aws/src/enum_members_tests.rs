use super::*;

#[test]
fn closed_model_values_check_every_added_pattern() {
    let model = carina_smithy::parse(
        &serde_json::json!({"smithy":"2.0","shapes":{
            "test#Mode":{"type":"enum","members":{
                "A":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"A"}},
                "B":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"B B"}}
            }}
        }})
        .to_string(),
    )
    .unwrap();
    let member = carina_smithy::ShapeRef {
        target: "test#Mode".into(),
        traits: Default::default(),
    };
    for rule in [
        TypeOverride::StringPattern(PatternSpec::new("^A$", PatternMatch::Full)),
        TypeOverride::RefinedString {
            pattern: PatternSpec::new("^A$", PatternMatch::Full),
            min_length: 1,
            max_length: 10,
        },
        TypeOverride::StringProfile(StringProfile::DnsName),
    ] {
        let mut declarations = TypeDeclarations::default();
        declarations.members.insert("Mode", rule);
        let error = enum_member_value(
            &model,
            "test.Patterns",
            &member,
            "Mode",
            "mode",
            "aws.test",
            &declarations,
            &HashMap::new(),
            None,
        )
        .err()
        .expect("B B must not bypass an added pattern");
        let message = format!("{error:#}");
        assert!(
            message.contains("test.Patterns.mode") && message.contains("B B"),
            "{message}"
        );
    }
}

#[test]
fn closed_string_values_check_added_patterns() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "smithy.api#String".into(),
        traits: Default::default(),
    };
    let error = string_members::derive(
        &model,
        &member,
        "test.Patterns",
        "mode",
        &[
            TypeOverride::StringPattern(PatternSpec::new("^A$", PatternMatch::Full)),
            TypeOverride::LocalEnum {
                namespace: "aws.test",
                name: "Mode",
                values: &["A", "B"],
            },
        ],
    )
    .err()
    .expect("a closed String value must satisfy the added pattern");
    let message = format!("{error:#}");
    assert!(
        message.contains("test.Patterns.mode") && message.contains('B'),
        "{message}"
    );
}

#[test]
fn named_string_values_check_all_added_patterns() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "smithy.api#String".into(),
        traits: Default::default(),
    };
    for identity in [AwsType::SqsQueueDeduplicationScope, AwsType::Region] {
        let (allowed, rejected): (&'static [&'static str], &str) = if identity == AwsType::Region {
            (&["us-east-1", "us-west-2"], "us-west-2")
        } else {
            (&["messageGroup", "queue"], "queue")
        };
        let error = string_members::derive(
            &model,
            &member,
            "test.Values",
            "mode",
            &[
                TypeOverride::StringPattern(PatternSpec::new(".*", PatternMatch::Full)),
                TypeOverride::StringPattern(PatternSpec::new(
                    "^(?:messageGroup|us-east-1)$",
                    PatternMatch::Full,
                )),
                TypeOverride::NamedValues {
                    identity,
                    values: allowed,
                },
            ],
        )
        .err()
        .expect("finite overrides must satisfy every added pattern");
        let message = format!("{error:#}");
        assert!(
            message.contains("test.Values.mode") && message.contains(rejected),
            "{message}"
        );
    }
}

fn text(source: &str) -> (SmithyModel, carina_smithy::ShapeRef) {
    let model = carina_smithy::parse(
        &serde_json::json!({"smithy":"2.0","shapes":{
            "test#Text":{"type":"string","traits":{"smithy.api#pattern":source}},
            "test#Mode":{"type":"enum","members":{
                "A":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"A"}},
                "B":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"B"}}
            }}
        }})
        .to_string(),
    )
    .unwrap();
    (
        model,
        carina_smithy::ShapeRef {
            target: "test#Text".into(),
            traits: Default::default(),
        },
    )
}

fn evaluate(code: &str, expected_aliases: &[(String, String)]) {
    use std::process::Command;
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let aws = super::test_support::linked_library("carina_aws_types");
    let core = super::test_support::linked_library("carina_core");
    let temp = super::test_support::build_tempdir();
    let source = temp.path().join("emitted.rs");
    let executable = temp.path().join("emitted");
    std::fs::write(&source,format!(r#"use carina_core::schema::{{AttributeType,RawShape}};
fn main() {{let ty={code};
let RawShape::Enum {{base,values:Some(values),dsl_aliases,..}}=ty.raw_shape() else {{panic!("closed enum")}};
assert!(matches!(base.raw_shape(),RawShape::String {{..}}));assert_eq!(values,["A"]);
assert_eq!(dsl_aliases.iter().map(|(a,b)|(a.as_str(),b.as_str())).collect::<Vec<_>>(), {expected_aliases:?});
carina_core::schema::ValidatedConfigSchema::validate(std::collections::BTreeMap::from([("value".into(),ty)])).unwrap();
}}"#)).unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2024", "-L"])
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("carina_core={}", core.display()))
        .arg("--extern")
        .arg(format!("carina_aws_types={}", aws.display()))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "emitted code must compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "emitted code must evaluate/admit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn empty_closed_identity_intersection_is_a_codegen_error() {
    let (model, member) = text("^different$");
    let error = string_members::derive(
        &model,
        &member,
        "test.Closed",
        "zone",
        &[TypeOverride::Identity(AwsType::CloudFrontHostedZoneId)],
    )
    .err()
    .expect("empty closed intersection");
    assert!(format!("{error:#}").contains("test.Closed.zone"));
}

#[test]
fn local_enum_cannot_relabel_an_identified_enum() {
    let base=ValueTypeOverride {
        type_code:r#"AttributeType::enum_(carina_core::schema::enum_identity("Old", Some("aws.test")),Some(vec!["A".into(),"B".into()]),vec![],None,None)"#.into(),
        documentation: None, validated_input_constraints: Vec::new(),
        attribute_type:AttributeType::enum_(carina_core::schema::enum_identity("Old",Some("aws.test")),Some(vec!["A".into(),"B".into()]),vec![],None,None),
    };
    let error = string_members::decorate(
        base,
        &TypeOverride::LocalEnum {
            namespace: "aws.test",
            name: "New",
            values: &["A"],
        },
    )
    .err()
    .expect("a local enum cannot replace Old");
    assert!(matches!(
        error.downcast_ref::<carina_aws_types::MemberIdentityError>(),
        Some(carina_aws_types::MemberIdentityError::IdentityConflict { .. })
    ));
}

#[test]
fn anonymous_local_enum_emission_compiles_and_evaluates() {
    let (model, member) = text("^A+$");
    let value = string_members::derive(
        &model,
        &member,
        "test.Enum",
        "mode",
        &[TypeOverride::LocalEnum {
            namespace: "aws.test",
            name: "Letters",
            values: &["A"],
        }],
    )
    .unwrap();
    let RawShape::Enum { dsl_aliases, .. } = value.attribute_type.raw_shape() else {
        panic!("Enum")
    };
    evaluate(&value.type_code, dsl_aliases);
}

#[test]
fn modeled_enum_values_must_not_disappear_during_derivation() {
    let (model, mut member) = text("^A$");
    member.target = "test#Mode".into();
    member
        .traits
        .insert("smithy.api#pattern".into(), serde_json::json!("^A$"));
    for named in [false, true] {
        let mut declarations = TypeDeclarations::default();
        if named {
            declarations
                .members
                .insert("Mode", TypeOverride::Identity(AwsType::DnsRecordType));
            member.traits.clear();
        }
        let error = enum_member_value(
            &model,
            "test.StrictEnum",
            &member,
            "Mode",
            "mode",
            "aws.test",
            &declarations,
            &HashMap::new(),
            None,
        )
        .err()
        .expect("B must cause an error rather than disappear");
        let message = format!("{error:#}");
        assert!(
            message.contains("test.StrictEnum.mode") && message.contains('B'),
            "{message}"
        );
    }
}

#[test]
fn enum_decoration_cannot_publish_a_string_identity_as_an_enum() {
    let base = ValueTypeOverride {
        type_code: "AttributeType::string()".into(),
        attribute_type: AttributeType::enum_(
            TypeIdentity::bare("Enum"),
            Some(vec!["A".into()]),
            vec![],
            None,
            None,
        ),
        documentation: None,
        validated_input_constraints: vec![],
    };
    assert!(string_members::decorate(base, &TypeOverride::Identity(AwsType::IamRoleName)).is_err());
}

#[test]
fn allowed_values_check_the_complete_enum_policy() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "smithy.api#String".into(),
        traits: Default::default(),
    };
    let error = string_members::derive(
        &model,
        &member,
        "test.Values",
        "region",
        &[TypeOverride::NamedValues {
            identity: AwsType::Region,
            values: &["not-a-region"],
        }],
    )
    .err()
    .expect("the named Enum's semantic policy must reject an invalid override value");
    let message = format!("{error:#}");
    assert!(
        message.contains("test.Values.region") && message.contains("not-a-region"),
        "{message}"
    );
}

#[test]
fn bad_allowed_values_are_errors() {
    let (model, mut member) = text("^A+$");
    let error = string_members::derive(
        &model,
        &member,
        "test.Values",
        "label",
        &[TypeOverride::LocalEnum {
            namespace: "aws.test",
            name: "Mode",
            values: &["A", "B"],
        }],
    )
    .err()
    .expect("B is rejected by the base");
    let message = format!("{error:#}");
    assert!(
        message.contains("test.Values.label") && message.contains('B'),
        "{message}"
    );
    member.target = "test#Mode".into();
    let mut declarations = TypeDeclarations::default();
    declarations.members.insert(
        "Mode",
        TypeOverride::NamedValues {
            identity: AwsType::DnsRecordType,
            values: &["A", "Z"],
        },
    );
    let error = enum_member_value(
        &model,
        "test.Values",
        &member,
        "Mode",
        "mode",
        "aws.test",
        &declarations,
        &HashMap::new(),
        None,
    )
    .err()
    .expect("Z absent from Smithy");
    let message = format!("{error:#}");
    assert!(
        message.contains("test.Values.mode") && message.contains('Z'),
        "{message}"
    );
}

#[test]
fn named_subset_cannot_contain_a_non_smithy_value() {
    let (model, mut member) = text(".*");
    member.target = "test#Mode".into();
    let mut declarations = TypeDeclarations::default();
    declarations.members.insert(
        "Mode",
        TypeOverride::NamedValues {
            identity: AwsType::DnsRecordType,
            values: &["A", "Z"],
        },
    );
    let error = enum_member_value(
        &model,
        "test.Values",
        &member,
        "Mode",
        "mode",
        "aws.test",
        &declarations,
        &HashMap::new(),
        None,
    )
    .err()
    .expect("Z absent from Smithy");
    let message = format!("{error:#}");
    assert!(
        message.contains("test.Values.mode") && message.contains('Z'),
        "{message}"
    );
}

#[test]
fn enum_aliases_are_part_of_the_shared_derived_value() {
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
fn named_enum_presentation_survives_without_a_callback() {
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
fn local_enum_identity_is_shared_by_code_and_metadata() {
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
fn open_enum_send_classification_is_generic() {
    let value = ValueTypeOverride {
        type_code: "arbitrary_open_enum_expression()".into(),
        documentation: None,
        validated_input_constraints: Vec::new(),
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
fn closed_named_values_emit_one_final_list() {
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
        value
            .type_code
            .matches("try_with_named_enum_values(")
            .count(),
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
