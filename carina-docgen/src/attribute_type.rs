//! Provider-independent attribute type wording and exhaustive shape traversal.

use std::collections::HashSet;
use std::fmt::{Debug, Display};
use std::hash::Hash;

use anyhow::{Context, Result, bail};
use carina_core::FiniteF64;
use carina_core::schema::{
    CustomValidator, DslTransform, FieldGroup, FieldGroupKind, ListElements, PatternAttributeType,
    PatternForm, PatternStructField, RawShape, TypeIdentity,
};

use crate::{PageBuilder, TypeDisplay};

pub type Bounds<T> = Option<(Option<T>, Option<T>)>;

/// An alias-resolved node and its canonical identity, stable across Ref edges.
pub struct Resolved<'a, R: PatternForm, N> {
    pub id: N,
    pub ty: &'a PatternAttributeType<R>,
}

/// A provider-owned target and an unprefixed, literal label (never Markdown).
pub struct DocLink<K> {
    pub target: K,
    pub label: String,
}

#[derive(Clone, Copy, Debug)]
pub enum TypeEdge<'a> {
    Field(&'a str),
    Item,
    Key,
    Value,
    Member(usize),
}

/// Metadata for a separately rendered declaration. Children remain typed schema
/// nodes so providers can keep their existing definition layout and prose.
pub enum DeclarationKind<'a, R: PatternForm> {
    Enum {
        identity: &'a TypeIdentity,
        base: &'a PatternAttributeType<R>,
        values: Option<&'a [String]>,
        dsl_aliases: &'a [(String, String)],
        validate: Option<&'a CustomValidator>,
        to_dsl: Option<&'a DslTransform>,
    },
    Struct {
        name: &'a str,
        fields: &'a [PatternStructField<R>],
        field_groups: &'a [FieldGroup],
    },
    Union {
        members: &'a [PatternAttributeType<R>],
    },
    Recursive,
}

pub struct Declaration<'a, R: PatternForm, N> {
    pub id: N,
    pub ty: &'a PatternAttributeType<R>,
    pub kind: DeclarationKind<'a, R>,
}

/// Provider-specific identity, scope, and declaration storage only.
pub trait TypeDocHooks<'a, R: PatternForm> {
    type NodeId: Clone + Eq + Hash;
    type Target: Clone + Eq + Hash + Debug;
    type Scope: Clone;

    /// Resolve aliases, rejecting missing definitions and alias-only cycles.
    fn resolve(&self, ty: &'a PatternAttributeType<R>) -> Result<Resolved<'a, R, Self::NodeId>>;
    fn scalar_name(&self, identity: &TypeIdentity) -> String;
    fn child_scope(&self, parent: &Self::Scope, edge: TypeEdge<'_>) -> Self::Scope;
    /// Reserve a target before processing its children. Implementations may
    /// queue declarations or render them immediately after reserving the target.
    /// Repeated registration must reuse the declaration's existing target.
    fn register(
        &mut self,
        scope: &Self::Scope,
        declaration: Declaration<'a, R, Self::NodeId>,
    ) -> Result<DocLink<Self::Target>>;
}

/// Semantic scalar data shared by schema trees and discovery-only providers.
pub enum ScalarDoc<'a> {
    String {
        name: &'a str,
        pattern: Option<&'a str>,
        length: Bounds<u64>,
        validate: bool,
        to_dsl: Option<&'a DslTransform>,
    },
    Int {
        name: &'a str,
        range: Bounds<i64>,
        validate: bool,
    },
    Float {
        name: &'a str,
        range: Bounds<FiniteF64>,
        validate: bool,
    },
    Bool,
    Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct ListConstraints {
    pub length: Bounds<u64>,
    pub elements: ListElements,
}

impl From<carina_core::schema::ListConstraints> for ListConstraints {
    fn from(constraints: carina_core::schema::ListConstraints) -> Self {
        Self {
            length: constraints.length(),
            elements: constraints.elements(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MapConstraints {
    pub length: Bounds<u64>,
}

impl From<carina_core::schema::MapConstraints> for MapConstraints {
    fn from(constraints: carina_core::schema::MapConstraints) -> Self {
        Self {
            length: constraints.length(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NamedKind {
    Enum,
    Struct,
    Union,
    Recursive,
}

pub fn scalar_display<K>(scalar: ScalarDoc<'_>) -> TypeDisplay<K> {
    let (name, constraints) = match scalar {
        ScalarDoc::String {
            name,
            pattern,
            length,
            validate,
            to_dsl,
        } => {
            let mut constraints = string_constraints(pattern, length);
            if validate {
                constraints.push("custom validator".into());
            }
            if let Some(transform) = to_dsl {
                constraints.push(format!("DSL transform: {transform:?}"));
            }
            (name, constraints)
        }
        ScalarDoc::Int {
            name,
            range,
            validate,
        } => (name, number_constraints(range, validate)),
        ScalarDoc::Float {
            name,
            range,
            validate,
        } => (name, number_constraints(range, validate)),
        ScalarDoc::Bool => ("Bool", Vec::new()),
        ScalarDoc::Duration => ("Duration", Vec::new()),
    };
    TypeDisplay::code(if constraints.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({})", constraints.join(", "))
    })
}

pub fn list_display<K>(
    item: TypeDisplay<K>,
    ordered: bool,
    constraints: ListConstraints,
) -> TypeDisplay<K> {
    let mut refinements = Vec::new();
    if let Some(length) = constraints.length {
        refinements.push(format!("items: {}", bounds(length)));
    }
    if constraints.elements == ListElements::Unique {
        refinements.push("unique items".into());
    }
    refinements.push(if ordered { "ordered" } else { "unordered" }.into());
    TypeDisplay::sequence([
        TypeDisplay::text("List<"),
        item,
        TypeDisplay::text(format!("> ({})", refinements.join(", "))),
    ])
}

pub fn map_display<K>(
    key: TypeDisplay<K>,
    value: TypeDisplay<K>,
    constraints: MapConstraints,
) -> TypeDisplay<K> {
    let map = TypeDisplay::sequence([
        TypeDisplay::text("Map<"),
        key,
        TypeDisplay::text(", "),
        value,
        TypeDisplay::text(">"),
    ]);
    match constraints.length {
        Some(length) => TypeDisplay::sequence([
            map,
            TypeDisplay::text(format!(" (entries: {})", bounds(length))),
        ]),
        None => map,
    }
}

pub fn named_display<K>(kind: NamedKind, link: DocLink<K>) -> TypeDisplay<K> {
    let label = match kind {
        NamedKind::Enum => link.label,
        NamedKind::Struct => format!("Struct({})", link.label),
        NamedKind::Union => format!("Union({})", link.label),
        NamedKind::Recursive => format!("Recursive type ({})", link.label),
    };
    TypeDisplay::link(link.target, label)
}

/// Compose a type description without rendering any Markdown. Named nodes
/// register a definition; inline nodes recurse here with a shared active set.
pub fn render_attribute_type<'a, R: PatternForm, H: TypeDocHooks<'a, R>>(
    ty: &'a PatternAttributeType<R>,
    scope: &H::Scope,
    hooks: &mut H,
) -> Result<TypeDisplay<H::Target>> {
    walk(ty, scope, hooks, &mut HashSet::new())
}

fn walk<'a, R: PatternForm, H: TypeDocHooks<'a, R>>(
    ty: &'a PatternAttributeType<R>,
    scope: &H::Scope,
    hooks: &mut H,
    active: &mut HashSet<H::NodeId>,
) -> Result<TypeDisplay<H::Target>> {
    let Resolved { id, ty } = hooks.resolve(ty)?;
    if active.contains(&id) {
        return Ok(named_display(
            NamedKind::Recursive,
            hooks.register(
                scope,
                Declaration {
                    id,
                    ty,
                    kind: DeclarationKind::Recursive,
                },
            )?,
        ));
    }
    if active.len() >= 64 {
        bail!("type nesting exceeds documentation limit (64)");
    }
    active.insert(id.clone());
    // Deliberately bind every field: additions to RawShape must break this match.
    let display = match ty.raw_shape() {
        RawShape::Reference {
            identity,
            pattern,
            length,
        } => scalar_display(ScalarDoc::String {
            name: &hooks.scalar_name(identity),
            pattern,
            length,
            validate: false,
            to_dsl: None,
        }),
        RawShape::String {
            identity,
            pattern,
            length,
            validate,
            to_dsl,
        } => scalar_display(ScalarDoc::String {
            name: &identity
                .map(|identity| hooks.scalar_name(identity))
                .unwrap_or_else(|| "String".into()),
            pattern,
            length,
            validate: validate.is_some(),
            to_dsl,
        }),
        RawShape::Int {
            identity,
            range,
            validate,
        } => scalar_display(ScalarDoc::Int {
            name: &identity
                .map(|identity| hooks.scalar_name(identity))
                .unwrap_or_else(|| "Int".into()),
            range,
            validate: validate.is_some(),
        }),
        RawShape::Float {
            identity,
            range,
            validate,
        } => scalar_display(ScalarDoc::Float {
            name: &identity
                .map(|identity| hooks.scalar_name(identity))
                .unwrap_or_else(|| "Float".into()),
            range,
            validate: validate.is_some(),
        }),
        RawShape::Bool => scalar_display(ScalarDoc::Bool),
        RawShape::Duration => scalar_display(ScalarDoc::Duration),
        RawShape::Enum {
            identity,
            base,
            values,
            dsl_aliases,
            validate,
            to_dsl,
        } => named_display(
            NamedKind::Enum,
            hooks.register(
                scope,
                Declaration {
                    id: id.clone(),
                    ty,
                    kind: DeclarationKind::Enum {
                        identity,
                        base,
                        values,
                        dsl_aliases,
                        validate,
                        to_dsl,
                    },
                },
            )?,
        ),
        RawShape::Struct {
            name,
            fields,
            field_groups,
        } => named_display(
            NamedKind::Struct,
            hooks.register(
                scope,
                Declaration {
                    id: id.clone(),
                    ty,
                    kind: DeclarationKind::Struct {
                        name,
                        fields,
                        field_groups,
                    },
                },
            )?,
        ),
        RawShape::Union(members) => named_display(
            NamedKind::Union,
            hooks.register(
                scope,
                Declaration {
                    id: id.clone(),
                    ty,
                    kind: DeclarationKind::Union { members },
                },
            )?,
        ),
        RawShape::List {
            element_type,
            ordered,
            constraints,
        } => {
            let child = hooks.child_scope(scope, TypeEdge::Item);
            list_display(
                walk(element_type, &child, hooks, active)?,
                ordered,
                constraints.into(),
            )
        }
        RawShape::Map {
            key,
            value,
            constraints,
        } => {
            let key_scope = hooks.child_scope(scope, TypeEdge::Key);
            let value_scope = hooks.child_scope(scope, TypeEdge::Value);
            map_display(
                walk(key, &key_scope, hooks, active)?,
                walk(value, &value_scope, hooks, active)?,
                constraints.into(),
            )
        }
        RawShape::UnloadedSchemaType(unloaded) => bail!("unloaded schema type: {unloaded:?}"),
        RawShape::Ref(name) => bail!("type hooks did not resolve schema reference {name}"),
    };
    active.remove(&id);
    Ok(display)
}

/// Field-group wording and layout shared by provider documentation.
pub fn render_field_groups<K: Clone + Eq + Hash + Debug>(
    page: &mut PageBuilder<K>,
    groups: &[FieldGroup],
) {
    if groups.is_empty() {
        return;
    }
    page.push_str("**Field groups:**\n\n");
    for group in groups {
        let cardinality = match group.kind() {
            FieldGroupKind::ExactlyOne => "Exactly one",
            FieldGroupKind::AtLeastOne => "At least one",
            FieldGroupKind::AtMostOne => "At most one",
        };
        page.push_str(&format!("- {cardinality} of: "));
        for (index, field) in group.fields().iter().enumerate() {
            if index > 0 {
                page.push_str(", ");
            }
            TypeDisplay::code(field).write_to(page);
        }
        page.push_str(".\n");
    }
    page.push_str("\n");
}

// These formatting rules come from carina-provider-github's doc generator.
fn bounds<T: Display>((min, max): (Option<T>, Option<T>)) -> String {
    format!(
        "{}..{}",
        min.map(|value| value.to_string()).unwrap_or_default(),
        max.map(|value| format!("={value}")).unwrap_or_default()
    )
}

fn string_constraints(pattern: Option<&str>, length: Bounds<u64>) -> Vec<String> {
    let mut constraints = Vec::new();
    if let Some(pattern) = pattern {
        constraints.push(format!("pattern: {pattern}"));
    }
    if let Some(length) = length {
        constraints.push(format!("len: {}", bounds(length)));
    }
    constraints
}

fn number_constraints<T: Display>(range: Bounds<T>, validate: bool) -> Vec<String> {
    let mut constraints = Vec::new();
    if let Some(range) = range {
        constraints.push(format!("range: {}", bounds(range)));
    }
    if validate {
        constraints.push("custom validator".into());
    }
    constraints
}

/// Literals retain their spelling in code spans. GFM removes the pipe escape
/// from table cells; controls must be visible because Markdown normalizes them.
pub(crate) fn code_span(literal: &str, table: bool) -> Result<String> {
    if literal.is_empty() {
        bail!("empty literal cannot be represented by a Markdown code span");
    }
    let mut code = String::new();
    let mut backslashes = 0;
    for character in visible_code_literal(literal).chars() {
        match character {
            '|' if table => {
                // GFM cannot round-trip a pipe preceded by an odd backslash run.
                if backslashes % 2 == 1 {
                    bail!("literal cannot round-trip in a GFM table cell: {literal:?}");
                }
                code.push_str(r"\|");
            }
            c => code.push(c),
        }
        backslashes = if character == '\\' {
            backslashes + 1
        } else {
            0
        };
    }
    let fence = "`".repeat(code.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
    let padding = if code.starts_with('`')
        || code.ends_with('`')
        || (code.starts_with(' ') && code.ends_with(' ') && !code.chars().all(|c| c == ' '))
    {
        " "
    } else {
        ""
    };
    let span = format!("{fence}{padding}{code}{padding}{fence}");
    crate::check_page(&span)
        .with_context(|| format!("literal {literal:?} rejected by carina-docgen"))?;
    Ok(span)
}

pub(crate) fn visible_code_literal(literal: &str) -> String {
    let mut visible = String::new();
    for character in literal.chars() {
        match character {
            '\n' => visible.push_str(r"\n"),
            '\t' => visible.push_str(r"\t"),
            c if c.is_control() => visible.extend(c.escape_unicode()),
            c => visible.push(c),
        }
    }
    visible
}
