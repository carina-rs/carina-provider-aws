use super::*;
use carina_core::schema::CompiledPattern;

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

#[test]
fn identity_662_round5_04_reference_pattern_is_a_portable_conjunct() {
    let (model, member) = text("^source$");
    let declarations = TypeDeclarations::default();
    let value = member_string_value(
        &model,
        "logs.LogGroup",
        &member,
        "KmsKeyId",
        "kms_key_id",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap();
    let RawShape::Reference {
        pattern: Some(source),
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("source pattern")
    };
    assert_eq!(source.as_str(), "^source$");
    let rules = declarations.input_patterns.borrow();
    assert_eq!(
        rules.get("kms_key_id").map(Vec::len),
        Some(1),
        "reference pattern must not be dropped or put in the source slot"
    );
    let matcher = CompiledPattern::try_new(&rules["kms_key_id"][0]).unwrap();
    assert!(!matcher.is_match("source"));
    assert!(
        matcher.is_match(
            "arn:aws:kms:us-east-1:123456789012:key/01234567-89ab-cdef-0123-456789abcdef"
        )
    );
}

#[test]
fn identity_662_round5_04_reference_pattern_errors_for_a_list_target() {
    let (model, member) = text("^source$");
    let path = "rules[].apply_server_side_encryption_by_default.kms_master_key_id";
    let error = member_string_value(
        &model,
        "s3.BucketServerSideEncryptionConfiguration",
        &member,
        "KmsMasterKeyId",
        path,
        &TypeDeclarations::default(),
        &HashMap::new(),
        None,
    )
    .err()
    .expect("unaddressable reference conjunction");
    let message = format!("{error:#}");
    assert!(
        message.contains(path) && message.contains("InputRule"),
        "{message}"
    );
}

#[test]
fn identity_662_round5_05_empty_closed_identity_intersection_is_a_codegen_error() {
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

fn evaluate(code: &str) {
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
let RawShape::Enum {{base,values:Some(values),..}}=ty.raw_shape() else {{panic!("closed enum")}};
assert!(matches!(base.raw_shape(),RawShape::String {{..}}));assert_eq!(values,["A"]);
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
fn identity_662_round5_06_local_enum_on_an_enum_compiles_and_evaluates() {
    let base=ValueTypeOverride {
        type_code:r#"AttributeType::enum_(carina_core::schema::enum_identity("Old", Some("aws.test")),Some(vec!["A".into(),"B".into()]),vec![],None,None)"#.into(),
        documentation:None,
        attribute_type:AttributeType::enum_(carina_core::schema::enum_identity("Old",Some("aws.test")),Some(vec!["A".into(),"B".into()]),vec![],None,None),
    };
    let result = string_members::decorate(
        base,
        &TypeOverride::LocalEnum {
            namespace: "aws.test",
            name: "New",
            values: &["A"],
        },
    )
    .unwrap();
    evaluate(&result.type_code);
}

#[test]
fn identity_662_round5_07_ecma_whitespace_is_exact() {
    let whitespace = [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}',
        '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
    ];
    for (source, inverted) in [
        (r"^\s$", false),
        (r"^\S$", true),
        (r"^[^\s]$", true),
        (r"^[\S]$", true),
    ] {
        let pattern = string_members::smithy_pattern("test.Space", "label", source).unwrap();
        let pattern = CompiledPattern::try_new(&pattern).unwrap();
        for c in whitespace {
            assert_eq!(
                pattern.is_match(&c.to_string()),
                !inverted,
                "{source}, U+{:04X}",
                c as u32
            );
        }
        for c in ['a', '\u{85}', '\u{200b}'] {
            assert_eq!(
                pattern.is_match(&c.to_string()),
                inverted,
                "{source}, U+{:04X}",
                c as u32
            );
        }
    }
}

#[test]
fn identity_662_round5_08_bad_allowed_values_are_errors() {
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
fn identity_662_round5_09_data_source_uses_its_declared_decoration() {
    let (model, _) = text(".*");
    let declared = DataSourceType::named(AwsType::IamRoleArn);
    let value = data_source_type(&model, "s3.Bucket", "arn", "Arn", &declared).unwrap();
    let RawShape::String {
        identity_layer:
            carina_core::schema::StringIdentityParts {
                identity: Some(identity),
                ..
            },
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("identity")
    };
    assert_eq!(*identity, AwsType::IamRoleArn.identity());
    for source in resource_defs::sts_data_sources() {
        let resource_defs::DataSourceShape::Single { output_attributes } = source.shape else {
            panic!("single")
        };
        assert_eq!(
            output_attributes
                .iter()
                .find(|a| a.name == "arn")
                .unwrap()
                .type_code
                .decoration,
            Some(TypeOverride::Identity(AwsType::StsCallerIdentityArn))
        );
    }
}

#[test]
fn identity_662_round5_09_decoded_representation_keeps_its_tightening() {
    let (model, _) = text(".*");
    let source = DataSourceType {
        representation: Representation::Int,
        decoration: Some(TypeOverride::IntRange { min: 4, max: 8 }),
    };
    let value = data_source_type(&model, "test.Decoded", "count", "Count", &source).unwrap();
    assert!(
        matches!(
            value.attribute_type.raw_shape(),
            RawShape::Int {
                member_layer: carina_core::schema::IntMemberParts {
                    range: Some((Some(4), Some(8)))
                },
                ..
            }
        ),
        "declared tightening must not be ignored"
    );
}

#[test]
fn identity_662_round5_08_named_subset_cannot_contain_a_non_smithy_value() {
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
fn identity_662_round5_10_negative_prefix_guard_is_not_acm_specific() {
    for (source, allowed, forbidden) in [
        (
            r"^(?!_)[A-Z_]{0,3}[A-Z]$",
            vec!["A", "AB", "A_A"],
            vec!["_A", "_", "AAAAA"],
        ),
        (
            r"^(?!x)[a-z]{1,4}$",
            vec!["a", "ax", "yyyy"],
            vec!["x", "xx", "abcde"],
        ),
        (
            r"^(?!x)[\w-]{1,4}$",
            vec!["a", "a-", "_a"],
            vec!["x", "é", "abcde"],
        ),
    ] {
        let pattern = string_members::smithy_pattern("test.Lookahead", "text", source)
            .expect("general negative-prefix translation");
        let matcher = CompiledPattern::try_new(&pattern).unwrap();
        for value in allowed {
            assert!(matcher.is_match(value), "{source}: {value}");
        }
        for value in forbidden {
            assert!(!matcher.is_match(value), "{source}: {value}");
        }
    }
}
