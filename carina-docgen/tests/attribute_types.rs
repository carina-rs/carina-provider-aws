use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use carina_core::FiniteF64;
use carina_core::schema::{
    AttributeType, CustomValidator, DslTransform, FieldGroup, ListElements, PatternAttributeType,
    PatternForm, PatternMatch, PatternReason, PatternSpec, PatternSpecDisplay, RawShape,
    StructField, TypeIdentity, ValidatedConfigSchema,
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
                Some(carina_core::schema::PatternSpec::new(
                    "a|`b",
                    carina_core::schema::PatternMatch::Substring {
                        reason: carina_core::schema::PatternReason::new(
                            "Preserve the fixture regex search semantics.",
                        )
                        .unwrap(),
                    },
                )),
                Some((Some(1), Some(8))),
                None,
            ),
            AttributeType::struct_("X", vec![]),
            length,
        );
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        let inline = format!(
            "Map\\<``String (pattern: /a|`b/ (Substring: Preserve the fixture regex search semantics.), len: 1..=8)``, [Struct(X)](#definition)\\>{suffix}"
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
fn pattern_constraints_use_core_display_and_preserve_modes_in_table_cells() {
    let full = PatternSpec::new("^a$", PatternMatch::Full);
    let substring = PatternSpec::new(
        "^a$",
        PatternMatch::Substring {
            reason: PatternReason::new("Preserve a|b\nsearch semantics.").unwrap(),
        },
    );
    let mut constraints = Vec::new();
    for (spec, literal) in [
        (&full, "`String (pattern: /^a$/ (Full))`"),
        (
            &substring,
            r#"`String (pattern: /^a$/ (Substring: "Preserve a|b\nsearch semantics."))`"#,
        ),
    ] {
        let ty = AttributeType::refined_string(None, Some(spec.clone()), None, None);
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        let constraint = markdown.lines().next().unwrap();
        let rendered = PatternSpecDisplay::new(spec).to_string();
        let expected = format!("`String (pattern: {rendered})`");
        assert_eq!(constraint, expected);
        assert_eq!(constraint, literal);
        assert!(
            markdown.contains(&format!("| {} |", expected.replace('|', r"\|"))),
            "{markdown}"
        );
        constraints.push(constraint.to_owned());
    }
    assert_ne!(constraints[0], constraints[1]);
}

#[test]
fn scalars_render_identity_and_member_constraints_in_order() {
    let string = AttributeType::string_member(
        carina_core::schema::StringIdentityLayer::with_validator(
            TypeIdentity::bare("Name"),
            Some(PatternSpec::new("[a-z]+", PatternMatch::Full)),
            Some((Some(1), Some(64))),
            Arc::new(|_| Ok(())),
            Some(DslTransform::Identity),
        ),
        carina_core::schema::StringMemberConstraints::new(
            Some(PatternSpec::new("a.*", PatternMatch::Full)),
            Some((Some(3), Some(16))),
        ),
    );
    let int = AttributeType::int_member(
        carina_core::schema::IntIdentityLayer::new(
            Some(TypeIdentity::bare("Count")),
            Some((Some(0), Some(100))),
        ),
        carina_core::schema::IntMemberConstraints::new(Some((Some(2), Some(8)))),
    );
    for (ty, expected) in [
        (
            string,
            "identity constraints: pattern: /[a-z]+/ (Full), len: 1..=64, member constraints: pattern: /a.*/ (Full), len: 3..=16",
        ),
        (
            int,
            "identity constraints: range: 0..=100, member constraints: range: 2..=8",
        ),
    ] {
        let markdown = page(&ty, &mut Hooks::default()).unwrap();
        assert_eq!(markdown.matches(expected).count(), 2, "{markdown}");
    }
}

#[test]
fn anonymous_scalars_render_the_effective_policy_without_layer_labels() {
    let int = AttributeType::int_member(
        carina_core::schema::IntIdentityLayer::new(None, Some((Some(0), Some(100000)))),
        carina_core::schema::IntMemberConstraints::new(Some((Some(60), Some(86400)))),
    );
    let rendered = page(&int, &mut Hooks::default()).unwrap();
    assert_eq!(
        rendered.matches("Int (range: 60..=86400)").count(),
        2,
        "{rendered}"
    );
    assert!(!rendered.contains("identity constraints") && !rendered.contains("member constraints"));
    let text = AttributeType::string_member(
        carina_core::schema::StringIdentityLayer::new(
            None,
            Some(PatternSpec::new("^a.*", PatternMatch::Full)),
            Some((Some(1), Some(10))),
            None,
        ),
        carina_core::schema::StringMemberConstraints::new(
            Some(PatternSpec::new(".*z$", PatternMatch::Full)),
            Some((Some(2), Some(8))),
        ),
    );
    let rendered = page(&text, &mut Hooks::default()).unwrap();
    assert_eq!(
        rendered
            .matches("String (pattern: /^a.*/ (Full), pattern: /.*z$/ (Full), len: 2..=8)")
            .count(),
        2,
        "{rendered}"
    );
    assert!(!rendered.contains("identity constraints") && !rendered.contains("member constraints"));
}

#[test]
fn scalars_render_all_refinements_in_github_order() {
    let validator = Arc::new(|_: &carina_core::resource::Value| Ok(()));
    let cases = [
        (
            AttributeType::reference(
                TypeIdentity::bare("Id"),
                Some(carina_core::schema::PatternSpec::new(
                    "^id$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(1), Some(8))),
            ),
            "Id (pattern: /^id$/ (Full), len: 1..=8)",
        ),
        (
            AttributeType::refined_string(
                None,
                Some(carina_core::schema::PatternSpec::new(
                    "^a$",
                    carina_core::schema::PatternMatch::Full,
                )),
                None,
                None,
            ),
            "String (pattern: /^a$/ (Full))",
        ),
        (
            AttributeType::refined_string(None, None, Some((None, Some(8))), None),
            "String (len: ..=8)",
        ),
        (
            AttributeType::refined_string_with_validator(
                TypeIdentity::bare("N"),
                Some(carina_core::schema::PatternSpec::new(
                    "^a$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(1), None)),
                validator.clone(),
                Some(DslTransform::HyphenToUnderscore),
            ),
            "N (pattern: /^a$/ (Full), len: 1.., custom validator, DSL transform: HyphenToUnderscore)",
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
fn open_enums_render_custom_validators() {
    let ty = AttributeType::enum_(
        TypeIdentity::bare("Name"),
        None,
        Vec::new(),
        Some(Arc::new(|_| Ok(()))),
        None,
    );
    let mut hooks = Hooks::default();
    assert_eq!(
        page(&ty, &mut hooks).unwrap(),
        "`Name (custom validator)`\n\n| Type |\n|------|\n| `Name (custom validator)` |\n"
    );
    assert!(hooks.declarations.is_empty());
}

#[test]
fn open_enums_render_dsl_transforms() {
    let ty = AttributeType::enum_(
        TypeIdentity::bare("Name"),
        None,
        Vec::new(),
        None,
        Some(DslTransform::HyphenToUnderscore),
    );
    let mut hooks = Hooks::default();
    assert_eq!(
        page(&ty, &mut hooks).unwrap(),
        "`Name (DSL transform: HyphenToUnderscore)`\n\n| Type |\n|------|\n| `Name (DSL transform: HyphenToUnderscore)` |\n"
    );
    assert!(hooks.declarations.is_empty());
}

#[test]
fn open_enums_without_constraints_render_only_the_declared_name() {
    let ty = AttributeType::enum_(TypeIdentity::bare("Name"), None, Vec::new(), None, None);
    let mut hooks = Hooks::default();
    assert_eq!(
        page(&ty, &mut hooks).unwrap(),
        "`Name`\n\n| Type |\n|------|\n| `Name` |\n"
    );
    assert!(hooks.declarations.is_empty());
}

#[test]
fn open_enums_preserve_string_base_constraints_like_declared_strings() {
    for (pattern, length) in [
        (
            Some(carina_core::schema::PatternSpec::new(
                "^a$",
                carina_core::schema::PatternMatch::Full,
            )),
            None,
        ),
        (None, Some((Some(1), Some(8)))),
        (
            Some(carina_core::schema::PatternSpec::new(
                "^a$",
                carina_core::schema::PatternMatch::Full,
            )),
            Some((Some(1), Some(8))),
        ),
    ] {
        let identity = TypeIdentity::new(Some("provider"), ["Scope"], "Name");
        let validate = Arc::new(|_: &carina_core::resource::Value| Ok(()));
        let ty = AttributeType::enum_with_base(
            identity.clone(),
            AttributeType::refined_string(None, pattern.clone(), length, None),
            None,
            Vec::new(),
            Some(validate.clone()),
            Some(DslTransform::HyphenToUnderscore),
        );
        let scalar = AttributeType::refined_string_with_validator(
            identity,
            pattern,
            length,
            validate,
            Some(DslTransform::HyphenToUnderscore),
        );
        let mut hooks = Hooks::default();
        let markdown = page(&ty, &mut hooks).unwrap();
        assert_eq!(markdown, page(&scalar, &mut Hooks::default()).unwrap());
        assert!(hooks.declarations.is_empty());

        let config =
            ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
        assert_eq!(
            page(config.parts().get("value").unwrap(), &mut Hooks::default()).unwrap(),
            markdown
        );
    }
}

#[test]
fn open_enums_preserve_numeric_base_ranges_before_validator_and_transform() {
    for (base, range) in [
        (
            AttributeType::refined_int(None, Some((Some(-1), Some(8)))),
            "range: -1..=8, ",
        ),
        (
            AttributeType::refined_float(None, Some((Some(FiniteF64::new(0.5).unwrap()), None))),
            "range: 0.5.., ",
        ),
        (AttributeType::refined_int(None, Some((None, None))), ""),
        (AttributeType::refined_float(None, Some((None, None))), ""),
    ] {
        let ty = AttributeType::enum_with_base(
            TypeIdentity::bare("Name"),
            base,
            None,
            Vec::new(),
            Some(Arc::new(|_| Ok(()))),
            Some(DslTransform::Identity),
        );
        let expected = format!("`Name ({range}custom validator, DSL transform: Identity)`");
        let mut hooks = Hooks::default();
        let markdown = page(&ty, &mut hooks).unwrap();
        assert!(markdown.starts_with(&expected), "{markdown}");
        assert!(markdown.contains(&format!("| {expected} |")), "{markdown}");
        assert!(hooks.declarations.is_empty());
    }
}

#[test]
fn open_enums_ignore_base_callbacks_and_transforms() {
    let validate: CustomValidator = Arc::new(|_| Ok(()));
    for (base, refinements) in [
        (
            AttributeType::refined_string_with_validator(
                TypeIdentity::new(Some("provider"), ["Scope"], "Text"),
                Some(carina_core::schema::PatternSpec::new(
                    "^a$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(1), Some(8))),
                validate.clone(),
                Some(DslTransform::HyphenToUnderscore),
            ),
            "pattern: /^a$/ (Full), len: 1..=8",
        ),
        (
            AttributeType::refined_int_with_validator(
                TypeIdentity::new(Some("provider"), ["Scope"], "Integer"),
                Some((Some(-1), Some(8))),
                validate.clone(),
            ),
            "range: -1..=8",
        ),
        (
            AttributeType::refined_float_with_validator(
                TypeIdentity::new(Some("provider"), ["Scope"], "Number"),
                Some((Some(FiniteF64::new(0.5).unwrap()), None)),
                validate.clone(),
            ),
            "range: 0.5..",
        ),
    ] {
        for (enum_validate, to_dsl, metadata) in [
            (None, None, ""),
            (
                Some(validate.clone()),
                Some(DslTransform::Identity),
                ", custom validator, DSL transform: Identity",
            ),
        ] {
            let ty = AttributeType::enum_with_base(
                TypeIdentity::new(Some("provider"), ["Scope"], "Open"),
                base.clone(),
                None,
                Vec::new(),
                enum_validate,
                to_dsl,
            );
            let expected = format!("`provider.Scope.Open ({refinements}{metadata})`");
            let mut hooks = Hooks::default();
            let markdown = page(&ty, &mut hooks).unwrap();
            assert!(markdown.starts_with(&expected), "{markdown}");
            assert!(markdown.contains(&format!("| {expected} |")), "{markdown}");
            assert!(hooks.declarations.is_empty());

            let config =
                ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
            assert_eq!(
                page(config.parts().get("value").unwrap(), &mut Hooks::default()).unwrap(),
                markdown
            );
        }
    }
}

#[test]
fn open_enums_preserve_nested_enum_scalar_refinements() {
    for (base, refinements) in [
        (
            AttributeType::refined_string(
                None,
                Some(carina_core::schema::PatternSpec::new(
                    "^a$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(1), Some(8))),
                None,
            ),
            "pattern: /^a$/ (Full), len: 1..=8",
        ),
        (
            AttributeType::refined_int(None, Some((Some(-1), Some(8)))),
            "range: -1..=8",
        ),
        (
            AttributeType::refined_float(None, Some((Some(FiniteF64::new(0.5).unwrap()), None))),
            "range: 0.5..",
        ),
    ] {
        let mut base = base;
        for name in ["Inner", "Middle"] {
            base = AttributeType::enum_with_base(
                TypeIdentity::new(Some("provider"), ["Scope"], name),
                base,
                None,
                Vec::new(),
                Some(Arc::new(|_| Ok(()))),
                Some(DslTransform::HyphenToUnderscore),
            );
        }
        let ty = AttributeType::enum_with_base(
            TypeIdentity::new(Some("provider"), ["Scope"], "Open"),
            base,
            None,
            Vec::new(),
            None,
            Some(DslTransform::Identity),
        );
        let expected = format!("`provider.Scope.Open ({refinements}, DSL transform: Identity)`");
        let mut hooks = Hooks::default();
        let markdown = page(&ty, &mut hooks).unwrap();
        assert!(markdown.starts_with(&expected), "{markdown}");
        assert!(markdown.contains(&format!("| {expected} |")), "{markdown}");
        assert!(hooks.declarations.is_empty());

        let config =
            ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
        assert_eq!(
            page(config.parts().get("value").unwrap(), &mut Hooks::default()).unwrap(),
            markdown
        );
    }
}

#[test]
fn open_enums_preserve_reference_base_constraints() {
    let ty = AttributeType::enum_with_base(
        TypeIdentity::new(Some("provider"), ["Scope"], "Open"),
        AttributeType::reference(
            TypeIdentity::new(Some("provider"), ["Scope"], "Reference"),
            Some(carina_core::schema::PatternSpec::new(
                "^ref-[a-z]+$",
                carina_core::schema::PatternMatch::Full,
            )),
            Some((Some(5), Some(32))),
        ),
        None,
        Vec::new(),
        Some(Arc::new(|_| Ok(()))),
        Some(DslTransform::Identity),
    );
    let expected = "`provider.Scope.Open (pattern: /^ref-[a-z]+$/ (Full), len: 5..=32, custom validator, DSL transform: Identity)`";
    let mut hooks = Hooks::default();
    let markdown = page(&ty, &mut hooks).unwrap();
    assert!(markdown.starts_with(expected), "{markdown}");
    assert!(markdown.contains(&format!("| {expected} |")), "{markdown}");
    assert!(hooks.declarations.is_empty());

    let config = ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
    assert_eq!(
        page(config.parts().get("value").unwrap(), &mut Hooks::default()).unwrap(),
        markdown
    );
}

#[test]
fn open_enums_reject_unsupported_bases_with_the_enum_identity() {
    for base in [
        AttributeType::struct_("Record", Vec::new()),
        AttributeType::list(AttributeType::string()),
        AttributeType::map(AttributeType::string()),
        AttributeType::union(vec![AttributeType::string(), AttributeType::int()]),
        AttributeType::bool(),
        AttributeType::duration(),
        AttributeType::ref_("Alias"),
    ] {
        let base_name = base.type_name();
        let ty = AttributeType::enum_with_base(
            TypeIdentity::new(Some("provider"), ["Scope"], "Open"),
            base,
            None,
            Vec::new(),
            None,
            None,
        );
        let mut hooks = Hooks::default();
        let error = page(&ty, &mut hooks).expect_err("unsupported open-enum base must fail");
        assert_eq!(
            error.to_string(),
            format!(
                "docgen cannot document open enum `provider.Scope.Open` with base `{base_name}`"
            ),
            "base: {base_name}"
        );
        assert!(hooks.declarations.is_empty());
    }
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
        "`provider.Scope.Open (custom validator, DSL transform: Identity)`\n\n| Type |\n|------|\n| `provider.Scope.Open (custom validator, DSL transform: Identity)` |\n"
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
            AttributeType::refined_string(
                None,
                Some(carina_core::schema::PatternSpec::new(
                    "^a$",
                    carina_core::schema::PatternMatch::Full,
                )),
                None,
                None,
            ),
            AttributeType::bool(),
            Some((Some(1), Some(4))),
        ),
    )]))
    .unwrap();
    let ty = config.parts().get("value").unwrap();
    let markdown = page(ty, &mut Hooks::default()).unwrap();
    assert!(
        markdown.starts_with("Map\\<`String (pattern: /^a$/ (Full))`, `Bool`\\> (entries: 1..=4)"),
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
    let ty = AttributeType::refined_string(
        None,
        Some(carina_core::schema::PatternSpec::new(
            "a|`b\n\t\r",
            carina_core::schema::PatternMatch::Substring {
                reason: carina_core::schema::PatternReason::new(
                    "Preserve the fixture regex search semantics.",
                )
                .unwrap(),
            },
        )),
        None,
        None,
    );
    let markdown = page(&ty, &mut Hooks::default()).unwrap();
    assert!(
        markdown.starts_with(r#"``String (pattern: "a|`b\n\t\r" (Substring: Preserve the fixture regex search semantics.))``"#),
        "{markdown}"
    );
    assert!(
        markdown.contains(r#"| ``String (pattern: "a\|`b\n\t\r" (Substring: Preserve the fixture regex search semantics.))`` |"#),
        "{markdown}"
    );
    let odd_slash = AttributeType::refined_string(
        None,
        Some(carina_core::schema::PatternSpec::new(
            r"a\|b",
            carina_core::schema::PatternMatch::Substring {
                reason: carina_core::schema::PatternReason::new(
                    "Preserve the fixture regex search semantics.",
                )
                .unwrap(),
            },
        )),
        None,
        None,
    );
    let markdown = page(&odd_slash, &mut Hooks::default()).unwrap();
    assert!(
        markdown.contains(r"| `String (pattern: /a\`<!---->`\|b/ (Substring: Preserve the fixture regex search semantics.))` |"),
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
