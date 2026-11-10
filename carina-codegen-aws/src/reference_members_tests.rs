use super::*;

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
fn reference_pattern_is_a_portable_conjunct() {
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
    assert_eq!(source.as_str(), carina_aws_types::kms_key_arn_pattern());
    let rules = declarations.input_patterns.borrow();
    assert_eq!(
        rules
            .get(&carina_core::schema::InputPath::new("kms_key_id").unwrap())
            .map(Vec::len),
        Some(1),
        "reference pattern must not be dropped or put in the source slot"
    );
    let matcher = &rules[&carina_core::schema::InputPath::new("kms_key_id").unwrap()][0];
    assert!(matcher.is_match("source"));
    assert!(
        !matcher.is_match(
            "arn:aws:kms:us-east-1:123456789012:key/01234567-89ab-cdef-0123-456789abcdef"
        )
    );
}

#[test]
fn reference_pattern_errors_for_a_list_target() {
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
fn a_reference_without_a_second_pattern_is_safe_in_a_list() {
    let model = carina_smithy::parse(
        &serde_json::json!({"smithy":"2.0","shapes":{
            "test#Text":{"type":"string"}
        }})
        .to_string(),
    )
    .unwrap();
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value = member_string_value(
        &model,
        "s3.BucketServerSideEncryptionConfiguration",
        &member,
        "KmsMasterKeyId",
        "rules[].apply_server_side_encryption_by_default.kms_master_key_id",
        &TypeDeclarations::default(),
        &HashMap::new(),
        None,
        None,
    )
    .expect("the type itself enforces its grammar without an InputPath obligation");
    assert!(matches!(
        value.attribute_type.raw_shape(),
        RawShape::Reference {
            pattern: Some(_),
            ..
        }
    ));
}
