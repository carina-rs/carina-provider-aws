//! Regression contracts from review of issue #662.
use carina_aws_types::*;
use carina_core::schema::{AttributeType, RawShape};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn metadata(ty: &AttributeType) -> Option<Value> {
    Some(match ty.raw_shape() {
        RawShape::String {
            identity_layer:
                carina_core::schema::StringIdentityParts {
                    pattern, length, ..
                },
            ..
        } => {
            json!({"kind":"string","pattern":pattern.map(|p|p.as_str()),"mode":pattern.map(|p|format!("{:?}",p.mode())),"length":length})
        }
        RawShape::Reference {
            pattern, length, ..
        } => {
            json!({"kind":"reference","pattern":pattern.map(|p|p.as_str()),"mode":pattern.map(|p|format!("{:?}",p.mode())),"length":length})
        }
        RawShape::Int {
            identity_layer: carina_core::schema::IntIdentityParts { range, .. },
            ..
        } => json!({"kind":"int","range":range}),
        RawShape::Enum {
            base,
            values,
            dsl_aliases,
            ..
        } => {
            let mut meta = metadata(base)?;
            let obj = meta.as_object_mut().unwrap();
            obj.insert("kind".into(), json!("enum"));
            obj.insert("values".into(), json!(values));
            obj.insert("aliases".into(), json!(dsl_aliases));
            meta
        }
        _ => return None,
    })
}

#[test]
fn standalone_policies_keep_baseline_bounds_and_patterns() {
    use carina_core::resource::{ConcreteValue, Value as InputValue};
    use carina_core::schema::{CompiledPattern, PatternMatch, PatternSpec, ValidatedConfigSchema};
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/standalone-63b2a0f.json")).unwrap();
    // Keep the historical snapshot intact. #700's three additions have their
    // own exact-definition and boundary regression in identity_layers_tests.
    let additions = [
        AwsType::LogsLogGroupName,
        AwsType::SqsQueueName,
        AwsType::OrganizationsOrganizationId,
    ];
    assert_eq!(
        expected.as_object().unwrap().len() + additions.len(),
        AwsType::ALL.len()
    );
    for row in AwsType::ALL {
        if additions.contains(row) {
            assert!(expected.get(row.rust_variant_name()).is_none());
            continue;
        }
        let ty = row.attribute_type();
        let actual = metadata(&ty).unwrap();
        let mut declaration = expected[row.rust_variant_name()].clone();
        let own_string = declaration["kind"] == "string"
            && (!declaration["pattern"].is_null() || !declaration["length"].is_null());
        let own_int = declaration["kind"] == "int" && !declaration["range"].is_null();
        if !own_string && !own_int {
            assert_eq!(
                actual, declaration,
                "{row:?}: presentation and unconstrained metadata"
            );
            continue;
        }
        if own_int {
            let range = &declaration["range"];
            let mut cases = vec![];
            for edge in range.as_array().unwrap().iter().filter_map(Value::as_i64) {
                cases.extend([edge.saturating_sub(1), edge, edge.saturating_add(1)]);
            }

            assert_eq!(actual, declaration, "{row:?}");
            for candidate in [
                ty.clone(),
                with_member_identity(AttributeType::int(), row.definition()),
            ] {
                let schema =
                    ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), candidate)]))
                        .unwrap();
                for n in &cases {
                    let bounds = &expected[row.rust_variant_name()]["range"];
                    if bounds[0].as_i64().is_some_and(|min| *n < min)
                        || bounds[1].as_i64().is_some_and(|max| *n > max)
                    {
                        let value: InputValue = InputValue::Concrete(ConcreteValue::Int(*n));
                        assert_eq!(
                            schema
                                .rootless_type("value")
                                .unwrap()
                                .validate_collect(&value)
                                .len(),
                            1,
                            "{row:?}: {n}"
                        );
                    }
                }
            }
            continue;
        }
        let pattern = declaration["pattern"]
            .as_str()
            .map(|p| CompiledPattern::try_new(&PatternSpec::new(p, PatternMatch::Full)).unwrap());
        let length = declaration["length"]
            .as_array()
            .map(|b| (b[0].as_u64(), b[1].as_u64()));
        if *row == AwsType::AccountId {
            declaration["pattern"] = json!("^[0-9]{12}$");
        }
        if *row == AwsType::ResourceId {
            declaration["pattern"] = json!("^[a-z0-9]+-[0-9a-f]{8,}$");
        }
        if *row == AwsType::Arn {
            declaration["pattern"] =
                json!(carina_aws_types::arn_regex(r"[^:]+:[^:\n]*:[^:\n]*:.*$"));
        }
        assert_eq!(
            actual, declaration,
            "{row:?}: only owned structural slots change"
        );
        let mut cases = vec![
            String::new(),
            row.valid_sample().into(),
            "bad".into(),
            format!("{}\n", row.valid_sample()),
            format!("é{}", row.valid_sample()),
        ];
        for n in [0, 1, 19, 20, 31, 32, 33, 40, 63, 64, 65, 2047, 2048, 2049] {
            let sample = row.valid_sample();
            cases.push(
                sample.chars().take(n).collect::<String>()
                    + &"A".repeat(n.saturating_sub(sample.chars().count())),
            );
            cases.push(format!("Z{}", "A".repeat(n)));
        }
        for candidate in [
            ty.clone(),
            with_member_identity(AttributeType::string(), row.definition()),
        ] {
            let schema =
                ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), candidate)]))
                    .unwrap();
            for text in &cases {
                let value: InputValue =
                    InputValue::Concrete(ConcreteValue::String(text.as_str().into()));
                let findings = schema
                    .rootless_type("value")
                    .unwrap()
                    .validate_collect(&value);
                let size = text.chars().count() as u64;
                let structural = pattern.as_ref().is_none_or(|p| p.is_match(text))
                    && length.is_none_or(|(min, max)| {
                        min.is_none_or(|m| size >= m) && max.is_none_or(|m| size <= m)
                    });
                if !structural {
                    assert_eq!(
                        findings.len(),
                        1,
                        "{row:?}: must preserve baseline structural rejection {text:?}"
                    );
                }
                if text == row.valid_sample() {
                    assert!(findings.is_empty(), "{row:?}: valid sample");
                }
                if !findings.is_empty() {
                    assert_eq!(findings.len(), 1, "{row:?}: one diagnostic");
                }
            }
        }
    }
}

#[test]
fn pattern_identities_keep_portable_constraints_without_callbacks() {
    for row in [
        AwsType::Route53HostedZoneId,
        AwsType::AcmCertificateArn,
        AwsType::S3Grantee,
    ] {
        for ty in [
            row.attribute_type(),
            with_member_identity(AttributeType::string(), row.definition()),
        ] {
            let RawShape::String {
                identity_layer,
                member_layer,
            } = ty.raw_shape()
            else {
                panic!("String")
            };
            assert!(identity_layer.pattern.is_some() && identity_layer.validate.is_none());
            assert!(member_layer.pattern.is_none() && member_layer.length.is_none());
            let schema = carina_core::schema::ValidatedConfigSchema::validate(BTreeMap::from([(
                "value".into(),
                ty,
            )]))
            .unwrap();
            let value: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                carina_core::resource::ConcreteValue::String("".into()),
            );
            assert_eq!(
                schema
                    .rootless_type("value")
                    .unwrap()
                    .validate_collect(&value)
                    .len(),
                1
            );
        }
    }
}

#[test]
fn named_closed_enum_keeps_its_set() {
    let ty = with_member_identity(
        AttributeType::string(),
        AwsType::CloudFrontHostedZoneId.definition(),
    );
    let RawShape::Enum {
        values: Some(values),
        dsl_aliases,
        ..
    } = ty.raw_shape()
    else {
        panic!("closed identity became open")
    };
    assert_eq!(values, ["Z2FDTNDATAQYW2"]);
    assert_eq!(dsl_aliases, [("Z2FDTNDATAQYW2".into(), "global".into())]);
}

#[test]
fn prefixed_ids_share_one_identity_pattern() {
    for row in AwsType::ALL {
        if row.identity().segments.first().map(String::as_str) != Some("ec2")
            || row.identity().kind != "Id"
        {
            continue;
        }
        let Some((prefix, suffix)) = row.valid_sample().rsplit_once('-') else {
            continue;
        };
        if suffix.len() < 8 || !suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let ty = with_member_identity(AttributeType::string(), row.definition());
        let RawShape::String { identity_layer, .. } = ty.raw_shape() else {
            continue;
        };
        assert!(
            identity_layer.pattern.is_some() && identity_layer.validate.is_none(),
            "{row:?}"
        );
        let schema = carina_core::schema::ValidatedConfigSchema::validate(BTreeMap::from([(
            "value".into(),
            ty,
        )]))
        .unwrap();
        for text in ["wrong-01234567".into(), format!("{prefix}-ABCDEF12")] {
            let value: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                carina_core::resource::ConcreteValue::String(text.into()),
            );
            assert_eq!(
                schema
                    .rootless_type("value")
                    .unwrap()
                    .validate_collect(&value)
                    .len(),
                1,
                "{row:?}"
            );
        }
    }
}
