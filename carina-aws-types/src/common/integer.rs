use carina_core::schema::{AttributeType, RawShape};

/// Intersect an integer constructor's existing constraints with wire bounds,
/// preserving its identity and semantic validator.
pub fn with_int_range(
    attribute_type: AttributeType,
    minimum: Option<i64>,
    maximum: Option<i64>,
) -> AttributeType {
    use carina_core::schema::{IntIdentityLayer, IntMemberConstraints, intersect_bounds};
    let RawShape::Int {
        identity_layer,
        member_layer,
    } = attribute_type.raw_shape()
    else {
        panic!("integer bounds require an Int constructor")
    };
    let old = if identity_layer.identity.is_none() {
        intersect_bounds(identity_layer.range, member_layer.range)
    } else {
        member_layer.range
    };
    let range = intersect_bounds(old, Some((minimum, maximum)));
    let definition = match identity_layer.validate {
        Some(check) => IntIdentityLayer::with_validator(
            identity_layer
                .identity
                .expect("validated Int identity")
                .clone(),
            identity_layer.range,
            check.clone(),
        ),
        None => IntIdentityLayer::new(
            identity_layer.identity.cloned(),
            if identity_layer.identity.is_none() {
                None
            } else {
                identity_layer.range
            },
        ),
    };
    AttributeType::int_member(definition, IntMemberConstraints::new(range))
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
    fn wire_bounds_intersect_existing_ranges_and_preserve_semantic_validation() {
        let bounded = with_int_range(crate::logs_log_group_retention_in_days(), Some(3), Some(90));
        let RawShape::Int {
            identity_layer:
                carina_core::schema::IntIdentityParts {
                    identity, validate, ..
                },
            member_layer: carina_core::schema::IntMemberParts { range },
        } = bounded.raw_shape()
        else {
            panic!("bounded Int");
        };
        assert_eq!(
            identity,
            Some(&crate::AwsType::LogsLogGroupRetentionInDays.identity())
        );
        assert_eq!(range, Some((Some(3), Some(90))));
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
                    range: Some((Some(1), Some(100)))
                },
                ..
            }
        ));
    }
}
