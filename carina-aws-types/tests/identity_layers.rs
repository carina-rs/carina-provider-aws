use carina_aws_types::*;
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, PatternMatch, PatternSpec, RawShape, ResourceSchema,
    StringIdentityLayer, StringMemberConstraints, TypeIdentity, ValidatedConfigSchema,
    ValidatedProviderSchemaWitness, legacy_validator,
};
use std::collections::BTreeMap;

fn id(name: &str) -> TypeIdentity {
    carina_core::schema::enum_identity(name, Some("test"))
}
fn value(text: &str) -> Value {
    Value::Concrete(ConcreteValue::String(text.into()))
}
fn diagnostics(ty: AttributeType, text: &str) -> Vec<String> {
    let schema = ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
    schema
        .rootless_type("value")
        .unwrap()
        .validate_collect(&value(text))
        .into_iter()
        .map(|(_, error)| error.to_string())
        .collect()
}
fn closed(values: &[&str]) -> AttributeType {
    AttributeType::enum_(
        id("Old"),
        Some(values.iter().map(|v| (*v).into()).collect()),
        vec![],
        None,
        None,
    )
}
fn identity_with_callback() -> IdentityDefinition {
    IdentityDefinition::string(
        id("OnlyOkay"),
        None,
        None,
        Some(legacy_validator(|v| match v {
            Value::Concrete(ConcreteValue::String(s)) if s.expose_plaintext() == "okay" => Ok(()),
            _ => Err("expected okay".into()),
        })),
        None,
    )
}

#[test]
fn structural_string_identity_binds_without_a_callback() {
    for named in [
        IdentityDefinition::string(
            id("PatternOnly"),
            Some(PatternSpec::new("^x$", PatternMatch::Full)),
            None,
            None,
            None,
        ),
        IdentityDefinition::string(id("LengthOnly"), None, Some((Some(1), Some(3))), None, None),
    ] {
        let member = try_with_member_identity(AttributeType::string(), named).unwrap();
        assert_eq!(diagnostics(member, "").len(), 1);
    }
}
#[test]
fn structural_int_identity_binds_without_a_callback() {
    let named = IdentityDefinition::int(id("RangeOnly"), Some((Some(1), Some(3))), None);
    let member = try_with_member_identity(AttributeType::int(), named).unwrap();
    let schema =
        ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), member)])).unwrap();
    let bad: Value = Value::Concrete(ConcreteValue::Int(4));
    assert_eq!(
        schema
            .rootless_type("value")
            .unwrap()
            .validate_collect(&bad)
            .len(),
        1
    );
}
#[test]
fn member_keeps_identity_length() {
    let member = with_member_identity(
        AttributeType::string(),
        AwsType::Route53HostedZoneId.definition(),
    );
    assert_eq!(
        diagnostics(member, &format!("Z{}", "A".repeat(39))).len(),
        1,
        "40-character hosted-zone ID must fail nominal validation"
    );
}
#[test]
fn identity_layer_reports_only_the_first_failure() {
    let standalone = route53_hosted_zone_id();
    for ty in [
        standalone.clone(),
        with_member_identity(
            AttributeType::string(),
            AwsType::Route53HostedZoneId.definition(),
        ),
    ] {
        let bad_pattern = diagnostics(ty.clone(), "PRIVATE-TOKEN");
        assert_eq!(bad_pattern.len(), 1);
        assert!(bad_pattern[0].contains("pattern"), "{bad_pattern:?}");
        let bad_length = diagnostics(ty, &format!("Z{}", "A".repeat(39)));
        assert_eq!(bad_length.len(), 1);
        assert!(bad_length[0].contains("32"), "{bad_length:?}");
    }
}

#[test]
fn identity_layer_is_the_single_structural_owner() {
    for row in [
        AwsType::Route53HostedZoneId,
        AwsType::AcmCertificateArn,
        AwsType::S3Grantee,
    ] {
        let ty = row.attribute_type();
        let RawShape::String {
            identity_layer,
            member_layer,
        } = ty.raw_shape()
        else {
            panic!("String")
        };
        assert!(identity_layer.pattern.is_some() && identity_layer.validate.is_none());
        assert!(member_layer.pattern.is_none() && member_layer.length.is_none());
    }
}
#[test]
fn enum_values_intersect_the_named_callback() {
    let ty = try_with_member_identity(closed(&["okay", "bad"]), identity_with_callback()).unwrap();
    let RawShape::Enum {
        values: Some(values),
        ..
    } = ty.raw_shape()
    else {
        panic!("closed enum")
    };
    assert_eq!(values, ["okay"]);
}
#[test]
fn empty_callback_intersection_is_an_error() {
    assert!(try_with_member_identity(closed(&["bad"]), identity_with_callback()).is_err());
}

fn findings(ty: AttributeType, text: &str) -> Vec<String> {
    let is_enum = matches!(ty.raw_shape(), RawShape::Enum { .. });
    let schema = ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
    let value: Value = if is_enum {
        Value::Concrete(ConcreteValue::enum_identifier(text))
    } else {
        Value::Concrete(ConcreteValue::String(text.into()))
    };
    schema
        .rootless_type("value")
        .unwrap()
        .validate_collect(&value)
        .into_iter()
        .map(|(_, e)| e.to_string())
        .collect()
}

#[test]
fn catalog_definitions_never_contain_member_constraints() {
    for row in AwsType::ALL {
        let ty = row.attribute_type();
        let scalar = if let RawShape::Enum { base, .. } = ty.raw_shape() {
            base
        } else {
            &ty
        };
        match scalar.raw_shape() {
            RawShape::String { member_layer, .. } => assert!(
                member_layer.pattern.is_none() && member_layer.length.is_none(),
                "{row:?}"
            ),
            RawShape::Int { member_layer, .. } => assert!(member_layer.range.is_none(), "{row:?}"),
            RawShape::Reference { .. } => {}
            _ => panic!("unexpected catalog carrier"),
        }
    }
}

#[test]
fn members_keep_lowercase_ids_and_arn_whole_value_checks() {
    for (row, bad) in [
        (AwsType::VpcId, "vpc-ABCDEF12"),
        (AwsType::IamRoleId, "AROAabcdefghijkl"),
        (AwsType::AvailabilityZoneId, "u1e1-az1"),
        (AwsType::Arn, "arn:aws:s3:::bucket\n"),
        (AwsType::IamRoleArn, "arn:aws:iam::123456789012:role/a\n"),
    ] {
        let ty = with_member_identity(AttributeType::string(), row.definition());
        let errors = findings(ty, bad);
        assert_eq!(errors.len(), 1, "{row:?}: {errors:?}");
        assert!(
            errors[0].contains("pattern") || errors[0].contains("expected"),
            "{errors:?}"
        );
    }
}

#[test]
fn standalone_invalid_value_has_one_diagnostic() {
    for row in [
        AwsType::AccountId,
        AwsType::VpcId,
        AwsType::IamRoleId,
        AwsType::Arn,
        AwsType::AcmCertificateDomainName,
    ] {
        assert_eq!(findings(row.attribute_type(), "?").len(), 1, "{row:?}");
    }
}

#[test]
fn semantic_callback_cannot_bypass_identity_pattern() {
    use carina_core::schema::{PatternMatch, PatternSpec, legacy_validator};
    let named = IdentityDefinition::string(
        carina_core::schema::enum_identity("Letters", Some("test")),
        Some(PatternSpec::new("^A$", PatternMatch::Full)),
        None,
        Some(legacy_validator(|_| Ok(()))),
        None,
    );
    let member = try_with_member_identity(AttributeType::string(), named).unwrap();
    assert!(findings(member.clone(), "A").is_empty());
    assert_eq!(findings(member, "B").len(), 1);
}

#[test]
fn enum_definitions_validate_finite_sets_and_reject_unsupported_open_constraints() {
    use carina_core::schema::{PatternMatch, PatternSpec};
    let build = |values| {
        IdentityDefinition::try_enum(
            carina_core::schema::enum_identity("Letters", Some("test")),
            Some(PatternSpec::new("^A+$", PatternMatch::Full)),
            Some((Some(1), Some(2))),
            values,
            vec![],
            None,
            None,
        )
    };
    assert!(build(None).err().unwrap().contains("open Enum"));
    let named = build(Some(vec!["A".into(), "AA".into()])).unwrap();
    for ty in [
        named.clone().into_attribute_type(),
        with_member_identity(AttributeType::string(), named),
    ] {
        assert!(findings(ty.clone(), "A").is_empty());
        for invalid in ["AB", "AAA"] {
            assert_eq!(findings(ty.clone(), invalid).len(), 1);
        }
    }
}

#[test]
fn closed_definition_rejects_inconsistent_values() {
    use carina_core::schema::{PatternMatch, PatternSpec};
    let result = IdentityDefinition::try_enum(
        carina_core::schema::enum_identity("Letters", Some("test")),
        Some(PatternSpec::new("^A+$", PatternMatch::Full)),
        Some((Some(1), Some(2))),
        Some(vec!["A".into(), "AB".into()]),
        vec![],
        None,
        None,
    );
    let error = result
        .err()
        .expect("AB violates the declared identity policy");
    assert!(
        error.contains("Letters") && error.contains("AB") && error.contains("pattern"),
        "{error}"
    );
}

#[test]
fn standalone_constraints_are_identity_metadata() {
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
fn members_preserve_one_identity_and_separate_smithy_layers() {
    let member = |min| {
        with_member_identity(
            AttributeType::string_member(
                StringIdentityLayer::new(None, None, None, None),
                StringMemberConstraints::new(
                    Some(PatternSpec::new(".*12", PatternMatch::Full)),
                    Some((Some(min), Some(32))),
                ),
            ),
            AwsType::VpcId.definition(),
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
fn finite_values_encode_the_checked_policy_without_member_bases() {
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
fn named_string_finite_intersection_transports() {
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
    );
    let schema = ResourceSchema::new("test.Finite")
        .attribute(AttributeSchema::optional("value", with_member_identity(source, named)).build())
        .validate()
        .expect("a checked finite set needs no constrained Enum base");
    carina_core::protocol_convert::resource_schema_to_protocol(&schema).unwrap();
}
