use carina_core::schema::{AttributeType, RawShape};

/// Preserve the complete reusable identity and attach the slot's constraints.
/// Identity pattern/length/range and member pattern/length/range stay distinct.
pub fn with_member_identity(base: AttributeType, named: AttributeType) -> AttributeType {
    try_with_member_identity(base, named).expect("codegen validated the identity intersection")
}

/// Fallible derivation used by codegen, so empty finite intersections include
/// the resource/path instead of becoming a generated-schema startup panic.
pub fn try_with_member_identity(
    base: AttributeType,
    named: AttributeType,
) -> Result<AttributeType, String> {
    use carina_core::schema::{IntIdentityLayer, IntMemberConstraints};
    match named.raw_shape() {
        RawShape::String { member_layer, .. }
            if member_layer.pattern.is_some() || member_layer.length.is_some() =>
        {
            return Err("named String definition must not contain member constraints".into());
        }
        RawShape::Int { member_layer, .. } if member_layer.range.is_some() => {
            return Err("named Int definition must not contain member constraints".into());
        }
        _ => {}
    }
    Ok(match (base.raw_shape(), named.raw_shape()) {
        (
            RawShape::Int {
                identity_layer: source,
                member_layer,
            },
            RawShape::Int {
                identity_layer: named,
                ..
            },
        ) => {
            let range = if source.identity.is_none() {
                carina_core::schema::intersect_bounds(source.range, member_layer.range)
            } else {
                member_layer.range
            };
            let definition = match named.validate {
                Some(check) => IntIdentityLayer::with_validator(
                    named
                        .identity
                        .ok_or("named Int is missing identity")?
                        .clone(),
                    named.range,
                    check.clone(),
                ),
                None => IntIdentityLayer::new(named.identity.cloned(), named.range),
            };
            AttributeType::int_member(definition, IntMemberConstraints::new(range))
        }
        (RawShape::String { .. }, RawShape::String { .. }) => {
            let member = string_slot(&base)?;
            let definition = named
                .string_identity_layer()
                .expect("String definition")
                .clone();
            AttributeType::string_member(definition, member)
        }
        (
            RawShape::String { .. },
            RawShape::Enum {
                identity,
                values,
                dsl_aliases,
                validate,
                to_dsl,
                base: named_base,
                ..
            },
        ) => {
            let selected = values
                .map(|values| intersect_values(&base, values, None, validate))
                .transpose()?;
            let aliases = dsl_aliases
                .iter()
                .filter(|(v, _)| selected.as_ref().is_none_or(|values| values.contains(v)))
                .cloned()
                .collect();
            AttributeType::enum_with_base(
                identity.clone(),
                enum_base(&base, named_base, selected.as_deref())?,
                selected,
                aliases,
                validate.cloned(),
                to_dsl.cloned(),
            )
        }
        (
            RawShape::Enum {
                base: scalar,
                values,
                dsl_aliases,
                ..
            },
            RawShape::Enum {
                identity,
                values: named_values,
                validate,
                to_dsl,
                base: named_base,
                ..
            },
        ) => {
            let selected = values
                .or(named_values)
                .map(|candidates| intersect_values(scalar, candidates, named_values, validate))
                .transpose()?;
            let aliases = dsl_aliases
                .iter()
                .filter(|(v, _)| selected.as_ref().is_none_or(|values| values.contains(v)))
                .cloned()
                .collect();
            AttributeType::enum_with_base(
                identity.clone(),
                enum_base(scalar, named_base, selected.as_deref())?,
                selected,
                aliases,
                validate.cloned(),
                to_dsl.cloned(),
            )
        }
        (
            RawShape::Enum {
                base: scalar,
                values,
                dsl_aliases,
                ..
            },
            RawShape::String {
                identity_layer:
                    carina_core::schema::StringIdentityParts {
                        identity,
                        validate,
                        to_dsl,
                        ..
                    },
                ..
            },
        ) => {
            let identity = identity.ok_or("named String is missing its identity")?;
            let selected = values
                .map(|values| intersect_values(scalar, values, None, validate))
                .transpose()?;
            let aliases = dsl_aliases
                .iter()
                .filter(|(value, _)| {
                    selected
                        .as_ref()
                        .is_none_or(|values| values.contains(value))
                })
                .cloned()
                .collect();
            AttributeType::enum_with_base(
                identity.clone(),
                enum_base(
                    scalar,
                    &AttributeType::string_member(
                        named
                            .string_identity_layer()
                            .expect("String definition")
                            .clone(),
                        Default::default(),
                    ),
                    selected.as_deref(),
                )?,
                selected,
                aliases,
                validate.cloned(),
                to_dsl.cloned(),
            )
        }
        _ => return Err("identity decoration requires compatible scalar carriers".into()),
    })
}

/// Project declaration-local constraints without donating a named definition.
fn string_slot(ty: &AttributeType) -> Result<carina_core::schema::StringMemberConstraints, String> {
    use carina_core::schema::StringMemberConstraints;
    let RawShape::String {
        identity_layer,
        member_layer,
    } = ty.raw_shape()
    else {
        return Err("String carrier required".into());
    };
    if identity_layer.identity.is_some() {
        return Ok(StringMemberConstraints::new(
            member_layer.pattern.cloned(),
            member_layer.length,
        ));
    }
    let pattern = match (identity_layer.pattern, member_layer.pattern) {
        (Some(a), Some(b)) if a != b => {
            return Err("two anonymous String patterns cannot fit one member pattern".into());
        }
        (a, b) => a.or(b).cloned(),
    };
    Ok(StringMemberConstraints::new(
        pattern,
        carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length),
    ))
}

fn enum_base(
    source: &AttributeType,
    definition: &AttributeType,
    selected: Option<&[String]>,
) -> Result<AttributeType, String> {
    let slot = string_slot(source)?;
    if selected.is_none() && (slot.parts().pattern.is_some() || slot.parts().length.is_some()) {
        return Err("an open enum cannot carry Smithy member constraints: core forbids a member layer in an Enum base".into());
    }
    if let Some(values) = selected {
        for value in values {
            if !member_string_accepts(source, value) || !member_string_accepts(definition, value) {
                return Err(format!(
                    "enum value {value:?} violates the derived or identity constraints"
                ));
            }
        }
        // Protocol 27 represents a closed enum by its exact checked set.
        // A constrained base is not transportable, and adds no further check
        // once every finite value satisfied both the slot and the identity.
        return Ok(AttributeType::string());
    }
    // Definition-only base; named identity/presentation belongs to the Enum.
    let RawShape::String {
        identity_layer,
        member_layer,
    } = definition.raw_shape()
    else {
        return Err("enum definition base must be String".into());
    };
    if member_layer.pattern.is_some() || member_layer.length.is_some() {
        return Err("enum identity definition contains a member layer".into());
    }
    Ok(AttributeType::refined_string(
        None,
        identity_layer.pattern.cloned(),
        identity_layer.length,
        None,
    ))
}

fn intersect_values(
    base: &AttributeType,
    candidates: &[String],
    other: Option<&[String]>,
    nominal: Option<&carina_core::schema::CustomValidator>,
) -> Result<Vec<String>, String> {
    let selected = candidates
        .iter()
        .filter(|v| other.is_none_or(|values| values.contains(v)))
        .filter(|v| member_string_accepts(base, v))
        .filter(|v| {
            nominal.is_none_or(|check| {
                let input: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                    carina_core::resource::ConcreteValue::String(v.as_str().into()),
                );
                check(&input).is_ok()
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("empty allowed-value intersection".into());
    }
    Ok(selected)
}

/// Rebind an identity definition with no declaration-local restrictions.
pub fn with_member_enum_identity(
    ty: AttributeType,
    identity: carina_core::schema::TypeIdentity,
) -> AttributeType {
    enum_presentation(ty, identity, false)
}

/// Build a closed enum in one step after codegen validated each selected value
/// against the source slot. No member layer is ever stored inside its base.
pub fn with_member_enum_values(
    ty: AttributeType,
    identity: carina_core::schema::TypeIdentity,
    values: &[&str],
) -> AttributeType {
    with_member_values(enum_presentation(ty, identity, true), values)
}

fn enum_presentation(
    ty: AttributeType,
    identity: carina_core::schema::TypeIdentity,
    finite: bool,
) -> AttributeType {
    let (base, values, aliases, semantic, to_dsl) = match ty.raw_shape() {
        RawShape::String { identity_layer, .. } => (
            ty.clone(),
            None,
            vec![],
            identity_layer.validate.cloned(),
            identity_layer.to_dsl.cloned(),
        ),
        RawShape::Enum {
            base,
            values,
            dsl_aliases,
            validate,
            to_dsl,
            ..
        } => (
            base.clone(),
            values.map(|v| v.to_vec()),
            dsl_aliases.to_vec(),
            validate.cloned(),
            to_dsl.cloned(),
        ),
        _ => panic!("local enum requires a String or Enum carrier"),
    };
    let RawShape::String {
        identity_layer,
        member_layer,
    } = base.raw_shape()
    else {
        panic!("String enum base")
    };
    assert!(
        finite
            || values.is_some()
            || (identity_layer.pattern.is_none()
                && identity_layer.length.is_none()
                && member_layer.pattern.is_none()
                && member_layer.length.is_none()),
        "open enum cannot retain structural constraints"
    );
    let definition = AttributeType::refined_string(
        None,
        identity_layer.pattern.cloned(),
        identity_layer.length,
        None,
    );
    AttributeType::enum_with_base(identity, definition, values, aliases, semantic, to_dsl)
}

/// Install the exact finite list already validated by codegen. This performs
/// no regex compilation, intersection, or value filtering at schema construction.
pub fn with_member_values(ty: AttributeType, allowed: &[&str]) -> AttributeType {
    let RawShape::Enum {
        identity,
        dsl_aliases,
        validate,
        to_dsl,
        ..
    } = ty.raw_shape()
    else {
        panic!("allowed values require an enum")
    };
    let values: Vec<String> = allowed.iter().map(|v| (*v).into()).collect();
    assert!(!values.is_empty(), "empty allowed-value intersection");
    let aliases = if dsl_aliases.is_empty() {
        values
            .iter()
            .map(|v| (v.clone(), crate::dsl_enum_value(v)))
            .collect()
    } else {
        dsl_aliases
            .iter()
            .filter(|(v, _)| values.contains(v))
            .cloned()
            .collect()
    };
    // Protocol 27 closed enums store the exact finite intersection. Codegen
    // checked every value; no member layer may survive inside their base.
    AttributeType::enum_with_base(
        identity.clone(),
        AttributeType::string(),
        Some(values),
        aliases,
        validate.cloned(),
        to_dsl.cloned(),
    )
}

pub fn member_string_accepts(base: &AttributeType, value: &str) -> bool {
    let RawShape::String {
        identity_layer,
        member_layer,
    } = base.raw_shape()
    else {
        return false;
    };
    let count = value.chars().count() as u64;
    let bounds_ok = |bounds: Option<(Option<u64>, Option<u64>)>| {
        bounds.is_none_or(|(min, max)| {
            min.is_none_or(|n| count >= n) && max.is_none_or(|n| count <= n)
        })
    };
    let pattern_ok = |pattern: Option<&carina_core::schema::PatternSpec>| {
        pattern.is_none_or(|p| {
            carina_core::schema::CompiledPattern::try_new(p)
                .expect("validated pattern")
                .is_match(value)
        })
    };
    bounds_ok(identity_layer.length)
        && bounds_ok(member_layer.length)
        && pattern_ok(identity_layer.pattern)
        && pattern_ok(member_layer.pattern)
        && identity_layer.validate.is_none_or(|check| {
            let input: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                carina_core::resource::ConcreteValue::String(value.into()),
            );
            check(&input).is_ok()
        })
}

/// Preserve the declared String constraints when selecting comparison semantics.
pub fn with_member_reference(base: AttributeType, reference: AttributeType) -> AttributeType {
    let slot = string_slot(&base).expect("a reference requires one String member pattern");
    let source = slot.parts().pattern;
    let source_length = slot.parts().length;
    let RawShape::Reference {
        identity, length, ..
    } = reference.raw_shape()
    else {
        panic!("reference declaration")
    };
    let (a, b) = source_length.unwrap_or((None, None));
    let (c, d) = length.unwrap_or((None, None));
    let length = if source_length.is_some() || length.is_some() {
        Some((a.into_iter().chain(c).max(), b.into_iter().chain(d).min()))
    } else {
        None
    };
    // The reference's additional grammar is a portable InputRule::Matches,
    // bound by codegen. Only the Smithy pattern occupies this slot.
    AttributeType::reference(identity.clone(), source.cloned(), length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::schema::{PatternMatch, PatternSpec};
    #[test]
    fn identity_662_standalone_constraints_are_not_donated_to_members() {
        let named = crate::iam_role_arn();
        assert!(matches!(
            named.raw_shape(),
            RawShape::String {
                identity_layer: carina_core::schema::StringIdentityParts {
                    pattern: Some(_),
                    validate: Some(_),
                    ..
                },
                ..
            }
        ));
        let base = AttributeType::refined_string(
            None,
            Some(PatternSpec::new(
                "^member-",
                PatternMatch::Substring {
                    reason: carina_core::schema::PatternReason::new("fixture search").unwrap(),
                },
            )),
            Some((Some(3), Some(9))),
            None,
        );
        let member = with_member_identity(base, named);
        let RawShape::String {
            identity_layer:
                carina_core::schema::StringIdentityParts {
                    identity,
                    validate,
                    pattern: own_pattern,
                    ..
                },
            member_layer: carina_core::schema::StringMemberParts { pattern, length },
        } = member.raw_shape()
        else {
            panic!("String")
        };
        assert!(own_pattern.is_some());
        assert_eq!(pattern.unwrap().as_str(), "^member-");
        assert_eq!(length, Some((Some(3), Some(9))));
        assert_eq!(identity, Some(&crate::AwsType::IamRoleArn.identity()));
        assert!(validate.is_some());
    }

    #[test]
    fn identity_662_integer_binding_keeps_the_declared_range_and_callback() {
        let member = with_member_identity(
            AttributeType::refined_int(None, Some((Some(10), Some(100)))),
            crate::logs_log_group_retention_in_days(),
        );
        assert!(matches!(
            member.raw_shape(),
            RawShape::Int {
                identity_layer: carina_core::schema::IntIdentityParts {
                    validate: Some(_),
                    ..
                },
                member_layer: carina_core::schema::IntMemberParts {
                    range: Some((Some(10), Some(100)))
                }
            }
        ));
    }
}
