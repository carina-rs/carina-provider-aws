//! The one string-derivation seam used by Rust and Markdown generation.
use super::*;
use carina_codegen_aws::source_corrections;
use carina_core::schema::{CompiledPattern, PatternReason};
use carina_smithy::{ShapeRef, Traits};
use regex_syntax::ast::{AssertionKind, Ast, Flag};
use std::sync::Mutex;

static ISSUES: Mutex<BTreeMap<String, serde_json::Value>> = Mutex::new(BTreeMap::new());
static SOURCE_CORRECTIONS: Mutex<BTreeMap<String, serde_json::Value>> = Mutex::new(BTreeMap::new());

pub(super) fn record_issue(
    resource: &str,
    path: &str,
    smithy: Option<&str>,
    dropped: Option<&PatternSpec>,
    reason: &str,
) {
    let issue = serde_json::json!({"resource":resource,"path":path,"smithy_pattern":smithy,"dropped_override_pattern":dropped.map(PatternSpec::as_str),"reason":reason});
    ISSUES
        .lock()
        .expect("pattern issues")
        .insert(issue.to_string(), issue);
}

pub(super) fn write_issues() -> Result<()> {
    if let Some(path) = std::env::var_os("CARINA_CODEGEN_CONSTRAINT_REPORT") {
        let issues: Vec<_> = ISSUES
            .lock()
            .expect("pattern issues")
            .values()
            .cloned()
            .collect();
        std::fs::write(path, serde_json::to_string_pretty(&issues)?)?;
    }
    if let Some(path) = std::env::var_os("CARINA_CODEGEN_SOURCE_CORRECTION_REPORT") {
        let corrections: Vec<_> = SOURCE_CORRECTIONS
            .lock()
            .expect("source corrections")
            .values()
            .cloned()
            .collect();
        std::fs::write(path, serde_json::to_string_pretty(&corrections)?)?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn issues() -> Vec<serde_json::Value> {
    ISSUES
        .lock()
        .expect("pattern issues")
        .values()
        .cloned()
        .collect()
}

/// Mirror AWSCC #538/#539's parsed anchor proof, without its whole-value
/// wrapping policy: Smithy unanchored patterns retain RegExp.test semantics.
fn whole_anchored(source: &str) -> bool {
    fn boundaries(ast: &Ast, multiline: &mut bool) -> (bool, bool) {
        match ast {
            Ast::Assertion(assertion) => match assertion.kind {
                AssertionKind::StartLine => (!*multiline, false),
                AssertionKind::EndLine => (false, !*multiline),
                AssertionKind::StartText => (true, false),
                AssertionKind::EndText => (false, true),
                _ => (false, false),
            },
            Ast::Concat(concat) => {
                let mut result = (false, false);
                for (index, part) in concat.asts.iter().enumerate() {
                    let (start, end) = boundaries(part, multiline);
                    if index == 0 {
                        result.0 = start;
                    }
                    result.1 = end;
                }
                result
            }
            Ast::Alternation(alt) => {
                let mut result = (true, true);
                for part in &alt.asts {
                    let (start, end) = boundaries(part, multiline);
                    result.0 &= start;
                    result.1 &= end;
                }
                result
            }
            Ast::Group(group) => {
                let mut scoped = group
                    .flags()
                    .and_then(|f| f.flag_state(Flag::MultiLine))
                    .unwrap_or(*multiline);
                boundaries(&group.ast, &mut scoped)
            }
            Ast::Flags(flags) => {
                if let Some(value) = flags.flags.flag_state(Flag::MultiLine) {
                    *multiline = value;
                }
                (false, false)
            }
            Ast::Repetition(rep) => {
                boundaries(&rep.ast, multiline);
                (false, false)
            }
            _ => (false, false),
        }
    }
    regex_syntax::ast::parse::Parser::new()
        .parse(source)
        .is_ok_and(|ast| boundaries(&ast, &mut false) == (true, true))
}

// These lookaheads are regular prefix exclusions. Rewriting a guarded
// character-class repetition is exact when its first iteration is mandatory,
// or when the following class excludes the forbidden character. Other
// lookarounds remain intact and fail with the resource/path at compilation.
fn negative_prefix_guards(source: &str) -> String {
    use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, HirKind};
    fn class_at(source: &str) -> Option<(&str, ClassUnicode)> {
        if !source.starts_with('[') {
            return None;
        }
        let mut escaped = false;
        let mut depth = 1u32;
        for (index, ch) in source.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '[' {
                depth += 1;
            }
            if ch == ']' {
                depth -= 1;
            }
            if depth == 0 {
                let text = &source[..index + 1];
                let hir = regex_syntax::Parser::new().parse(text).ok()?;
                if let HirKind::Class(Class::Unicode(class)) = hir.kind() {
                    return Some((text, class.clone()));
                }
                return None;
            }
        }
        None
    }
    fn rewrite(source: &str) -> Option<(usize, String)> {
        let tail = source.strip_prefix("(?!")?;
        let forbidden = tail.chars().next()?;
        if !forbidden.is_ascii_alphanumeric() && forbidden != '-' && forbidden != '_' {
            return None;
        }
        let after = tail.get(forbidden.len_utf8()..)?.strip_prefix(')')?;
        let (class_text, mut class) = class_at(after)?;
        let quant = after.get(class_text.len()..)?;
        let (min, max, size) = if quant.starts_with('{') {
            let end = quant.find('}')?;
            let bounds = &quant[1..end];
            let (min, max) = if let Some((a, b)) = bounds.split_once(',') {
                (
                    a.parse::<u32>().ok()?,
                    if b.is_empty() {
                        None
                    } else {
                        Some(b.parse::<u32>().ok()?)
                    },
                )
            } else {
                let n = bounds.parse::<u32>().ok()?;
                (n, Some(n))
            };
            (min, max, end + 1)
        } else if quant.starts_with('+') {
            (1, None, 1)
        } else if quant.starts_with('*') {
            (0, None, 1)
        } else if quant.starts_with('?') {
            (0, Some(1), 1)
        } else {
            return None;
        };
        if max.is_some_and(|max| max == 0 || min > max) {
            return None;
        }
        if min == 0 {
            let following_element = &quant[size..];
            let (following_text, following) = class_at(following_element)?;
            let following_quantifier = &following_element[following_text.len()..];
            // The proof requires the following class to consume a character.
            // If it is nullable the guard must continue farther; leave that
            // unsupported lookahead intact so codegen fails instead of widening.
            if following_quantifier.starts_with(['?', '*'])
                || following_quantifier
                    .strip_prefix('{')
                    .and_then(|q| q.split([',', '}']).next())
                    .is_some_and(|min| min.parse::<u32>() == Ok(0))
            {
                return None;
            }
            if following
                .iter()
                .any(|range| range.start() <= forbidden && forbidden <= range.end())
            {
                return None;
            }
        }
        class.difference(&ClassUnicode::new([ClassUnicodeRange::new(
            forbidden, forbidden,
        )]));
        let first = if class.iter().next().is_none() {
            "[a&&b]".to_owned()
        } else {
            let ranges = class
                .iter()
                .map(|r| format!(r"\x{{{:x}}}-\x{{{:x}}}", r.start() as u32, r.end() as u32))
                .collect::<String>();
            format!("[{ranges}]")
        };
        let rest = match max {
            Some(max) => format!("{{{},{}}}", min.saturating_sub(1), max - 1),
            None => format!("{{{},}}", min.saturating_sub(1)),
        };
        let replacement = format!("{first}{class_text}{rest}");
        let replacement = if min == 0 {
            format!("(?:{replacement})?")
        } else {
            replacement
        };
        let consumed = source.len() - after.len() + class_text.len() + size;
        Some((consumed, replacement))
    }
    let mut result = String::new();
    let mut rest = source;
    let mut depth = 0u32;
    while !rest.is_empty() {
        if depth == 0
            && let Some((count, replacement)) = rewrite(rest)
        {
            result.push_str(&replacement);
            rest = &rest[count..];
            continue;
        }
        let ch = rest.chars().next().expect("nonempty");
        rest = &rest[ch.len_utf8()..];
        result.push(ch);
        if ch == '\\' {
            if let Some(next) = rest.chars().next() {
                result.push(next);
                rest = &rest[next.len_utf8()..];
            }
        } else if ch == '[' {
            depth += 1
        } else if ch == ']' {
            depth = depth.saturating_sub(1)
        }
    }
    result
}
fn translate(source: &str) -> Result<String> {
    const JS_WHITESPACE: &str = r"\x09-\x0D \u{00A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";
    let mut result = String::new();
    let mut chars = source.chars().peekable();
    let mut class = false;
    let mut first_in_class = false;
    let mut unpaired_atom = false;
    let mut range_endpoint = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let escaped = chars.next();
            if class
                && range_endpoint
                && escaped
                    .is_some_and(|c| matches!(c, 'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'p' | 'P'))
            {
                anyhow::bail!(
                    "ECMA character-class escape used as a range endpoint is unsupported"
                );
            }
            match escaped {
                Some('d') => result.push_str(if class { "0-9" } else { "[0-9]" }),
                Some('w') => result.push_str(if class { "A-Za-z0-9_" } else { "[A-Za-z0-9_]" }),
                Some('s') if class => result.push_str(JS_WHITESPACE),
                Some('s') => result.push_str(&format!("[{JS_WHITESPACE}]")),
                Some('D') => result.push_str("[^0-9]"),
                Some('W') => result.push_str("[^A-Za-z0-9_]"),
                Some('S') => result.push_str(&format!("[^{JS_WHITESPACE}]")),
                Some('b') if class => result.push_str(r"\x08"),
                Some('b') => result.push_str(r"(?-u:\b)"),
                Some('B') => result.push_str(r"(?-u:\B)"),
                Some(other @ ('u' | 'x')) => {
                    result.push('\\');
                    result.push(other);
                    if other == 'u' && chars.peek() == Some(&'{') {
                        for next in chars.by_ref() {
                            result.push(next);
                            if next == '}' {
                                break;
                            }
                        }
                    } else {
                        for _ in 0..if other == 'u' { 4 } else { 2 } {
                            if let Some(next) = chars.next() {
                                result.push(next);
                            }
                        }
                    }
                }
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
            if class {
                unpaired_atom = !range_endpoint
                    && !escaped.is_some_and(|c| {
                        matches!(c, 'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'p' | 'P')
                    });
                range_endpoint = false;
                first_in_class = false;
            }
        } else if ch == '.' && !class {
            result.push_str(r"[^\r\n\u2028\u2029]");
        } else {
            if ch == '[' && !class {
                class = true;
                first_in_class = true;
                unpaired_atom = false;
                result.push(ch);
            } else if ch == ']' {
                class = false;
                range_endpoint = false;
                result.push(ch);
            } else if class && ch == '^' && first_in_class {
                first_in_class = false;
                result.push(ch);
            } else if class
                && ch == '-'
                && !range_endpoint
                && unpaired_atom
                && chars.peek().is_some_and(|c| *c != ']')
            {
                // ECMA ranges consume an atom on each side. In [--a], the
                // first '-' is a literal endpoint, not a set-difference op.
                result.push('-');
                range_endpoint = true;
                unpaired_atom = false;
                first_in_class = false;
            } else {
                if class {
                    if matches!(ch, '[' | '&' | '~' | '-') {
                        result.push('\\');
                    }
                    unpaired_atom = !range_endpoint;
                    range_endpoint = false;
                    first_in_class = false;
                }
                result.push(ch);
            }
        }
    }
    // Normalize ECMA shorthand classes before the lookahead proof consults
    // regex-syntax's Unicode sets (whose native \w has different semantics).
    Ok(negative_prefix_guards(&result))
}

pub(super) fn smithy_pattern(resource: &str, path: &str, source: &str) -> Result<PatternSpec> {
    let translated = translate(source).with_context(|| {
        format!("{resource}.{path}: cannot translate Smithy pattern {source:?}")
    })?;
    let mode = if whole_anchored(&translated) {
        PatternMatch::Full
    } else {
        PatternMatch::Substring {
            reason: PatternReason::new("Smithy pattern is unanchored").expect("visible reason"),
        }
    };
    let pattern = PatternSpec::new(translated, mode);
    if let Err(error) = CompiledPattern::try_new(&pattern) {
        record_issue(
            resource,
            path,
            Some(source),
            None,
            &format!("Untranslatable Smithy pattern: {error}"),
        );
        anyhow::bail!("{resource}.{path}: cannot translate Smithy pattern {source:?}: {error}");
    }
    Ok(pattern)
}

pub(super) fn effective_traits(model: &SmithyModel, member: &ShapeRef) -> Traits {
    let mut traits = match model.get_shape(&member.target) {
        Some(
            Shape::String(s)
            | Shape::Integer(s)
            | Shape::Long(s)
            | Shape::Float(s)
            | Shape::Double(s),
        ) => s.traits.clone(),
        Some(Shape::List(s)) => s.traits.clone(),
        Some(Shape::Map(s)) => s.traits.clone(),
        Some(Shape::Enum(s)) => s.traits.clone(),
        _ => Traits::new(),
    };
    traits.extend(member.traits.clone());
    traits
}

pub(super) fn intersect<T: Ord + Copy>(left: Bounds<T>, right: Bounds<T>) -> Result<Bounds<T>> {
    if left.is_none() && right.is_none() {
        return Ok(None);
    }
    let bounds = carina_core::schema::intersect_bounds(left, right);
    let (min, max) = bounds.unwrap_or((None, None));
    anyhow::ensure!(
        !matches!((min,max),(Some(a),Some(b)) if a>b),
        "empty constraint intersection"
    );
    Ok(bounds)
}

/// An added pattern is always a separate portable input predicate. It never
/// occupies the declared type's single Smithy pattern slot, even without a
/// Smithy pattern. This gives every table the same conjunction semantics.
pub(super) fn input_patterns(rules: &[TypeOverride]) -> Vec<PatternSpec> {
    rules
        .iter()
        .flat_map(|rule| match rule {
            TypeOverride::StringPattern(pattern) | TypeOverride::RefinedString { pattern, .. } => {
                vec![pattern.clone()]
            }
            TypeOverride::StringProfile(profile) => match profile.constraints().raw_shape() {
                RawShape::String {
                    identity_layer,
                    member_layer,
                } => [identity_layer.pattern, member_layer.pattern]
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect(),
                _ => unreachable!("string profile"),
            },
            _ => vec![],
        })
        .collect()
}

pub(super) fn derive(
    model: &SmithyModel,
    member: &ShapeRef,
    resource: &str,
    path: &str,
    rules: &[TypeOverride],
) -> Result<ValueTypeOverride> {
    let traits = effective_traits(model, member);
    let source = traits
        .get("smithy.api#pattern")
        .and_then(|value| value.as_str());
    let correction = source_corrections::correction(resource, path);
    let corrected = correction.map(|correction| -> Result<String> {
        let source = source.with_context(|| format!("{resource}.{path}: expected a Smithy pattern starting with the literal ^arn:aws:"))?;
        let pattern = correction.apply(source).map_err(|error| anyhow::anyhow!("{resource}.{path}: Smithy model correction: {error}"))?;
        let record = serde_json::json!({"resource":resource,"path":path,"correction":format!("{correction:?}"),"smithy_pattern":source,"corrected_pattern":pattern,"note":correction.documentation()});
        SOURCE_CORRECTIONS.lock().expect("source corrections").insert(record.to_string(), record);
        Ok(pattern)
    }).transpose()?;
    let pattern = corrected
        .as_deref()
        .or(source)
        .map(|s| smithy_pattern(resource, path, s))
        .transpose()?;
    let mut length = length_from_traits(&traits);
    for rule in rules {
        let extra_length = match rule {
            TypeOverride::StringBounds { min, max } => Some((Some(*min), Some(*max))),
            TypeOverride::RefinedString {
                min_length,
                max_length,
                ..
            } => Some((Some(*min_length), Some(*max_length))),
            TypeOverride::StringProfile(profile) => {
                let ty = profile.constraints();
                let RawShape::String {
                    identity_layer,
                    member_layer,
                } = ty.raw_shape()
                else {
                    unreachable!("string profile")
                };
                carina_core::schema::intersect_bounds(identity_layer.length, member_layer.length)
            }
            _ => None,
        };
        length = intersect(length, extra_length).with_context(|| format!("{resource}.{path}"))?;
    }
    let validated_input_constraints = input_patterns(rules)
        .into_iter()
        .map(|p| super::ValidatedInputConstraint::pattern(resource, path, p))
        .collect::<Result<Vec<_>>>()?;
    let mut value = string_refinement(pattern.as_ref(), length);
    value.validated_input_constraints = validated_input_constraints;
    for rule in rules {
        value = decorate(value, rule).with_context(|| format!("{resource}.{path}"))?;
    }
    if let Some(correction) = correction {
        value.documentation = Some(match value.documentation {
            Some(note) => format!("{note}; {}", correction.documentation()),
            None => correction.documentation().into(),
        });
    }
    validate_closed_patterns(&value).with_context(|| format!("{resource}.{path}"))?;
    Ok(value)
}

pub(super) fn decorate(
    mut value: ValueTypeOverride,
    rule: &TypeOverride,
) -> Result<ValueTypeOverride> {
    match rule {
        TypeOverride::Identity(row)
        | TypeOverride::EnumIdentity(row)
        | TypeOverride::NamedValues { identity: row, .. } => {
            let named = row.definition();
            anyhow::ensure!(
                matches!(
                    (
                        value.attribute_type.raw_shape(),
                        named.as_attribute_type().raw_shape()
                    ),
                    (
                        RawShape::String { .. },
                        RawShape::String { .. } | RawShape::Enum { .. }
                    ) | (RawShape::Enum { .. }, RawShape::Enum { .. })
                        | (RawShape::Int { .. }, RawShape::Int { .. })
                ),
                "identity {row:?} has an incompatible carrier"
            );
            value.documentation = identity_validation_note(*row);
            value.attribute_type =
                carina_aws_types::try_with_member_identity(value.attribute_type, named.clone())
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("identity {row:?}"))?;
            if let TypeOverride::NamedValues { values, .. } = rule {
                select_allowed_values(&mut value, values)?;
            }
            // Emit only after the final checked subset and presentation are
            // known. Both codegen and runtime consume that same definition.
            value.type_code = if let RawShape::Enum {
                values: Some(values),
                dsl_aliases,
                ..
            } = value.attribute_type.raw_shape()
            {
                let aliases = dsl_aliases
                    .iter()
                    .map(|(api, dsl)| (api.as_str(), dsl.as_str()))
                    .collect::<Vec<_>>();
                format!(
                    "carina_aws_types::try_with_named_enum_values(carina_aws_types::AwsType::{row:?}.definition(), &{values:?}, &{aliases:?}).expect({:?})",
                    format!("invalid generated enum subset for {}", row.identity())
                )
            } else {
                format!(
                    "carina_aws_types::with_member_identity({}, carina_aws_types::AwsType::{row:?}.definition())",
                    value.type_code
                )
            };
        }

        TypeOverride::CoreIdentity(core) => {
            anyhow::ensure!(
                matches!(value.attribute_type.raw_shape(), RawShape::String { .. }),
                "core identity requires a String carrier"
            );
            value.attribute_type =
                carina_aws_types::try_with_member_identity(value.attribute_type, core.definition())
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("core identity {core:?}"))?;
            value.type_code = format!(
                "carina_aws_types::with_member_identity({}, carina_aws_types::CoreIdentity::{core:?}.definition())",
                value.type_code
            );
        }
        TypeOverride::Identities(rows) => {
            let mut members = Vec::new();
            let mut codes = Vec::new();
            for row in *rows {
                let child = decorate(
                    ValueTypeOverride {
                        type_code: value.type_code.clone(),
                        attribute_type: value.attribute_type.clone(),
                        documentation: None,
                        validated_input_constraints: Vec::new(),
                    },
                    &TypeOverride::Identity(*row),
                )?;
                members.push(child.attribute_type);
                codes.push(child.type_code);
            }
            value.attribute_type = AttributeType::union(members);
            value.type_code = format!("AttributeType::union(vec![{}])", codes.join(", "));
        }
        TypeOverride::LocalEnum {
            namespace,
            name,
            values,
        } => {
            let identity = carina_core::schema::enum_identity(name, Some(namespace));
            anyhow::ensure!(
                matches!(
                    value.attribute_type.raw_shape(),
                    RawShape::String { .. } | RawShape::Enum { .. }
                ),
                "local enum requires a string carrier"
            );
            let source = match value.attribute_type.raw_shape() {
                RawShape::Enum { values, .. } => values,
                RawShape::String { .. } => None,
                _ => unreachable!(),
            };
            let selected = validate_allowed_values(
                &value.attribute_type,
                source,
                values,
                &value.validated_input_constraints,
            )?;
            let selected = selected.iter().map(String::as_str).collect::<Vec<_>>();
            let aliases = match value.attribute_type.raw_shape() {
                RawShape::Enum { dsl_aliases, .. } => dsl_aliases
                    .iter()
                    .filter(|(api, _)| selected.contains(&api.as_str()))
                    .cloned()
                    .collect::<Vec<_>>(),
                _ => selected
                    .iter()
                    .map(|v| (v.to_string(), carina_aws_types::dsl_enum_value(v)))
                    .collect(),
            };
            let aliases = aliases
                .iter()
                .map(|(api, dsl)| (api.as_str(), dsl.as_str()))
                .collect::<Vec<_>>();
            value.attribute_type = carina_aws_types::try_with_member_enum_values(
                value.attribute_type,
                identity,
                &selected,
                &aliases,
            )?;
            value.type_code = format!(
                "carina_aws_types::try_with_member_enum_values({}, carina_core::schema::enum_identity({name:?}, Some({namespace:?})), &{selected:?}, &{aliases:?}).expect({:?})",
                value.type_code,
                format!("invalid generated local enum {}.{name}", namespace)
            );
        }
        TypeOverride::IntRange { min, max } => {
            anyhow::ensure!(
                matches!(value.attribute_type.raw_shape(), RawShape::Int { .. }),
                "integer tightening requires an Int carrier"
            );
            let current = if let RawShape::Int {
                identity_layer,
                member_layer,
            } = value.attribute_type.raw_shape()
            {
                carina_core::schema::intersect_bounds(identity_layer.range, member_layer.range)
            } else {
                unreachable!()
            };
            intersect(current, Some((Some(*min), Some(*max))))?;
            value.attribute_type =
                carina_aws_types::with_int_range(value.attribute_type, Some(*min), Some(*max));
            value.type_code = format!(
                "carina_aws_types::with_int_range({}, Some({min}), Some({max}))",
                value.type_code
            );
        }
        TypeOverride::StringBounds { .. }
        | TypeOverride::RefinedString { .. }
        | TypeOverride::StringPattern(_)
        | TypeOverride::StringProfile(_)
        | TypeOverride::ListLength { .. }
        | TypeOverride::NoIdentity => {}
    }
    Ok(value)
}

fn select_allowed_values(value: &mut ValueTypeOverride, allowed: &[&str]) -> Result<()> {
    let RawShape::Enum { values, .. } = value.attribute_type.raw_shape() else {
        anyhow::bail!("allowed-value refinement requires a named string enum")
    };
    let selected = validate_allowed_values(
        &value.attribute_type,
        values,
        allowed,
        &value.validated_input_constraints,
    )?;
    value.attribute_type = carina_aws_types::try_with_member_values(
        value.attribute_type.clone(),
        &selected.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    Ok(())
}

/// Additional patterns are portable slot rules, but finite types must satisfy
/// them before publication just as they must satisfy the Smithy type policy.
pub(super) fn validate_added_patterns(
    value: &str,
    constraints: &[super::ValidatedInputConstraint],
) -> Result<()> {
    for constraint in constraints {
        if let super::SlotConstraint::Pattern(pattern) = &constraint.constraint {
            anyhow::ensure!(
                pattern.is_match(value),
                "enum/override value {value:?} is rejected by added pattern {:?}",
                pattern.as_ref().as_str()
            );
        }
    }
    Ok(())
}

fn validate_closed_patterns(value: &ValueTypeOverride) -> Result<()> {
    match value.attribute_type.raw_shape() {
        RawShape::Enum {
            values: Some(values),
            ..
        } => {
            for candidate in values {
                validate_added_patterns(candidate, &value.validated_input_constraints)?;
            }
        }
        RawShape::Union(members) => {
            for member in members {
                if let RawShape::Enum {
                    values: Some(values),
                    ..
                } = member.raw_shape()
                {
                    for candidate in values {
                        validate_added_patterns(candidate, &value.validated_input_constraints)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_allowed_values(
    base: &AttributeType,
    source: Option<&[String]>,
    allowed: &[&str],
    constraints: &[super::ValidatedInputConstraint],
) -> Result<Vec<String>> {
    anyhow::ensure!(!allowed.is_empty(), "empty allowed-value intersection");
    let validator = carina_aws_types::MemberStringValidator::new(base)?;
    for value in allowed {
        validate_added_patterns(value, constraints)?;
        anyhow::ensure!(
            source.is_none_or(|values| values.iter().any(|old| old == value)),
            "override value {value:?} is absent from the Smithy/declared value set"
        );
        anyhow::ensure!(
            validator.accepts(value),
            "override value {value:?} is rejected by the derived String constraints"
        );
    }
    Ok(allowed.iter().map(|value| (*value).to_owned()).collect())
}
