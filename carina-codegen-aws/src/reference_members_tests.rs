use super::*;

#[test]
fn identified_reference_slots_have_a_typed_classification_error() {
    let value = ValueTypeOverride {
        type_code: "carina_aws_types::kms_key_id()".into(),
        attribute_type: AwsType::KmsKeyId.attribute_type(),
        documentation: None,
        validated_input_constraints: vec![],
    };
    let error = member_reference_value(
        value,
        "test.Ref",
        "key",
        &TypeDeclarations::default(),
        AwsType::KmsKeyReference.definition(),
        "unused",
    )
    .err()
    .expect("an identified String cannot become a reference slot");
    assert!(
        format!("{error:#}").contains("reference slot cannot carry a String identity"),
        "{error:#}"
    );
    assert!(matches!(
        error.downcast_ref::<carina_aws_types::MemberIdentityError>(),
        Some(carina_aws_types::MemberIdentityError::ReferenceStringIdentity(_))
    ));
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
    assert_eq!(
        source.as_str(),
        carina_aws_types::kms_key_reference_pattern()
    );
    let rules = declarations.input_constraints.borrow();
    assert_eq!(
        rules
            .get(&carina_core::schema::InputPath::new("kms_key_id").unwrap())
            .map(Vec::len),
        Some(2),
        "reference pattern must not be dropped or put in the source slot"
    );
    let matcher = rules[&carina_core::schema::InputPath::new("kms_key_id").unwrap()]
        .iter()
        .find_map(|rule| match rule {
            SlotConstraint::Pattern(p) if p.as_str() == "^source$" => Some(p),
            _ => None,
        })
        .unwrap();
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

#[test]
fn reference_slot_lengths_are_input_rules_not_identity_bounds() {
    let (model, mut member) = text(".*");
    member.traits.insert(
        "smithy.api#length".into(),
        serde_json::json!({"min":1,"max":256}),
    );
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
        identity,
        pattern,
        length,
    } = value.attribute_type.raw_shape()
    else {
        panic!("Reference")
    };
    let canonical = AwsType::KmsKeyReference.attribute_type();
    let RawShape::Reference {
        identity: expected,
        pattern: grammar,
        length: bounds,
    } = canonical.raw_shape()
    else {
        panic!("Reference")
    };
    assert_eq!(
        (identity, pattern, length),
        (expected, grammar, bounds),
        "slot constraints cannot change a reference definition"
    );
}

#[test]
fn every_anonymous_reference_pattern_is_preserved() {
    let value = ValueTypeOverride {
        type_code: "AttributeType::string()".into(),
        documentation: None,
        validated_input_constraints: vec![],
        attribute_type: AttributeType::string_member(
            carina_core::schema::StringIdentityLayer::new(
                None,
                Some(PatternSpec::new("^a.*", PatternMatch::Full)),
                None,
                None,
            ),
            carina_core::schema::StringMemberConstraints::new(
                Some(PatternSpec::new(".*z$", PatternMatch::Full)),
                None,
            ),
        ),
    };
    let declarations = TypeDeclarations::default();
    member_reference_value(
        value,
        "test.Ref",
        "key",
        &declarations,
        AwsType::KmsKeyReference.definition(),
        "unused",
    )
    .unwrap();
    let input = carina_core::schema::InputPath::new("key").unwrap();
    let rules = declarations.input_constraints.borrow();
    assert_eq!(
        rules.get(&input).map(Vec::len),
        Some(2),
        "both anonymous patterns are slot obligations"
    );
}

#[test]
fn reference_slot_bounds_must_overlap_the_canonical_definition() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "smithy.api#String".into(),
        traits: std::collections::BTreeMap::from([(
            "smithy.api#length".into(),
            serde_json::json!({"min":3000,"max":4000}),
        )]),
    };
    let error = member_string_value(
        &model,
        "logs.LogGroup",
        &member,
        "KmsKeyId",
        "kms_key_id",
        &TypeDeclarations::default(),
        &HashMap::new(),
        None,
        None,
    )
    .err()
    .expect("a slot cannot require a value longer than its reference permits");
    assert!(format!("{error:#}").contains("logs.LogGroup.kms_key_id"));
}
