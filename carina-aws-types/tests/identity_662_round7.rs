use carina_aws_types::*;
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, RawShape, ValidatedConfigSchema};
use std::collections::BTreeMap;

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
fn identity_662_round9_catalog_definitions_never_contain_member_constraints() {
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
fn identity_662_round7_01_members_keep_lowercase_ids_and_arn_whole_value_checks() {
    for (row, bad) in [
        (AwsType::VpcId, "vpc-ABCDEF12"),
        (AwsType::IamRoleId, "AROAabcdefghijkl"),
        (AwsType::AvailabilityZoneId, "u1e1-az1"),
        (AwsType::Arn, "arn:aws:s3:::bucket\n"),
        (AwsType::IamRoleArn, "arn:aws:iam::123456789012:role/a\n"),
    ] {
        let ty = with_member_identity(AttributeType::string(), row.attribute_type());
        let errors = findings(ty, bad);
        assert_eq!(errors.len(), 1, "{row:?}: {errors:?}");
        assert!(
            errors[0].contains("pattern") || errors[0].contains("expected"),
            "{errors:?}"
        );
    }
}

#[test]
fn identity_662_round7_01_standalone_invalid_value_has_one_diagnostic() {
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
fn identity_662_round9_semantic_callback_cannot_bypass_identity_pattern() {
    use carina_core::schema::{PatternMatch, PatternSpec, legacy_validator};
    let named = AttributeType::refined_string_with_validator(
        carina_core::schema::enum_identity("Letters", Some("test")),
        Some(PatternSpec::new("^A$", PatternMatch::Full)),
        None,
        legacy_validator(|_| Ok(())),
        None,
    );
    let member = try_with_member_identity(AttributeType::string(), named).unwrap();
    assert!(findings(member.clone(), "A").is_empty());
    assert_eq!(findings(member, "B").len(), 1);
}

#[test]
fn identity_662_round9_enum_definitions_validate_finite_sets_and_reject_unsupported_open_constraints()
 {
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
    let named = build(Some(vec!["A".into(), "AA".into()]))
        .unwrap()
        .into_attribute_type();
    for ty in [
        named.clone(),
        with_member_identity(AttributeType::string(), named),
    ] {
        assert!(findings(ty.clone(), "A").is_empty());
        for invalid in ["AB", "AAA"] {
            assert_eq!(findings(ty.clone(), invalid).len(), 1);
        }
    }
}

#[test]
fn identity_662_round7_01_closed_definition_rejects_inconsistent_values() {
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
