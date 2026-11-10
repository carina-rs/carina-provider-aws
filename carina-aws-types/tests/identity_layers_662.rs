use carina_aws_types::{AwsType, with_member_identity};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, PatternMatch, PatternSpec, RawShape, ResourceSchema,
    StringIdentityLayer, StringMemberConstraints, ValidatedProviderSchemaWitness,
};

#[test]
fn identity_662_round9_standalone_constraints_are_identity_metadata() {
    let ty = AwsType::VpcId.attribute_type();
    let RawShape::String {
        identity_layer,
        member_layer,
    } = ty.raw_shape()
    else {
        panic!("String")
    };
    assert_eq!(
        identity_layer.pattern.unwrap().as_str(),
        "^vpc-[0-9a-f]{8,}$"
    );
    assert!(member_layer.pattern.is_none() && member_layer.length.is_none());
    let zone = AwsType::Route53HostedZoneId.attribute_type();
    let RawShape::String { identity_layer, .. } = zone.raw_shape() else {
        panic!("String")
    };
    assert_eq!(identity_layer.length, Some((None, Some(32))));
    assert!(
        identity_layer.validate.is_none(),
        "structural checks need no callback"
    );
}

#[test]
fn identity_662_round9_members_preserve_one_identity_and_separate_smithy_layers() {
    let member = |min| {
        with_member_identity(
            AttributeType::string_member(
                StringIdentityLayer::new(None, None, None, None),
                StringMemberConstraints::new(
                    Some(PatternSpec::new(".*12", PatternMatch::Full)),
                    Some((Some(min), Some(32))),
                ),
            ),
            AwsType::VpcId.attribute_type(),
        )
    };
    let schema = ResourceSchema::new("ec2.Test")
        .attribute(AttributeSchema::optional("first", member(8)).build())
        .attribute(AttributeSchema::optional("second", member(9)).build())
        .validate()
        .unwrap();
    let bad: Value = Value::Concrete(ConcreteValue::String("vpc-abcdef34".into()));
    let good: Value = Value::Concrete(ConcreteValue::String("vpc-abcdef12".into()));
    assert!(
        schema
            .effective_attribute("first")
            .unwrap()
            .validate(&bad)
            .is_err()
    );
    schema
        .effective_attribute("first")
        .unwrap()
        .validate(&good)
        .unwrap();
    ValidatedProviderSchemaWitness::new(
        vec![schema],
        Default::default(),
        Default::default(),
        vec![],
    )
    .unwrap();
}

#[test]
fn identity_662_round9_finite_values_encode_the_checked_policy_without_member_bases() {
    let definition = carina_aws_types::IdentityDefinition::enum_(
        carina_core::schema::enum_identity("Letters", Some("test")),
        Some(PatternSpec::new("^A+$", PatternMatch::Full)),
        Some((Some(1), Some(2))),
        Some(vec!["A".into(), "AA".into()]),
        vec![],
        None,
        None,
    )
    .into_attribute_type();
    let narrowed = carina_aws_types::with_member_values(definition, &["A"]);
    let RawShape::Enum { base, .. } = narrowed.raw_shape() else {
        panic!("Enum")
    };
    let RawShape::String {
        identity_layer,
        member_layer,
    } = base.raw_shape()
    else {
        panic!("String")
    };
    assert!(identity_layer.pattern.is_none() && identity_layer.length.is_none());
    assert!(member_layer.pattern.is_none() && member_layer.length.is_none());
    carina_core::schema::ValidatedConfigSchema::validate(std::collections::BTreeMap::from([(
        "value".into(),
        narrowed,
    )]))
    .unwrap();
}

#[test]
fn identity_662_round9_named_string_finite_intersection_transports() {
    let source = AttributeType::enum_with_base(
        carina_core::schema::enum_identity("Source", Some("test")),
        AttributeType::string(),
        Some(vec!["a".into(), "aa".into()]),
        vec![],
        None,
        None,
    );
    let named = carina_aws_types::IdentityDefinition::string(
        carina_core::schema::enum_identity("Letters", Some("test")),
        Some(PatternSpec::new("^a+$", PatternMatch::Full)),
        Some((Some(1), Some(3))),
        None,
        None,
    )
    .into_attribute_type();
    let schema = ResourceSchema::new("test.Finite")
        .attribute(AttributeSchema::optional("value", with_member_identity(source, named)).build())
        .validate()
        .expect("a checked finite set needs no constrained Enum base");
    carina_core::protocol_convert::resource_schema_to_protocol(&schema).unwrap();
}
