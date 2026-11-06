//! Provider-independent attribute type wording and exhaustive shape traversal.

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::hash::Hash;

use anyhow::{Result, bail};
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
#[derive(Clone)]
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
        values: &'a [String],
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
    /// Reserve a target and queue its declaration. The page registry caches the
    /// returned link before the caller renders the queued declaration's children.
    /// The page registry calls this hook at most once per canonical NodeId.
    fn register(
        &mut self,
        scope: &Self::Scope,
        declaration: Declaration<'a, R, Self::NodeId>,
    ) -> Result<DocLink<Self::Target>>;
}

/// Semantic constraints for builtin scalars in discovery-only providers.
/// The variant determines the builtin name. Declared scalars must be rendered
/// from their schema node through [`render_attribute_type`], which obtains the
/// name from [`TypeDocHooks::scalar_name`].
///
/// Direct scalar composition cannot accept declared identities:
///
/// ```compile_fail
/// use carina_core::schema::TypeIdentity;
/// use carina_docgen::ScalarDoc;
/// let identity = TypeIdentity::bare("Declared");
/// ScalarDoc::String {
///     identity: Some(&identity),
///     pattern: None,
///     length: None,
///     validate: false,
///     to_dsl: None,
/// };
/// ```
/// ```compile_fail
/// use carina_core::schema::TypeIdentity;
/// use carina_docgen::ScalarDoc;
/// let identity = TypeIdentity::bare("Declared");
/// ScalarDoc::Int { identity: Some(&identity), range: None, validate: false };
/// ```
/// ```compile_fail
/// use carina_core::schema::TypeIdentity;
/// use carina_docgen::ScalarDoc;
/// let identity = TypeIdentity::bare("Declared");
/// ScalarDoc::Float { identity: Some(&identity), range: None, validate: false };
/// ```
pub enum ScalarDoc<'a> {
    String {
        pattern: Option<&'a str>,
        length: Bounds<u64>,
        validate: bool,
        to_dsl: Option<&'a DslTransform>,
    },
    Int {
        range: Bounds<i64>,
        validate: bool,
    },
    Float {
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
enum NamedKind {
    Enum,
    Struct,
    Union,
    Recursive,
}

/// Supplemental provider validation prose. Shared compositors own its placement
/// and literal escaping; it cannot be appended to an arbitrary type display.
#[derive(Clone, Copy, Debug)]
pub struct ValidationNote<'a>(&'a str);

impl<'a> ValidationNote<'a> {
    pub fn new(text: &'a str) -> Self {
        Self(text)
    }

    pub(crate) fn text(self) -> &'a str {
        self.0
    }
}

/// Render a builtin scalar using shared wording. Declared identities and their
/// naming hooks are available only through [`render_attribute_type`].
///
/// ```compile_fail
/// use carina_docgen::{ScalarDoc, TypeDisplay, scalar_display};
/// let _: TypeDisplay<()> = scalar_display(ScalarDoc::Bool, |_| "ProviderBool".into(), None);
/// ```
pub fn scalar_display<K>(
    scalar: ScalarDoc<'_>,
    note: Option<ValidationNote<'_>>,
) -> TypeDisplay<K> {
    scalar_display_with_name(scalar, None, note)
}

fn scalar_display_with_name<K>(
    scalar: ScalarDoc<'_>,
    name: Option<String>,
    note: Option<ValidationNote<'_>>,
) -> TypeDisplay<K> {
    scalar_display_with_constraints(scalar, name, Vec::new(), note)
}

fn scalar_display_with_constraints<K>(
    scalar: ScalarDoc<'_>,
    name: Option<String>,
    mut constraints: Vec<String>,
    note: Option<ValidationNote<'_>>,
) -> TypeDisplay<K> {
    let (builtin_name, refinements) = match scalar {
        ScalarDoc::String {
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
            ("String", constraints)
        }
        ScalarDoc::Int { range, validate } => ("Int", number_constraints(range, validate)),
        ScalarDoc::Float { range, validate } => ("Float", number_constraints(range, validate)),
        ScalarDoc::Bool => ("Bool", Vec::new()),
        ScalarDoc::Duration => ("Duration", Vec::new()),
    };
    constraints.extend(refinements);
    let name = name.unwrap_or_else(|| builtin_name.to_string());
    TypeDisplay::code(if constraints.is_empty() {
        name
    } else {
        format!("{name} ({})", constraints.join(", "))
    })
    .with_note(note)
}

pub fn list_display<K>(
    item: TypeDisplay<K>,
    ordered: bool,
    constraints: ListConstraints,
    note: Option<ValidationNote<'_>>,
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
    .with_note(note)
}

pub fn map_display<K>(
    key: TypeDisplay<K>,
    value: TypeDisplay<K>,
    constraints: MapConstraints,
    note: Option<ValidationNote<'_>>,
) -> TypeDisplay<K> {
    let map = TypeDisplay::sequence([
        TypeDisplay::text("Map<"),
        key,
        TypeDisplay::text(", "),
        value,
        TypeDisplay::text(">"),
    ]);
    let display = match constraints.length {
        Some(length) => TypeDisplay::sequence([
            map,
            TypeDisplay::text(format!(" (entries: {})", bounds(length))),
        ]),
        None => map,
    };
    display.with_note(note)
}

#[derive(Clone)]
struct RegisteredLink<K> {
    kind: NamedKind,
    link: DocLink<K>,
}

fn named_display<K>(registered: RegisteredLink<K>) -> TypeDisplay<K> {
    let DocLink { target, label } = registered.link;
    let label = match registered.kind {
        NamedKind::Enum => label,
        NamedKind::Struct => format!("Struct({label})"),
        NamedKind::Union => format!("Union({label})"),
        NamedKind::Recursive => format!("Recursive type ({label})"),
    };
    TypeDisplay::link(target, label)
}

/// Declaration registration state for one page. Keep this registry for every
/// render call on that page, including calls made to render declaration fields.
/// Node IDs must remain canonical and must not be reused for different nodes.
///
/// The registry fixes each link's wording from the declaration kind passed to
/// the hook and memoizes it before any queued children are rendered. Providers
/// cannot select a different kind when reusing a registered target.
/// Named wording cannot be selected separately from its declaration:
///
/// ```compile_fail
/// use carina_docgen::{NamedKind, named_display};
/// ```
pub struct TypeRegistry<N, K> {
    links: HashMap<N, RegisteredLink<K>>,
}

impl<N, K> Default for TypeRegistry<N, K> {
    fn default() -> Self {
        Self {
            links: HashMap::new(),
        }
    }
}

impl<N: Clone + Eq + Hash, K: Clone> TypeRegistry<N, K> {
    fn register<'a, R: PatternForm, H: TypeDocHooks<'a, R, NodeId = N, Target = K>>(
        &mut self,
        scope: &H::Scope,
        declaration: Declaration<'a, R, N>,
        hooks: &mut H,
    ) -> Result<TypeDisplay<K>> {
        if let Some(link) = self.links.get(&declaration.id) {
            return Ok(named_display(link.clone()));
        }
        let kind = match &declaration.kind {
            DeclarationKind::Enum { .. } => NamedKind::Enum,
            DeclarationKind::Struct { .. } => NamedKind::Struct,
            DeclarationKind::Union { .. } => NamedKind::Union,
            DeclarationKind::Recursive => NamedKind::Recursive,
        };
        let id = declaration.id.clone();
        let link = RegisteredLink {
            kind,
            link: hooks.register(scope, declaration)?,
        };
        self.links.insert(id, link.clone());
        Ok(named_display(link))
    }
}

/// Compose a type description without rendering any Markdown. Closed enums
/// and compound declarations register definitions; open enums use scalar names
/// and constraints.
/// Inline nodes recurse here with a shared active set.
pub fn render_attribute_type<'a, R: PatternForm, H: TypeDocHooks<'a, R>>(
    ty: &'a PatternAttributeType<R>,
    scope: &H::Scope,
    hooks: &mut H,
    registry: &mut TypeRegistry<H::NodeId, H::Target>,
    note: Option<ValidationNote<'_>>,
) -> Result<TypeDisplay<H::Target>> {
    Ok(walk(ty, scope, hooks, registry, &mut HashSet::new())?.with_note(note))
}

fn walk<'a, R: PatternForm, H: TypeDocHooks<'a, R>>(
    ty: &'a PatternAttributeType<R>,
    scope: &H::Scope,
    hooks: &mut H,
    registry: &mut TypeRegistry<H::NodeId, H::Target>,
    active: &mut HashSet<H::NodeId>,
) -> Result<TypeDisplay<H::Target>> {
    let Resolved { id, ty } = hooks.resolve(ty)?;
    if active.contains(&id) {
        return registry.register(
            scope,
            Declaration {
                id,
                ty,
                kind: DeclarationKind::Recursive,
            },
            hooks,
        );
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
        } => scalar_display_with_name(
            ScalarDoc::String {
                pattern,
                length,
                validate: false,
                to_dsl: None,
            },
            Some(hooks.scalar_name(identity)),
            None,
        ),
        RawShape::String {
            identity,
            pattern,
            length,
            validate,
            to_dsl,
        } => scalar_display_with_name(
            ScalarDoc::String {
                pattern,
                length,
                validate: validate.is_some(),
                to_dsl,
            },
            identity.map(|identity| hooks.scalar_name(identity)),
            None,
        ),
        RawShape::Int {
            identity,
            range,
            validate,
        } => scalar_display_with_name(
            ScalarDoc::Int {
                range,
                validate: validate.is_some(),
            },
            identity.map(|identity| hooks.scalar_name(identity)),
            None,
        ),
        RawShape::Float {
            identity,
            range,
            validate,
        } => scalar_display_with_name(
            ScalarDoc::Float {
                range,
                validate: validate.is_some(),
            },
            identity.map(|identity| hooks.scalar_name(identity)),
            None,
        ),
        RawShape::Bool => scalar_display(ScalarDoc::Bool, None),
        RawShape::Duration => scalar_display(ScalarDoc::Duration, None),
        RawShape::Enum {
            identity,
            base,
            values,
            dsl_aliases,
            validate,
            to_dsl,
        } => match values {
            None => {
                let constraints = match base.raw_shape() {
                    // The enum owns the name, validator, and DSL transform;
                    // its base contributes only structural refinements.
                    RawShape::String {
                        identity: _,
                        pattern,
                        length,
                        validate: _,
                        to_dsl: _,
                    }
                    | RawShape::Reference {
                        identity: _,
                        pattern,
                        length,
                    } => string_constraints(pattern, length),
                    RawShape::Int {
                        identity: _,
                        range,
                        validate: _,
                    } => number_constraints(range, false),
                    RawShape::Float {
                        identity: _,
                        range,
                        validate: _,
                    } => number_constraints(range, false),
                    // Bool/Duration are not refinable scalars in schema admission.
                    // Unsupported bases are rejected without rendering their metadata.
                    RawShape::Bool
                    | RawShape::Duration
                    | RawShape::Struct {
                        name: _,
                        fields: _,
                        field_groups: _,
                    }
                    | RawShape::List {
                        element_type: _,
                        ordered: _,
                        constraints: _,
                    }
                    | RawShape::Map {
                        key: _,
                        value: _,
                        constraints: _,
                    }
                    | RawShape::Union(_)
                    | RawShape::Enum {
                        identity: _,
                        base: _,
                        values: _,
                        dsl_aliases: _,
                        validate: _,
                        to_dsl: _,
                    }
                    | RawShape::Ref(_)
                    | RawShape::UnloadedSchemaType(_) => bail!(
                        "open enum `{identity}` must have a direct String, Reference, Int, or Float base"
                    ),
                };
                scalar_display_with_constraints(
                    ScalarDoc::String {
                        pattern: None,
                        length: None,
                        validate: validate.is_some(),
                        to_dsl,
                    },
                    Some(hooks.scalar_name(identity)),
                    constraints,
                    None,
                )
            }
            Some(values) => registry.register(
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
                hooks,
            )?,
        },
        RawShape::Struct {
            name,
            fields,
            field_groups,
        } => registry.register(
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
            hooks,
        )?,
        RawShape::Union(members) => registry.register(
            scope,
            Declaration {
                id: id.clone(),
                ty,
                kind: DeclarationKind::Union { members },
            },
            hooks,
        )?,
        RawShape::List {
            element_type,
            ordered,
            constraints,
        } => {
            let child = hooks.child_scope(scope, TypeEdge::Item);
            list_display(
                walk(element_type, &child, hooks, registry, active)?,
                ordered,
                constraints.into(),
                None,
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
                walk(key, &key_scope, hooks, registry, active)?,
                walk(value, &value_scope, hooks, registry, active)?,
                constraints.into(),
                None,
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
    if let Some(range) = range.filter(|(min, max)| min.is_some() || max.is_some()) {
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
        if table {
            return Ok(String::new());
        }
        bail!("empty literal cannot be represented by a Markdown code span");
    }
    let span = |code: &str| {
        let fence = "`".repeat(code.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
        let padding = if code.starts_with('`')
            || code.ends_with('`')
            || (code.starts_with(' ') && code.ends_with(' ') && !code.chars().all(|c| c == ' '))
        {
            " "
        } else {
            ""
        };
        format!("{fence}{padding}{code}{padding}{fence}")
    };
    let mut markdown = String::new();
    let mut code = String::new();
    let mut backslashes = 0_usize;
    for character in visible_code_literal(literal).chars() {
        match character {
            '|' if table => {
                if !backslashes.is_multiple_of(2) {
                    // GFM removes a backslash from odd runs before pipes. Split
                    // before the pipe; an invisible comment keeps the adjacent
                    // code spans' backtick delimiters from touching.
                    markdown.push_str(&span(&code));
                    markdown.push_str("<!---->");
                    code.clear();
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
    markdown.push_str(&span(&code));
    crate::check_page(&markdown)?;
    Ok(markdown)
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
