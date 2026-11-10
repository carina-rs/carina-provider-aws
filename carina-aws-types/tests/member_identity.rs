use carina_aws_types::{
    AwsType, IdentityDefinition, MemberIdentityError, try_with_member_identity,
};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeType, PatternMatch, PatternSpec, TypeIdentity, ValidatedConfigSchema,
};
use std::collections::BTreeMap;

fn admitted(ty: AttributeType) -> carina_core::schema::RootlessType {
    ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)]))
        .unwrap()
        .rootless_type("value")
        .unwrap()
}

#[test]
fn replacing_an_int_identity_cannot_discard_its_range() {
    let base =
        AttributeType::refined_int(Some(TypeIdentity::bare("Old")), Some((Some(1), Some(5))));
    let named = IdentityDefinition::int(TypeIdentity::bare("New"), None, None);
    let result = try_with_member_identity(base, named);
    assert!(
        matches!(result, Err(MemberIdentityError::IdentityConflict { .. })),
        "replacing Old with New must be a typed error"
    );
}

#[test]
fn replacing_a_string_identity_cannot_discard_its_pattern() {
    let base = AttributeType::refined_string(
        Some(TypeIdentity::bare("Old")),
        Some(PatternSpec::new("^old$", PatternMatch::Full)),
        Some((Some(3), Some(3))),
        None,
    );
    let named = IdentityDefinition::string(TypeIdentity::bare("New"), None, None, None, None);
    assert!(try_with_member_identity(base, named).is_err());
}

#[test]
fn open_enum_cannot_hide_slot_constraints_in_its_base() {
    let base = AttributeType::refined_string(
        None,
        Some(PatternSpec::new("^a+$", PatternMatch::Full)),
        None,
        None,
    );
    let named = IdentityDefinition::enum_(
        TypeIdentity::bare("Target"),
        None,
        None,
        None,
        vec![],
        None,
        None,
    );
    assert!(matches!(
        try_with_member_identity(base, named),
        Err(MemberIdentityError::OpenEnumConstraints)
    ));
}

#[test]
fn resource_id_accepts_digit_bearing_prefixes() {
    assert!(carina_aws_types::validate_aws_resource_id("ec2-abcdef12").is_ok());
    let value: Value = Value::Concrete(ConcreteValue::String("ec2-abcdef12".into()));
    admitted(AwsType::ResourceId.attribute_type())
        .validate(&value)
        .unwrap();
}

#[test]
fn arn_and_kms_syntax_is_owned_by_the_identity_pattern() {
    for row in [AwsType::Arn, AwsType::KmsKeyArn, AwsType::KmsKeyId] {
        assert!(
            !row.has_validator(),
            "{row:?} duplicates its structural grammar in a callback"
        );
    }
    let invalid: Value = Value::Concrete(ConcreteValue::String("arn:aws:kms::123:key/bad".into()));
    for ty in [
        AwsType::KmsKeyArn.attribute_type(),
        try_with_member_identity(AttributeType::string(), AwsType::KmsKeyArn.definition()).unwrap(),
    ] {
        assert_eq!(admitted(ty).validate_collect(&invalid).len(), 1);
    }
    for text in ["arn:aws:s3:", "arn:aws:s3::", "not-an-arn"] {
        let value: Value = Value::Concrete(ConcreteValue::String(text.into()));
        assert!(
            admitted(AwsType::Arn.attribute_type())
                .validate(&value)
                .is_err()
        );
    }
}

#[test]
fn integer_tightening_keeps_the_original_identity_range() {
    let base =
        AttributeType::refined_int(Some(TypeIdentity::bare("Count")), Some((Some(1), Some(5))));
    let ty = carina_aws_types::with_int_range(base, Some(0), Some(10));
    let schema = admitted(ty);
    for (n, valid) in [(0, false), (1, true), (5, true), (6, false)] {
        let value: Value = Value::Concrete(ConcreteValue::Int(n));
        assert_eq!(schema.validate(&value).is_ok(), valid);
    }
}

#[test]
fn reference_binding_rejects_a_different_existing_identity() {
    let base = AttributeType::refined_string(
        Some(TypeIdentity::bare("Old")),
        None,
        Some((Some(1), Some(5))),
        None,
    );
    let result =
        carina_aws_types::try_with_member_reference(base, AwsType::KmsKeyReference.definition());
    assert!(matches!(
        result,
        Err(MemberIdentityError::IdentityConflict { .. })
    ));
}

#[test]
fn handwritten_reference_keeps_its_grammar_without_an_input_rule() {
    let ty = carina_aws_types::with_member_reference(
        AttributeType::string(),
        AwsType::KmsKeyReference.definition(),
    );
    let schema = admitted(ty);
    for (text, valid) in [
        ("not-a-kms-key", false),
        ("alias/aws/s3", true),
        ("01234567-89ab-cdef-0123-456789abcdef", true),
    ] {
        let value: Value = Value::Concrete(ConcreteValue::String(text.into()));
        assert_eq!(schema.validate(&value).is_ok(), valid, "{text}");
    }
}

#[test]
fn competing_reference_patterns_are_an_error_instead_of_a_dropped_check() {
    let base = AttributeType::refined_string(
        None,
        Some(PatternSpec::new("^source$", PatternMatch::Full)),
        None,
        None,
    );
    assert!(
        carina_aws_types::try_with_member_reference(base, AwsType::KmsKeyReference.definition(),)
            .is_err()
    );
}

#[test]
fn same_identity_with_a_different_callback_is_a_conflicting_definition() {
    use carina_core::schema::legacy_validator;
    let id = TypeIdentity::bare("Checked");
    let first = legacy_validator(|_| Ok(()));
    let second = legacy_validator(|_| Err("different policy".into()));
    for (base, named) in [
        (
            IdentityDefinition::string(id.clone(), None, None, Some(first.clone()), None),
            IdentityDefinition::string(id.clone(), None, None, Some(second.clone()), None),
        ),
        (
            IdentityDefinition::int(id.clone(), Some((Some(1), Some(100))), Some(first)),
            IdentityDefinition::int(id.clone(), Some((Some(1), Some(100))), Some(second)),
        ),
    ] {
        assert!(matches!(
            try_with_member_identity(base.into_attribute_type(), named),
            Err(MemberIdentityError::ConflictingDefinition(_))
        ));
    }
    assert!(matches!(
        try_with_member_identity(
            IdentityDefinition::int(id.clone(), Some((Some(1), Some(5))), None)
                .into_attribute_type(),
            IdentityDefinition::int(id, Some((Some(1), Some(10))), None),
        ),
        Err(MemberIdentityError::ConflictingDefinition(_))
    ));
}

#[test]
fn attaching_an_identity_rejects_disjoint_member_bounds() {
    let number = AttributeType::refined_int(None, Some((Some(1), Some(100))));
    assert!(try_with_member_identity(number, AwsType::Ec2AmazonSideAsn.definition()).is_err());
    let text = AttributeType::refined_string(None, None, Some((Some(1), Some(3))), None);
    assert!(
        try_with_member_identity(text, AwsType::OrganizationsAccountEmail.definition()).is_err()
    );
}

#[test]
fn anonymous_integer_bounds_live_only_in_the_member_layer() {
    let ty = carina_aws_types::with_int_range(
        AttributeType::refined_int(None, Some((Some(1), Some(100)))),
        Some(0),
        Some(200),
    );
    let carina_core::schema::RawShape::Int {
        identity_layer,
        member_layer,
    } = ty.raw_shape()
    else {
        panic!("Int");
    };
    assert!(
        identity_layer.identity.is_none()
            && identity_layer.range.is_none()
            && identity_layer.validate.is_none()
    );
    assert_eq!(member_layer.range, Some((Some(1), Some(100))));
}

#[test]
fn independently_requested_catalog_definitions_remain_identical() {
    let row = AwsType::IamRoleArn;
    try_with_member_identity(row.attribute_type(), row.definition()).unwrap();
    let row = carina_aws_types::CoreIdentity::Email;
    try_with_member_identity(row.attribute_type(), row.definition()).unwrap();
}

#[test]
fn anonymous_shared_strings_publish_only_member_constraints() {
    for ty in [
        carina_aws_types::iam_path(),
        carina_aws_types::iam_path_prefix(),
        carina_aws_types::route53_dns_name(),
        carina_aws_types::elbv2_health_check_port(),
    ] {
        let carina_core::schema::RawShape::String {
            identity_layer,
            member_layer,
        } = ty.raw_shape()
        else {
            panic!("String")
        };
        assert!(
            identity_layer.identity.is_none()
                && identity_layer.pattern.is_none()
                && identity_layer.length.is_none()
        );
        assert!(member_layer.pattern.is_some());
    }
}

#[test]
fn references_cannot_absorb_slot_local_constraints() {
    for base in [
        AttributeType::refined_string(None, None, Some((Some(1), Some(256))), None),
        AttributeType::refined_string(
            None,
            Some(PatternSpec::new("^slot$", PatternMatch::Full)),
            None,
            None,
        ),
    ] {
        assert!(
            carina_aws_types::try_with_member_reference(
                base,
                AwsType::KmsKeyReference.definition()
            )
            .is_err()
        );
    }
    let no_grammar = IdentityDefinition::reference(
        carina_core::schema::TypeIdentity::bare("Reference"),
        None,
        None,
    );
    let base = AttributeType::refined_string(
        None,
        Some(PatternSpec::new("^slot$", PatternMatch::Full)),
        None,
        None,
    );
    assert!(carina_aws_types::try_with_member_reference(base, no_grammar).is_err());
}

#[test]
fn enum_identity_attachment_rejects_relabeling_and_conflicting_definitions() {
    let definition = |name: &str, values: &[&str], aliases: Vec<(String, String)>| {
        IdentityDefinition::enum_(
            TypeIdentity::bare(name),
            None,
            None,
            Some(values.iter().map(|v| (*v).into()).collect()),
            aliases,
            None,
            None,
        )
    };
    assert!(matches!(
        try_with_member_identity(
            definition("Old", &["A"], vec![]).into_attribute_type(),
            definition("New", &["A"], vec![])
        ),
        Err(MemberIdentityError::IdentityConflict { .. })
    ));
    for named in [
        definition("Same", &["A", "B"], vec![]),
        definition("Same", &["A"], vec![("A".into(), "first".into())]),
    ] {
        assert!(matches!(
            try_with_member_identity(
                definition("Same", &["A"], vec![]).into_attribute_type(),
                named
            ),
            Err(MemberIdentityError::ConflictingDefinition(_))
        ));
    }
    let named = definition("Same", &["A"], vec![]);
    try_with_member_identity(named.clone().into_attribute_type(), named).unwrap();
}

#[test]
fn an_enum_cannot_take_a_string_identity() {
    let base = AttributeType::enum_(
        TypeIdentity::bare("Source"),
        Some(vec!["A".into()]),
        vec![],
        None,
        None,
    );
    let named =
        IdentityDefinition::string(TypeIdentity::bare("StringIdentity"), None, None, None, None);
    assert!(try_with_member_identity(base, named).is_err());
}

#[test]
fn int_enum_constructors_share_the_catalog_callback() {
    let row = AwsType::LogsLogGroupRetentionInDays;
    let base =
        carina_aws_types::validated_int_enum(row.identity(), row.allowed_int_values().unwrap());
    try_with_member_identity(base, row.definition()).unwrap();
}

#[test]
fn enum_alias_targets_must_belong_to_the_checked_value_set() {
    let result = IdentityDefinition::try_enum(
        TypeIdentity::bare("Letters"),
        Some(PatternSpec::new("^A+$", PatternMatch::Full)),
        Some((Some(1), Some(2))),
        Some(vec!["A".into()]),
        vec![("B".into(), "bee".into())],
        None,
        None,
    );
    assert!(
        result.is_err(),
        "an alias must not introduce B outside the checked set"
    );
}

#[test]
fn narrowing_an_enum_preserves_an_explicitly_empty_alias_set() {
    let base = AttributeType::enum_(
        TypeIdentity::bare("Unaliased"),
        Some(vec!["API-VALUE".into()]),
        vec![],
        None,
        None,
    );
    let narrowed = carina_aws_types::with_member_values(base, &["API-VALUE"]);
    let carina_core::schema::RawShape::Enum { dsl_aliases, .. } = narrowed.raw_shape() else {
        panic!("Enum")
    };
    assert!(
        dsl_aliases.is_empty(),
        "narrowing must not synthesize presentation aliases"
    );
}
