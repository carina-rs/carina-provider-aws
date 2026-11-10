use crate::IdentityDefinition;
use carina_core::schema::{
    AttributeType, CompiledPattern, CustomValidator, RawShape, TypeIdentity,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberIdentityError {
    IdentityConflict {
        existing: Box<TypeIdentity>,
        requested: Box<TypeIdentity>,
    },
    ConflictingDefinition(Box<TypeIdentity>),
    OpenEnumConstraints,
    Invalid(String),
}
impl std::fmt::Display for MemberIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdentityConflict {
                existing,
                requested,
            } => write!(f, "cannot replace identity {existing} with {requested}"),
            Self::ConflictingDefinition(id) => {
                write!(f, "conflicting identity definition for {id}")
            }
            Self::OpenEnumConstraints => f.write_str(
                "open Enum bases cannot enforce identity or member pattern/length constraints",
            ),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for MemberIdentityError {}
impl From<String> for MemberIdentityError {
    fn from(s: String) -> Self {
        Self::Invalid(s)
    }
}
impl From<&str> for MemberIdentityError {
    fn from(s: &str) -> Self {
        Self::Invalid(s.into())
    }
}

fn check_existing_identity(
    base: &AttributeType,
    named: &AttributeType,
) -> Result<(), MemberIdentityError> {
    let existing = match base.raw_shape() {
        RawShape::String { identity_layer, .. } => identity_layer.identity,
        RawShape::Int { identity_layer, .. } => identity_layer.identity,
        _ => None,
    };
    let Some(existing) = existing else {
        return Ok(());
    };
    let requested = match named.raw_shape() {
        RawShape::String { identity_layer, .. } => identity_layer.identity,
        RawShape::Int { identity_layer, .. } => identity_layer.identity,
        RawShape::Enum { identity, .. } | RawShape::Reference { identity, .. } => Some(identity),
        _ => None,
    }
    .ok_or("named definition has no identity")?;
    if existing != requested {
        return Err(MemberIdentityError::IdentityConflict {
            existing: Box::new(existing.clone()),
            requested: Box::new(requested.clone()),
        });
    }
    let same = match (base.raw_shape(), named.raw_shape()) {
        (
            RawShape::String {
                identity_layer: a, ..
            },
            RawShape::String {
                identity_layer: b, ..
            },
        ) => {
            a.pattern == b.pattern
                && a.length == b.length
                && a.to_dsl == b.to_dsl
                && a.validate.is_some() == b.validate.is_some()
        }
        (
            RawShape::Int {
                identity_layer: a, ..
            },
            RawShape::Int {
                identity_layer: b, ..
            },
        ) => a.range == b.range && a.validate.is_some() == b.validate.is_some(),
        _ => false,
    };
    if !same {
        return Err(MemberIdentityError::ConflictingDefinition(Box::new(
            existing.clone(),
        )));
    }
    Ok(())
}

/// Preserve the complete reusable identity and attach the slot's constraints.
/// Identity pattern/length/range and member pattern/length/range stay distinct.
pub fn with_member_identity(base: AttributeType, named: IdentityDefinition) -> AttributeType {
    try_with_member_identity(base, named).expect("codegen validated the identity intersection")
}

/// Fallible derivation used by codegen, so empty finite intersections include
/// the resource/path instead of becoming a generated-schema startup panic.
pub fn try_with_member_identity(
    base: AttributeType,
    named: IdentityDefinition,
) -> Result<AttributeType, MemberIdentityError> {
    use carina_core::schema::{IntIdentityLayer, IntMemberConstraints};
    let named = named.into_attribute_type();
    check_existing_identity(&base, &named)?;
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
fn string_slot(
    ty: &AttributeType,
) -> Result<carina_core::schema::StringMemberConstraints, MemberIdentityError> {
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
) -> Result<AttributeType, MemberIdentityError> {
    let constrained = |ty: &AttributeType| {
        matches!(ty.raw_shape(),
        RawShape::String {identity_layer:i,member_layer:m}
            if i.pattern.is_some() || i.length.is_some() || m.pattern.is_some() || m.length.is_some())
    };
    if selected.is_none() && (constrained(source) || constrained(definition)) {
        return Err(MemberIdentityError::OpenEnumConstraints);
    }
    if let Some(values) = selected {
        let source_validator = MemberStringValidator::new(source)?;
        let definition_validator = MemberStringValidator::new(definition)?;
        for value in values {
            if !source_validator.accepts(value) || !definition_validator.accepts(value) {
                return Err(format!(
                    "enum value {value:?} violates the derived or identity constraints"
                )
                .into());
            }
        }
        // Protocol 27 represents a closed enum by its exact checked set.
        // A constrained base is not transportable, and adds no further check
        // once every finite value satisfied both the slot and the identity.
        return Ok(AttributeType::string());
    }
    // Definition-only base; named identity/presentation belongs to the Enum.
    let RawShape::String { identity_layer, .. } = definition.raw_shape() else {
        return Err("enum definition base must be String".into());
    };
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
) -> Result<Vec<String>, MemberIdentityError> {
    let validator = MemberStringValidator::new(base)?;
    let selected = candidates
        .iter()
        .filter(|v| other.is_none_or(|values| values.contains(v)))
        .filter(|v| validator.accepts(v))
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

/// A String policy whose two patterns are compiled once before a value loop.
pub struct MemberStringValidator<'a> {
    lengths: [Option<(Option<u64>, Option<u64>)>; 2],
    patterns: Vec<CompiledPattern>,
    semantic: Option<&'a CustomValidator>,
}
impl<'a> MemberStringValidator<'a> {
    pub fn new(base: &'a AttributeType) -> Result<Self, MemberIdentityError> {
        let RawShape::String {
            identity_layer: i,
            member_layer: m,
        } = base.raw_shape()
        else {
            return Err("String carrier required".into());
        };
        let patterns = [i.pattern, m.pattern]
            .into_iter()
            .flatten()
            .map(CompiledPattern::try_new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MemberIdentityError::Invalid(e.to_string()))?;
        Ok(Self {
            lengths: [i.length, m.length],
            patterns,
            semantic: i.validate,
        })
    }
    pub fn accepts(&self, value: &str) -> bool {
        let count = value.chars().count() as u64;
        self.lengths.iter().all(|b| {
            b.is_none_or(|(min, max)| {
                min.is_none_or(|n| count >= n) && max.is_none_or(|n| count <= n)
            })
        }) && self.patterns.iter().all(|p| p.is_match(value))
            && self.semantic.is_none_or(|check| {
                let input: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                    carina_core::resource::ConcreteValue::String(value.into()),
                );
                check(&input).is_ok()
            })
    }
}

/// Preserve the declared String constraints when selecting comparison semantics.
pub fn with_member_reference(base: AttributeType, reference: IdentityDefinition) -> AttributeType {
    try_with_member_reference(base, reference)
        .expect("codegen validated the reference intersection")
}

pub fn try_with_member_reference(
    base: AttributeType,
    reference: IdentityDefinition,
) -> Result<AttributeType, MemberIdentityError> {
    let reference = reference.into_attribute_type();
    check_existing_identity(&base, &reference)?;
    let slot = string_slot(&base)?;
    let RawShape::Reference {
        identity, length, ..
    } = reference.raw_shape()
    else {
        return Err("reference definition required".into());
    };
    let length = carina_core::schema::intersect_bounds(slot.parts().length, length);
    if length.is_some_and(|(a, b)| matches!((a,b),(Some(a),Some(b)) if a>b)) {
        return Err("empty reference length intersection".into());
    }
    // Codegen binds the reference's extra pattern as a portable input rule.
    Ok(AttributeType::reference(
        identity.clone(),
        slot.parts().pattern.cloned(),
        length,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::schema::{PatternMatch, PatternSpec};
    #[test]
    fn standalone_constraints_are_not_donated_to_members() {
        let named = crate::AwsType::IamRoleArn.definition();
        assert!(matches!(
            named.as_attribute_type().raw_shape(),
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
    fn integer_binding_keeps_the_declared_range_and_callback() {
        let member = with_member_identity(
            AttributeType::refined_int(None, Some((Some(10), Some(100)))),
            crate::AwsType::LogsLogGroupRetentionInDays.definition(),
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
