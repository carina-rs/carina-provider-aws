use carina_core::schema::{AttributeType, RawShape};

/// Intersect an integer constructor's existing constraints with wire bounds,
/// preserving its identity definition and semantic validator. Wire bounds
/// belong to the declaration's member layer.
pub fn with_int_range(
    attribute_type: AttributeType,
    minimum: Option<i64>,
    maximum: Option<i64>,
) -> AttributeType {
    let RawShape::Int {
        identity_layer,
        member_layer,
    } = attribute_type.raw_shape()
    else {
        panic!("integer bounds require an Int constructor");
    };
    let anonymous = identity_layer.identity.is_none();
    let existing_member_range = if anonymous {
        carina_core::schema::intersect_bounds(identity_layer.range, member_layer.range)
    } else {
        member_layer.range
    };
    let identity = match identity_layer.validate {
        Some(validate) => carina_core::schema::IntIdentityLayer::with_validator(
            identity_layer
                .identity
                .expect("a validated Int has an identity")
                .clone(),
            identity_layer.range,
            validate.clone(),
        ),
        None => carina_core::schema::IntIdentityLayer::new(
            identity_layer.identity.cloned(),
            if anonymous {
                None
            } else {
                identity_layer.range
            },
        ),
    };
    AttributeType::int_member(
        identity,
        carina_core::schema::IntMemberConstraints::new(carina_core::schema::intersect_bounds(
            existing_member_range,
            Some((minimum, maximum)),
        )),
    )
}

/// Anonymous signed 32-bit carrier for authored helpers that model Smithy
/// Integer members. Smithy Long members continue using `AttributeType::int()`.
pub fn smithy_integer() -> AttributeType {
    with_int_range(
        AttributeType::int(),
        Some(i64::from(i32::MIN)),
        Some(i64::from(i32::MAX)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::{ConcreteValue, Value};

    #[test]
    fn member_ranges_preserve_one_identity_across_declarations() {
        use carina_core::schema::{ValidatedConfigSchema, ValidatedProviderSchemaWitness};
        use std::collections::BTreeMap;

        let original = crate::logs_log_group_retention_in_days();
        let bounded = with_int_range(original.clone(), Some(3), Some(90));
        let narrower = with_int_range(bounded.clone(), Some(7), Some(30));
        let RawShape::Int { member_layer, .. } = narrower.raw_shape() else {
            panic!("bounded Int");
        };
        assert_eq!(member_layer.range, Some((Some(7), Some(30))));
        let config = ValidatedConfigSchema::validate(BTreeMap::from([
            ("original".into(), original),
            ("bounded".into(), bounded),
            ("narrower".into(), narrower),
        ]))
        .expect("each declaration retains its local bounds");
        ValidatedProviderSchemaWitness::new(vec![], config, BTreeMap::new(), vec![])
            .expect("local bounds must not conflict with the reusable identity");
    }

    #[test]
    fn wire_bounds_intersect_existing_ranges_and_preserve_semantic_validation() {
        let bounded = with_int_range(crate::logs_log_group_retention_in_days(), Some(3), Some(90));
        let RawShape::Int {
            identity_layer:
                carina_core::schema::IntIdentityParts {
                    identity,
                    range,
                    validate,
                },
            ..
        } = bounded.raw_shape()
        else {
            panic!("bounded Int");
        };
        assert_eq!(
            identity,
            Some(&crate::AwsType::LogsLogGroupRetentionInDays.identity())
        );
        assert_eq!(range, None);
        let RawShape::Int { member_layer, .. } = bounded.raw_shape() else {
            unreachable!()
        };
        assert_eq!(member_layer.range, Some((Some(3), Some(90))));
        assert!(validate.is_some());
        let bounded = crate::tests::rootless(bounded);
        assert!(
            bounded
                .validate_collect(&Value::Concrete(ConcreteValue::Int(7)))
                .is_empty()
        );
        for value in [1, 4, 120, i64::MAX] {
            assert!(
                !bounded
                    .validate_collect(&Value::Concrete(ConcreteValue::Int(value)))
                    .is_empty()
            );
        }
        let narrower = with_int_range(
            AttributeType::refined_int(None, Some((Some(1), Some(100)))),
            Some(0),
            Some(i64::from(i32::MAX)),
        );
        assert!(matches!(
            narrower.raw_shape(),
            RawShape::Int {
                member_layer: carina_core::schema::IntMemberParts {
                    range: Some((Some(1), Some(100))),
                },
                ..
            }
        ));
    }
}
