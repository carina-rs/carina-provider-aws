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
fn open_enum_cannot_hide_a_named_string_policy_in_its_base() {
    let base = AttributeType::enum_with_base(
        TypeIdentity::bare("Source"),
        AttributeType::string(),
        None,
        vec![],
        None,
        None,
    );
    let named = IdentityDefinition::string(
        TypeIdentity::bare("Target"),
        Some(PatternSpec::new("^a+$", PatternMatch::Full)),
        Some((Some(1), Some(8))),
        None,
        None,
    );
    assert!(
        matches!(
            try_with_member_identity(base, named),
            Err(MemberIdentityError::OpenEnumConstraints)
        ),
        "core ignores structural restrictions on an open Enum base"
    );
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
