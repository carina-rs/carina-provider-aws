use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use carina_core::FiniteF64;
use carina_core::schema::{
    AttributeType, DslTransform, FieldGroup, ListElements, PatternAttributeType, PatternForm,
    RawShape, StructField, TypeIdentity, ValidatedConfigSchema,
};
use carina_docgen::{
    Declaration, DeclarationKind, DocLink, Heading, PageBuilder, Resolved, TypeDocHooks, TypeEdge,
    TypeRegistry, render_attribute_type, render_field_groups,
};

struct Hooks<'a, R: PatternForm> {
    defs: HashMap<String, &'a PatternAttributeType<R>>,
    declarations: Vec<Declaration<'a, R, usize>>,
}

impl<R: PatternForm> Default for Hooks<'_, R> {
    fn default() -> Self {
        Self {
            defs: HashMap::new(),
            declarations: Vec::new(),
        }
    }
}

impl<'a, R: PatternForm> TypeDocHooks<'a, R> for Hooks<'a, R> {
    type NodeId = usize;
    type Target = String;
    type Scope = String;

    fn resolve(&self, mut ty: &'a PatternAttributeType<R>) -> Result<Resolved<'a, R, usize>> {
        let mut aliases = HashSet::new();
        while let RawShape::Ref(name) = ty.raw_shape() {
            if !aliases.insert(name) {
                bail!("alias cycle: {name}");
            }
            ty = self
                .defs
                .get(name)
                .copied()
                .with_context(|| format!("missing definition: {name}"))?;
        }
        Ok(Resolved {
            id: ty as *const _ as usize,
            ty,
        })
    }

    fn scalar_name(&self, identity: &TypeIdentity) -> String {
        identity.to_string()
    }

    fn child_scope(&self, parent: &String, edge: TypeEdge<'_>) -> String {
        match edge {
            TypeEdge::Field(name) => format!("{parent}.{name}"),
            TypeEdge::Item => format!("{parent}[]"),
            TypeEdge::Key => format!("{parent}.key"),
            TypeEdge::Value => format!("{parent}.value"),
            TypeEdge::Member(index) => format!("{parent}.{index}"),
        }
    }

    fn register(
        &mut self,
        scope: &String,
        declaration: Declaration<'a, R, usize>,
    ) -> Result<DocLink<String>> {
        let label = match &declaration.kind {
            DeclarationKind::Enum { identity, .. } => identity.kind.clone(),
            DeclarationKind::Struct { name, .. } => name.to_string(),
            DeclarationKind::Union { .. } => "Choice".into(),
            DeclarationKind::Recursive => self
                .defs
                .iter()
                .find(|(_, ty)| std::ptr::eq(**ty, declaration.ty))
                .expect("recursive fixture has a canonical alias")
                .0
                .clone(),
        };
        self.declarations.push(declaration);
        Ok(DocLink {
            target: scope.clone(),
            label,
        })
    }
}

fn page<'a, R: PatternForm>(
    ty: &'a PatternAttributeType<R>,
    hooks: &mut Hooks<'a, R>,
) -> Result<String> {
    let display = render_attribute_type(
        ty,
        &"Root".into(),
        hooks,
        &mut TypeRegistry::default(),
        None,
    )?;
    let mut page = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    page.table(["Type"], [[display.table_cell()]])?;
    // Test hooks use the scope as the target; these fixtures each register one declaration.
    if !hooks.declarations.is_empty() {
        let target = if matches!(hooks.declarations[0].kind, DeclarationKind::Recursive) {
            "Root[]"
        } else if matches!(ty.raw_shape(), RawShape::Map { .. }) {
            "Root.value"
        } else {
            "Root"
        };
        page.target_heading(target.to_string(), Heading::plain(2, "Definition"))?;
    }
    Ok(page.finish()?)
}

#[test]
fn maps_preserve_entry_bounds_constrained_keys_and_linked_values() {
    for (length, suffix) in [
        (Some((Some(2), Some(5))), " (entries: 2..=5)"),
        (Some((Some(2), None)), " (entries: 2..)"),
        (Some((None, Some(5))), " (entries: ..=5)"),
        (None, ""),
        (Some((None, None)), " (entries: ..)"),
    ] {
        let ty = AttributeType::refined_map(
            AttributeType::refined_string(
                None,
                Some("a|`b".into()),
                Some((Some(1), Some(8))),
                None,
            ),
            AttributeType::struct_("X", vec![]),
            length,
        );
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        let inline = format!(
            "Map\\<``String (pattern: a|`b, len: 1..=8)``, [Struct(X)](#definition)\\>{suffix}"
        );
        assert!(markdown.starts_with(&inline), "{markdown}");
        assert!(
            markdown.contains(&format!("| {} |", inline.replace("a|", "a\\|"))),
            "{markdown}"
        );
    }
}

#[test]
fn plain_maps_are_never_shortened() {
    let ty = AttributeType::map(AttributeType::string());
    let markdown = page(&ty, &mut Hooks::default()).unwrap();
    assert!(
        markdown.starts_with("Map\\<`String`, `String`\\>"),
        "{markdown}"
    );
}

#[test]
fn recursive_list_through_alias_uses_a_typed_recursive_link() {
    let ty = AttributeType::list(AttributeType::ref_("Alias"));
    let alias = AttributeType::ref_("Loop");
    let mut hooks = Hooks::default();
    hooks.defs.insert("Alias".into(), &alias);
    hooks.defs.insert("Loop".into(), &ty);
    let markdown = page(&ty, &mut hooks).unwrap();
    assert!(
        markdown.starts_with("List\\<[Recursive type (Loop)](#definition)\\> (ordered)"),
        "{markdown}"
    );
    assert_eq!(hooks.declarations.len(), 1);
}

#[test]
fn self_referencing_struct_fields_reuse_the_target_across_render_calls() {
    let ty = AttributeType::struct_(
        "X",
        vec![StructField::optional("next", AttributeType::ref_("X")).build()],
    );
    let mut hooks = Hooks::default();
    hooks.defs.insert("X".into(), &ty);
    let mut page = PageBuilder::new();
    let mut registry = TypeRegistry::default();
    render_attribute_type(&ty, &"Root".into(), &mut hooks, &mut registry, None)
        .unwrap()
        .write_to(&mut page);
    page.push_str("\n\n");
    // The hook only queues definitions. It deliberately has no target cache.
    let mut index = 0;
    while index < hooks.declarations.len() {
        assert!(
            index < 4,
            "registration must terminate for a self reference"
        );
        let DeclarationKind::Struct { fields, .. } = &hooks.declarations[index].kind else {
            panic!("struct definition");
        };
        let field_type = fields[0].field_type();
        render_attribute_type(
            field_type,
            &"Root.next".into(),
            &mut hooks,
            &mut registry,
            None,
        )
        .unwrap()
        .write_to(&mut page);
        page.push_str("\n\n");
        index += 1;
    }
    assert_eq!(hooks.declarations.len(), 1);
    page.target_heading("Root".to_string(), Heading::plain(2, "X"))
        .unwrap();
    assert_eq!(
        page.finish().unwrap(),
        "[Struct(X)](#x)\n\n[Struct(X)](#x)\n\n## X\n\n"
    );
}

#[test]
fn inline_alias_cycles_keep_recursive_wording_across_render_calls() {
    let ty = AttributeType::list(AttributeType::ref_("X"));
    let mut hooks = Hooks::default();
    hooks.defs.insert("X".into(), &ty);
    let mut registry = TypeRegistry::default();
    let mut page = PageBuilder::new();
    for scope in ["Root", "Other"] {
        render_attribute_type(&ty, &scope.into(), &mut hooks, &mut registry, None)
            .unwrap()
            .write_to(&mut page);
        page.push_str("\n\n");
    }
    assert_eq!(hooks.declarations.len(), 1);
    page.target_heading("Root[]".to_string(), Heading::plain(2, "X"))
        .unwrap();
    assert_eq!(
        page.finish().unwrap(),
        "List\\<[Recursive type (X)](#x)\\> (ordered)\n\n".repeat(2) + "## X\n\n"
    );
}

#[test]
fn numeric_ranges_without_bounds_are_not_constraints() {
    for ty in [
        AttributeType::refined_int(None, Some((None, None))),
        AttributeType::refined_float(None, Some((None, None))),
    ] {
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        assert!(!markdown.contains("range:"), "{markdown}");
    }
}

#[test]
fn unions_register_members_and_return_a_link() {
    let ty = AttributeType::union(vec![AttributeType::string(), AttributeType::bool()]);
    let mut hooks = Hooks::default();
    let markdown = page(&ty, &mut hooks).unwrap();
    assert!(
        markdown.starts_with("[Union(Choice)](#definition)"),
        "{markdown}"
    );
    let DeclarationKind::Union { members } = &hooks.declarations[0].kind else {
        panic!("union metadata");
    };
    assert_eq!(members.len(), 2);
    for (member, expected) in members.iter().zip(["`String`", "`Bool`"]) {
        assert!(
            page(member, &mut Hooks::default())
                .unwrap()
                .starts_with(expected)
        );
    }
}

#[test]
fn lists_render_item_bounds_uniqueness_and_order() {
    for (ordered, elements, length, suffix) in [
        (true, ListElements::AllowDuplicates, None, "ordered"),
        (
            false,
            ListElements::AllowDuplicates,
            Some((None, Some(8))),
            "items: ..=8, unordered",
        ),
        (
            true,
            ListElements::Unique,
            Some((Some(1), None)),
            "items: 1.., unique items, ordered",
        ),
        (
            false,
            ListElements::Unique,
            Some((Some(1), Some(8))),
            "items: 1..=8, unique items, unordered",
        ),
    ] {
        let ty = AttributeType::refined_list(AttributeType::bool(), ordered, length, elements);
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        assert!(
            markdown.starts_with(&format!("List\\<`Bool`\\> ({suffix})")),
            "{markdown}"
        );
    }
}

#[test]
fn scalars_render_all_refinements_in_github_order() {
    let validator = Arc::new(|_: &carina_core::resource::Value| Ok(()));
    let cases = [
        (
            AttributeType::reference(
                TypeIdentity::bare("Id"),
                Some("^id$".into()),
                Some((Some(1), Some(8))),
            ),
            "Id (pattern: ^id$, len: 1..=8)",
        ),
        (
            AttributeType::refined_string(None, Some("^a$".into()), None, None),
            "String (pattern: ^a$)",
        ),
        (
            AttributeType::refined_string(None, None, Some((None, Some(8))), None),
            "String (len: ..=8)",
        ),
        (
            AttributeType::refined_string_with_validator(
                TypeIdentity::bare("N"),
                Some("^a$".into()),
                Some((Some(1), None)),
                validator.clone(),
                Some(DslTransform::HyphenToUnderscore),
            ),
            "N (pattern: ^a$, len: 1.., custom validator, DSL transform: HyphenToUnderscore)",
        ),
        (
            AttributeType::refined_int_with_validator(
                TypeIdentity::bare("I"),
                Some((Some(-1), Some(8))),
                validator.clone(),
            ),
            "I (range: -1..=8, custom validator)",
        ),
        (
            AttributeType::refined_int(None, Some((Some(1), None))),
            "Int (range: 1..)",
        ),
        (
            AttributeType::refined_int(None, Some((None, Some(8)))),
            "Int (range: ..=8)",
        ),
        (
            AttributeType::refined_float_with_validator(
                TypeIdentity::bare("F"),
                Some((Some(FiniteF64::new(0.5).unwrap()), None)),
                validator,
            ),
            "F (range: 0.5.., custom validator)",
        ),
        (AttributeType::float(), "Float"),
        (AttributeType::bool(), "Bool"),
        (AttributeType::duration(), "Duration"),
    ];
    for (ty, expected) in cases {
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        assert!(markdown.starts_with(&format!("`{expected}`")), "{markdown}");
        assert!(
            markdown.contains(&format!("| `{expected}` |")),
            "{markdown}"
        );
    }
}

#[test]
fn enum_registration_preserves_closed_values_base_aliases_validator_and_transform() {
    let transform = DslTransform::Identity;
    let ty = AttributeType::enum_with_base(
        TypeIdentity::bare("E"),
        AttributeType::int(),
        Some(vec!["a-b".into()]),
        vec![("a-b".into(), "a_b".into())],
        Some(Arc::new(|_| Ok(()))),
        Some(transform.clone()),
    );
    let mut hooks = Hooks::default();
    assert!(
        page(&ty, &mut hooks)
            .unwrap()
            .starts_with("[E](#definition)")
    );
    let DeclarationKind::Enum {
        identity,
        base,
        values,
        dsl_aliases,
        validate,
        to_dsl,
    } = &hooks.declarations[0].kind
    else {
        panic!("enum metadata");
    };
    assert_eq!(identity.kind, "E");
    assert!(matches!(base.raw_shape(), RawShape::Int { .. }));
    assert_eq!(*values, ["a-b".to_string()].as_slice());
    assert_eq!(dsl_aliases[0], ("a-b".into(), "a_b".into()));
    assert!(validate.is_some());
    assert_eq!(*to_dsl, Some(&transform));
}

#[test]
fn open_enums_use_the_hooks_scalar_name_without_registering_a_definition() {
    let ty = AttributeType::enum_with_base(
        TypeIdentity::new(Some("provider"), ["Scope"], "Open"),
        AttributeType::string(),
        None,
        Vec::new(),
        Some(Arc::new(|_| Ok(()))),
        Some(DslTransform::Identity),
    );
    let mut hooks = Hooks::default();
    let markdown = page(&ty, &mut hooks).unwrap();
    assert_eq!(
        markdown,
        "`provider.Scope.Open`\n\n| Type |\n|------|\n| `provider.Scope.Open` |\n"
    );
    assert!(hooks.declarations.is_empty());

    let config = ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
    let mut hooks = Hooks::default();
    assert_eq!(
        page(config.parts().get("value").unwrap(), &mut hooks).unwrap(),
        markdown
    );
    assert!(hooks.declarations.is_empty());
}

#[test]
fn compiled_shapes_use_the_same_renderer() {
    let config = ValidatedConfigSchema::validate(BTreeMap::from([(
        "value".into(),
        AttributeType::refined_map(
            AttributeType::refined_string(None, Some("^a$".into()), None, None),
            AttributeType::bool(),
            Some((Some(1), Some(4))),
        ),
    )]))
    .unwrap();
    let ty = config.parts().get("value").unwrap();
    let markdown = page(ty, &mut Hooks::default()).unwrap();
    assert!(
        markdown.starts_with("Map\\<`String (pattern: ^a$)`, `Bool`\\> (entries: 1..=4)"),
        "{markdown}"
    );
}

#[test]
fn nesting_limit_missing_aliases_and_alias_only_cycles_are_errors() {
    let mut ty = AttributeType::bool();
    for _ in 0..64 {
        ty = AttributeType::list(ty);
    }
    assert!(
        page(&ty, &mut Hooks::default())
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
    let missing = AttributeType::ref_("Missing");
    assert!(
        page(&missing, &mut Hooks::default())
            .unwrap_err()
            .to_string()
            .contains("missing definition")
    );
    let cycle = AttributeType::ref_("Cycle");
    let mut hooks = Hooks::default();
    hooks.defs.insert("Cycle".into(), &cycle);
    assert!(
        page(&cycle, &mut hooks)
            .unwrap_err()
            .to_string()
            .contains("alias cycle")
    );
}

#[test]
fn scalar_code_spans_preserve_controls_backticks_and_table_pipes() {
    let ty = AttributeType::refined_string(None, Some("a|`b\n\t\r".into()), None, None);
    let markdown = page(&ty, &mut Hooks::default()).unwrap();
    assert!(
        markdown.starts_with("``String (pattern: a|`b\\n\\t\\u{d})``"),
        "{markdown}"
    );
    assert!(
        markdown.contains("| ``String (pattern: a\\|`b\\n\\t\\u{d})`` |"),
        "{markdown}"
    );
    let odd_slash = AttributeType::refined_string(None, Some(r"a\|b".into()), None, None);
    let markdown = page(&odd_slash, &mut Hooks::default()).unwrap();
    assert!(
        markdown.contains(r"| `String (pattern: a\`<!---->`\|b)` |"),
        "{markdown}"
    );
}

#[test]
fn field_group_sentences_share_cardinality_and_safe_code_spans() {
    let groups = [
        FieldGroup::exactly_one(["a", "b"]).unwrap(),
        FieldGroup::at_least_one(["a", "b"]).unwrap(),
        FieldGroup::at_most_one(["a", "b"]).unwrap(),
    ];
    let mut page: PageBuilder<()> = PageBuilder::new();
    render_field_groups(&mut page, &groups);
    assert_eq!(
        page.finish().unwrap(),
        "**Field groups:**\n\n- Exactly one of: `a`, `b`.\n- At least one of: `a`, `b`.\n- At most one of: `a`, `b`.\n\n"
    );
}
