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
    ReferenceStringIdentity(Box<TypeIdentity>),
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
            Self::ReferenceStringIdentity(id) => {
                write!(f, "reference slot cannot carry a String identity: {id}")
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

fn same_validator(left: Option<&CustomValidator>, right: Option<&CustomValidator>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => std::sync::Arc::ptr_eq(left, right),
        _ => false,
    }
}

type Bounds<T> = Option<(Option<T>, Option<T>)>;

fn checked_bounds<T: Ord + Copy>(
    left: Bounds<T>,
    right: Bounds<T>,
) -> Result<Bounds<T>, MemberIdentityError> {
    let bounds = carina_core::schema::intersect_bounds(left, right);
    if bounds.is_some_and(|(min, max)| matches!((min,max),(Some(min),Some(max)) if min>max)) {
        return Err("empty constraint intersection".into());
    }
    Ok(bounds)
}

fn same_string_definition(left: &AttributeType, right: &AttributeType) -> bool {
    match (left.raw_shape(), right.raw_shape()) {
        (
            RawShape::String {
                identity_layer: a,
                member_layer: am,
            },
            RawShape::String {
                identity_layer: b,
                member_layer: bm,
            },
        ) => {
            a.identity == b.identity
                && a.pattern == b.pattern
                && a.length == b.length
                && a.to_dsl == b.to_dsl
                && same_validator(a.validate, b.validate)
                && am.pattern == bm.pattern
                && am.length == bm.length
        }
        _ => false,
    }
}

fn check_existing_identity(
    base: &AttributeType,
    named: &AttributeType,
) -> Result<(), MemberIdentityError> {
    let existing = match base.raw_shape() {
        RawShape::String { identity_layer, .. } => identity_layer.identity,
        RawShape::Int { identity_layer, .. } => identity_layer.identity,
        RawShape::Enum { identity, .. } | RawShape::Reference { identity, .. } => Some(identity),
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
                && same_validator(a.validate, b.validate)
        }
        (
            RawShape::Int {
                identity_layer: a, ..
            },
            RawShape::Int {
                identity_layer: b, ..
            },
        ) => a.range == b.range && same_validator(a.validate, b.validate),
        (
            RawShape::Enum {
                base: a,
                values: av,
                dsl_aliases: aa,
                validate: ac,
                to_dsl: ad,
                ..
            },
            RawShape::Enum {
                base: b,
                values: bv,
                dsl_aliases: ba,
                validate: bc,
                to_dsl: bd,
                ..
            },
        ) => {
            av == bv
                && aa == ba
                && ad == bd
                && same_validator(ac, bc)
                && same_string_definition(a, b)
        }
        (
            RawShape::Reference {
                pattern: a,
                length: al,
                ..
            },
            RawShape::Reference {
                pattern: b,
                length: bl,
                ..
            },
        ) => a == b && al == bl,
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
                checked_bounds(source.range, member_layer.range)?
            } else {
                member_layer.range
            };
            checked_bounds(named.range, range)?;
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
            checked_bounds(definition.parts().length, member.parts().length)?;
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
                .map(|values| intersect_values(&base, values, &named))
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
                ..
            },
            RawShape::Enum {
                identity,
                values: named_values,
                dsl_aliases,
                validate,
                to_dsl,
                base: named_base,
                ..
            },
        ) => {
            let selected = values
                .or(named_values)
                .map(|candidates| intersect_values(scalar, candidates, &named))
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
        checked_bounds(identity_layer.length, member_layer.length)?,
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
    debug_assert!(identity_layer.pattern.is_none() && identity_layer.length.is_none());
    Ok(AttributeType::string())
}

fn intersect_values(
    base: &AttributeType,
    candidates: &[String],
    named: &AttributeType,
) -> Result<Vec<String>, MemberIdentityError> {
    let source = MemberStringValidator::new(base)?;
    let definition = MemberStringValidator::new(named)?;
    let selected = candidates
        .iter()
        .filter(|v| source.accepts(v) && definition.accepts(v))
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("empty allowed-value intersection".into());
    }
    Ok(selected)
}

/// Select a checked finite subset of an Enum definition without relabeling a
/// pre-identified source or accepting a member-constrained named argument.
pub fn try_with_named_enum_values(
    named: IdentityDefinition,
    values: &[&str],
    aliases: &[(&str, &str)],
) -> Result<AttributeType, MemberIdentityError> {
    let named = named.into_attribute_type();
    let RawShape::Enum {
        identity,
        base,
        validate,
        to_dsl,
        ..
    } = named.raw_shape()
    else {
        return Err("named Enum definition required".into());
    };
    if values.is_empty() {
        return Err("empty allowed-value intersection".into());
    }
    let policy = MemberStringValidator::new(&named)?;
    for value in values {
        if !policy.accepts(value) {
            return Err(format!("enum value {value:?} violates identity {identity}").into());
        }
    }
    for (canonical, _) in aliases {
        if !values.contains(canonical) {
            return Err(
                format!("alias target {canonical:?} is absent from the selected set").into(),
            );
        }
    }
    Ok(AttributeType::enum_with_base(
        identity.clone(),
        base.clone(),
        Some(values.iter().map(|v| (*v).into()).collect()),
        aliases
            .iter()
            .map(|(api, dsl)| ((*api).into(), (*dsl).into()))
            .collect(),
        validate.cloned(),
        to_dsl.cloned(),
    ))
}

pub fn with_named_enum_values(
    named: IdentityDefinition,
    values: &[&str],
    aliases: &[(&str, &str)],
) -> AttributeType {
    let message = format!(
        "invalid named enum subset for {}",
        match named.as_attribute_type().raw_shape() {
            RawShape::Enum { identity, .. } => identity.to_string(),
            _ => "non-enum definition".into(),
        }
    );
    try_with_named_enum_values(named, values, aliases).expect(&message)
}

/// Build a finite local enum from an anonymous String slot. An identified
/// carrier must keep its catalog definition and use the named subset API.
pub fn try_with_member_enum_values(
    ty: AttributeType,
    identity: TypeIdentity,
    values: &[&str],
    aliases: &[(&str, &str)],
) -> Result<AttributeType, MemberIdentityError> {
    let existing = match ty.raw_shape() {
        RawShape::String { identity_layer, .. } => identity_layer.identity,
        RawShape::Enum { identity, .. } | RawShape::Reference { identity, .. } => Some(identity),
        _ => return Err("local enum requires an anonymous String carrier".into()),
    };
    if let Some(existing) = existing {
        return Err(MemberIdentityError::IdentityConflict {
            existing: Box::new(existing.clone()),
            requested: Box::new(identity),
        });
    }
    if values.is_empty() {
        return Err("empty allowed-value intersection".into());
    }
    let policy = MemberStringValidator::new(&ty)?;
    for value in values {
        if !policy.accepts(value) {
            return Err(
                format!("enum value {value:?} violates the source String constraints").into(),
            );
        }
    }
    for (canonical, _) in aliases {
        if !values.contains(canonical) {
            return Err(
                format!("alias target {canonical:?} is absent from the selected set").into(),
            );
        }
    }
    // The complete finite set has satisfied the anonymous slot policy. No
    // callback from an old identity can be donated to the new local enum.
    Ok(AttributeType::enum_with_base(
        identity,
        AttributeType::string(),
        Some(values.iter().map(|value| (*value).into()).collect()),
        aliases
            .iter()
            .map(|(api, dsl)| ((*api).into(), (*dsl).into()))
            .collect(),
        None,
        None,
    ))
}

/// Checked convenience constructor for a finite local enum.
pub fn with_member_enum_values(
    ty: AttributeType,
    identity: TypeIdentity,
    values: &[&str],
    aliases: &[(&str, &str)],
) -> AttributeType {
    let message = format!("invalid local enum values for {identity}");
    try_with_member_enum_values(ty, identity, values, aliases).expect(&message)
}

/// Select a finite subset while preserving the carrier's identity and aliases.
/// Every selected value must satisfy its full existing definition.
pub fn try_with_member_values(
    ty: AttributeType,
    allowed: &[&str],
) -> Result<AttributeType, MemberIdentityError> {
    let RawShape::Enum {
        identity,
        dsl_aliases,
        validate,
        to_dsl,
        ..
    } = ty.raw_shape()
    else {
        return Err("allowed values require an enum".into());
    };
    if allowed.is_empty() {
        return Err("empty allowed-value intersection".into());
    }
    let policy = MemberStringValidator::new(&ty)?;
    for value in allowed {
        if !policy.accepts(value) {
            return Err(format!(
                "enum value {value:?} is absent from or rejected by identity {identity}"
            )
            .into());
        }
    }
    let values: Vec<String> = allowed.iter().map(|v| (*v).into()).collect();
    let aliases = dsl_aliases
        .iter()
        .filter(|(v, _)| values.contains(v))
        .cloned()
        .collect();
    Ok(AttributeType::enum_with_base(
        identity.clone(),
        AttributeType::string(),
        Some(values),
        aliases,
        validate.cloned(),
        to_dsl.cloned(),
    ))
}

/// A String policy whose two patterns are compiled once before a value loop.
pub struct MemberStringValidator<'a> {
    lengths: [Option<(Option<u64>, Option<u64>)>; 2],
    patterns: Vec<CompiledPattern>,
    semantic: Option<&'a CustomValidator>,
    allowed: Option<&'a [String]>,
}
impl<'a> MemberStringValidator<'a> {
    pub fn new(base: &'a AttributeType) -> Result<Self, MemberIdentityError> {
        if let RawShape::Enum {
            base,
            values,
            validate,
            ..
        } = base.raw_shape()
        {
            let mut definition = Self::new(base)?;
            definition.allowed = values;
            definition.semantic = validate;
            return Ok(definition);
        }
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
            allowed: None,
        })
    }
    pub fn accepts(&self, value: &str) -> bool {
        let count = value.chars().count() as u64;
        self.lengths.iter().all(|b| {
            b.is_none_or(|(min, max)| {
                min.is_none_or(|n| count >= n) && max.is_none_or(|n| count <= n)
            })
        }) && self.patterns.iter().all(|p| p.is_match(value))
            && self
                .allowed
                .is_none_or(|values| values.iter().any(|v| v == value))
            && self.semantic.is_none_or(|check| {
                let input: carina_core::resource::Value = carina_core::resource::Value::Concrete(
                    carina_core::resource::ConcreteValue::String(value.into()),
                );
                check(&input).is_ok()
            })
    }
}

/// Bind the canonical reference definition. Slot constraints require InputRules
/// and are rejected here rather than becoming identity-wide grammar or bounds.
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
    if matches!(base.raw_shape(), RawShape::Reference { .. }) {
        return Ok(reference);
    }
    let RawShape::String {
        identity_layer: i,
        member_layer: m,
    } = base.raw_shape()
    else {
        return Err("String carrier required".into());
    };
    if i.pattern.is_some() || i.length.is_some() || m.pattern.is_some() || m.length.is_some() {
        return Err("reference slot constraints require InputRules".into());
    }
    if !matches!(reference.raw_shape(), RawShape::Reference { .. }) {
        return Err("reference definition required".into());
    }
    Ok(reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::schema::{PatternMatch, PatternSpec};
    #[test]
    fn finite_candidates_check_the_full_named_policy_before_callback() {
        let named=IdentityDefinition::string(TypeIdentity::bare("Checked"),Some(PatternSpec::new("^good$",PatternMatch::Full)),None,
            Some(carina_core::schema::legacy_validator(|input| {
                if matches!(input,carina_core::resource::Value::Concrete(carina_core::resource::ConcreteValue::String(value)) if value.expose_plaintext()=="bad") {
                    panic!("structural rejection must precede semantic validation");
                }
                Ok(())
            })),None).into_attribute_type();
        let result = intersect_values(
            &AttributeType::string(),
            &["bad".into(), "good".into()],
            &named,
        )
        .unwrap();
        assert_eq!(result, ["good"]);
    }
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
