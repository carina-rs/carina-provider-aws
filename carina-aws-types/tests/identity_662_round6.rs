use carina_aws_types::*;
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeType, PatternMatch, PatternSpec, RawShape, TypeIdentity, ValidatedConfigSchema,
    legacy_validator,
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
fn identity_with_callback() -> AttributeType {
    AttributeType::refined_string_with_validator(
        id("OnlyOkay"),
        None,
        None,
        legacy_validator(|v| match v {
            Value::Concrete(ConcreteValue::String(s)) if s.expose_plaintext() == "okay" => Ok(()),
            _ => Err("expected okay".into()),
        }),
        None,
    )
}

#[test]
fn identity_662_round9_structural_string_identity_binds_without_a_callback() {
    for named in [
        AttributeType::refined_string(
            Some(id("PatternOnly")),
            Some(PatternSpec::new("^x$", PatternMatch::Full)),
            None,
            None,
        ),
        AttributeType::refined_string(Some(id("LengthOnly")), None, Some((Some(1), Some(3))), None),
    ] {
        let member = try_with_member_identity(AttributeType::string(), named).unwrap();
        assert_eq!(diagnostics(member, "").len(), 1);
    }
}
#[test]
fn identity_662_round9_structural_int_identity_binds_without_a_callback() {
    let named = AttributeType::refined_int(Some(id("RangeOnly")), Some((Some(1), Some(3))));
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
fn identity_662_round6_04_member_keeps_identity_length_in_its_callback() {
    let member = with_member_identity(AttributeType::string(), route53_hosted_zone_id());
    assert_eq!(
        diagnostics(member, &format!("Z{}", "A".repeat(39))).len(),
        1,
        "40-character hosted-zone ID must fail nominal validation"
    );
}
#[test]
fn identity_662_round9_identity_layer_reports_only_the_first_failure() {
    let standalone = route53_hosted_zone_id();
    for ty in [
        standalone.clone(),
        with_member_identity(AttributeType::string(), standalone),
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
fn identity_662_round9_identity_layer_is_the_single_structural_owner() {
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
fn identity_662_round6_07_enum_values_intersect_the_named_callback() {
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
fn identity_662_round6_07_empty_callback_intersection_is_an_error() {
    assert!(try_with_member_identity(closed(&["bad"]), identity_with_callback()).is_err());
}
#[test]
fn identity_662_round6_07_nameless_string_is_an_error_instead_of_a_panic() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_with_member_identity(closed(&["okay"]), AttributeType::string())
    }));
    assert!(result.is_ok(), "fallible binding must not panic");
    assert!(result.unwrap().is_err());
}
