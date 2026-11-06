use std::cell::Cell;
use std::sync::Arc;

use carina_core::schema::{AttributeType, Draft, ListElements, RawShape, TypeIdentity};
use carina_docgen::{
    Declaration, DeclarationKind, DocLink, DocgenError, Heading, ListConstraints, MapConstraints,
    PageBuilder, PageViolationKind, Resolved, ScalarDoc, TableCell, TypeDisplay, TypeDocHooks,
    TypeEdge, TypeRegistry, ValidationNote, check_page, heading_ids, list_display, map_display,
    render_attribute_type, scalar_display,
};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

const IPV6_DESCRIPTION: &str =
    "Indicates whether to enable Ipv6 Support for Vpc Attachment. Valid Values: enable | disable";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Target {
    Options,
    Missing,
}

fn violation_kinds(markdown: &str) -> Vec<PageViolationKind> {
    check_page(markdown)
        .expect_err("page must fail validation")
        .violations()
        .iter()
        .map(|violation| violation.kind)
        .collect()
}

fn assert_std_error<T, E: std::error::Error>(_: &Result<T, E>) {}

fn render_like_registry(markdown: &str, check: impl for<'a> FnOnce(&'a AstNode<'a>, &str, &str)) {
    // carina-docs-bundle c6bfdcec781abf0ddb61268e5e7d114717353e4f::render_markdown.
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.render.r#unsafe = false;
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options);
    let mut html = String::new();
    comrak::format_html(root, &options, &mut html).unwrap();
    // Inline characters printed by the HTML renderer, without block separators.
    let visible: String = root
        .descendants()
        .filter_map(|node| match &node.data.borrow().value {
            NodeValue::Text(text) => Some(text.to_string()),
            NodeValue::Code(code) => Some(code.literal.clone()),
            _ => None,
        })
        .collect();
    check(root, &html, &visible);
}

fn named_type(ty: AttributeType) -> TypeDisplay<Target> {
    schema_type(&ty, |identity| identity.to_string(), None)
}

fn schema_type(
    ty: &AttributeType,
    scalar_name: impl Fn(&TypeIdentity) -> String,
    note: Option<ValidationNote<'_>>,
) -> TypeDisplay<Target> {
    struct Hooks<F>(F);
    impl<'a, F: Fn(&TypeIdentity) -> String> TypeDocHooks<'a, Draft> for Hooks<F> {
        type NodeId = usize;
        type Target = Target;
        type Scope = ();
        fn resolve(&self, ty: &'a AttributeType) -> anyhow::Result<Resolved<'a, Draft, usize>> {
            Ok(Resolved {
                id: ty as *const _ as usize,
                ty,
            })
        }
        fn scalar_name(&self, identity: &TypeIdentity) -> String {
            self.0(identity)
        }
        fn child_scope(&self, _parent: &(), _edge: TypeEdge<'_>) {}
        fn register(
            &mut self,
            _scope: &(),
            declaration: Declaration<'a, Draft, usize>,
        ) -> anyhow::Result<DocLink<Target>> {
            let label = match declaration.kind {
                DeclarationKind::Struct { name, .. } => name.to_string(),
                DeclarationKind::Union { .. } => "Choice".into(),
                _ => panic!("named fixture must be a struct or union"),
            };
            Ok(DocLink {
                target: Target::Options,
                label,
            })
        }
    }
    render_attribute_type(
        ty,
        &(),
        &mut Hooks(scalar_name),
        &mut TypeRegistry::default(),
        note,
    )
    .unwrap()
}

#[test]
fn page_builder_inline_preserves_code_literals_in_prose() {
    for (literal, expected_code, visible_code) in [
        ("key`part|value", "``key`part|value``", "key`part|value"),
        ("key\u{7}part", r"`key\u{7}part`", r"key\u{7}part"),
    ] {
        let mut page: PageBuilder<Target> = PageBuilder::new();
        page.push_str("Uses ");
        page.inline(TableCell::code(literal));
        page.push_str(" in prose.\n\n");

        let markdown = page.finish().expect("valid inline code in prose");
        assert_eq!(markdown, format!("Uses {expected_code} in prose.\n\n"));
        render_like_registry(&markdown, |_, html, visible| {
            assert_eq!(
                html,
                format!("<p>Uses <code>{visible_code}</code> in prose.</p>\n")
            );
            assert_eq!(visible, format!("Uses {visible_code} in prose."));
        });
    }
}

#[test]
fn page_builder_inline_keeps_text_and_typed_links_in_prose() {
    let mut parts = TableCell::text("Use | ");
    parts.push_code("name");
    parts.push_text(" with ");
    parts.push_link(Target::Options, "Struct(Options)");

    let mut page = PageBuilder::new();
    page.inline(parts);
    page.push_str(".\n\n");
    page.heading(Heading::code(3, "options")).unwrap();
    page.target_heading(Target::Options, Heading::plain(3, "Options"))
        .unwrap();

    let markdown = page.finish().expect("valid typed inline parts in prose");
    assert_eq!(
        markdown,
        concat!(
            "Use | `name` with [Struct(Options)](#options-1).\n\n",
            "### `options`\n\n### Options\n\n",
        )
    );
    render_like_registry(&markdown, |_, html, visible| {
        assert!(html.contains("<a href=\"#options-1\">Struct(Options)</a>"));
        assert_eq!(visible, "Use | name with Struct(Options).optionsOptions");
    });
}

#[test]
fn attribute_reference_entry_writes_description_and_resolves_type_links() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.heading(Heading::plain(1, "Page")).unwrap();
    page.attribute_reference_entry(
        "options",
        TableCell::link(Target::Options, "Struct(Options)"),
        Some("Uses `<bucket>.s3.amazonaws.com`."),
    )
    .unwrap();
    page.attribute_reference_entry("id", TableCell::text("String"), None)
        .unwrap();
    page.attribute_reference_entry("empty", TableCell::text("String"), Some(""))
        .unwrap();
    page.target_heading(Target::Options, Heading::plain(3, "Options"))
        .unwrap();

    let markdown = page.finish().expect("valid attribute entries");
    assert_eq!(
        markdown,
        concat!(
            "# Page\n\n",
            "### `options`\n\n",
            "- **Type:** [Struct(Options)](#options-1)\n",
            "- **Read-only**\n\n",
            "Uses `<bucket>.s3.amazonaws.com`.\n\n",
            "### `id`\n\n- **Type:** String\n- **Read-only**\n\n",
            "### `empty`\n\n- **Type:** String\n- **Read-only**\n\n",
            "### Options\n\n",
        )
    );
}

#[test]
fn attribute_reference_entry_allocates_duplicate_heading_slugs() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.heading(Heading::code(3, "region")).unwrap();
    page.attribute_reference_entry(
        "region",
        TableCell::code("Region"),
        Some("The region to use."),
    )
    .unwrap();
    page.heading(Heading::plain(3, "Another heading")).unwrap();
    let markdown = page.finish().unwrap();
    assert!(markdown.contains("- **Type:** `Region`\n"), "{markdown}");
    assert_eq!(
        heading_ids(&markdown),
        ["region", "region-1", "another-heading"]
    );
}

#[test]
fn scalar_compositors_name_declared_identities_and_keep_builtin_names() {
    let identity = TypeIdentity::bare("Declared");
    for ty in [
        AttributeType::refined_string(Some(identity.clone()), None, None, None),
        AttributeType::refined_int(Some(identity.clone()), None),
        AttributeType::refined_float(Some(identity), None),
    ] {
        let identity = match ty.raw_shape() {
            RawShape::String { identity, .. }
            | RawShape::Int { identity, .. }
            | RawShape::Float { identity, .. } => identity.unwrap(),
            _ => unreachable!("declared scalar fixture"),
        };
        let calls = Cell::new(0);
        let display = schema_type(
            &ty,
            |actual| {
                assert!(std::ptr::eq(actual, identity));
                calls.set(calls.get() + 1);
                format!("Provider{}", actual.kind)
            },
            None,
        );
        assert_eq!(calls.get(), 1);
        let mut page: PageBuilder<Target> = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        page.table(["Type"], [[display.table_cell()]]).unwrap();
        render_like_registry(&page.finish().unwrap(), |_, _, visible| {
            assert_eq!(visible, "ProviderDeclaredTypeProviderDeclared");
        });
    }
    for (scalar, ty, name) in [
        (
            ScalarDoc::String {
                pattern: None,
                length: None,
                validate: false,
                to_dsl: None,
            },
            AttributeType::string(),
            "String",
        ),
        (
            ScalarDoc::Int {
                range: None,
                validate: false,
            },
            AttributeType::int(),
            "Int",
        ),
        (
            ScalarDoc::Float {
                range: None,
                validate: false,
            },
            AttributeType::float(),
            "Float",
        ),
        (ScalarDoc::Bool, AttributeType::bool(), "Bool"),
        (ScalarDoc::Duration, AttributeType::duration(), "Duration"),
    ] {
        for display in [
            scalar_display(scalar, None),
            schema_type(
                &ty,
                |_| panic!("builtin names must not call the provider's naming hook"),
                None,
            ),
        ] {
            let mut page: PageBuilder<Target> = PageBuilder::new();
            display.write_to(&mut page);
            assert_eq!(page.finish().unwrap(), format!("`{name}`"));
        }
    }
}

#[test]
fn type_validation_notes_use_one_literal_suffix_inline_and_in_tables() {
    let ty = AttributeType::refined_int_with_validator(
        TypeIdentity::bare("Asn"),
        None,
        Arc::new(|_| Ok(())),
    );
    let note = "64512..=65534 or 4200000000..=4294967294; [text](#literal) | `literal`";
    let display = schema_type(
        &ty,
        |identity| identity.kind.clone(),
        Some(ValidationNote::new(note)),
    );
    let mut page: PageBuilder<Target> = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    page.table(["Type"], [[display.table_cell()]]).unwrap();
    let markdown = page.finish().unwrap();
    assert_eq!(
        markdown.matches("`Asn (custom validator)`; ").count(),
        2,
        "{markdown}"
    );
    render_like_registry(&markdown, |root, _, visible| {
        assert!(visible.contains(note), "{visible}");
        assert!(
            root.descendants()
                .all(|node| !matches!(node.data.borrow().value, NodeValue::Link(_)))
        );
        let code: Vec<_> = root
            .descendants()
            .filter_map(|node| match &node.data.borrow().value {
                NodeValue::Code(code) => Some(code.literal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(code, ["Asn (custom validator)"; 2]);
    });
}

#[test]
fn page_builder_methods_return_std_errors() {
    let mut page: PageBuilder<Target> = PageBuilder::new();

    let heading = page.heading(Heading::plain(1, "Page"));
    assert_std_error(&heading);
    heading.expect("valid heading");

    let target_heading = page.target_heading(Target::Options, Heading::plain(2, "Options"));
    assert_std_error(&target_heading);
    target_heading.expect("valid target heading");

    let finished = page.finish();
    assert_std_error(&finished);
    finished.expect("valid finished page");
}

#[test]
fn check_page_rejects_unresolved_fragment_only_links() {
    assert_eq!(
        violation_kinds("# Page\n\n[missing](#not-here)\n"),
        [PageViolationKind::UnresolvedFragment]
    );
}

#[test]
fn check_page_accepts_literal_link_syntax_inside_code_spans() {
    check_page("# Page\n\n`List<[Struct(X)](#not-a-heading)>`\n")
        .expect("code spans contain literal text, including link syntax");
}

#[test]
fn type_display_pattern_is_literal_code() {
    let pattern = r"^[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?\z";
    let display = scalar_display(
        ScalarDoc::String {
            pattern: Some(pattern),
            length: None,
            validate: false,
            to_dsl: None,
        },
        None,
    );
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.attribute_reference_entry("name", display.table_cell(), None)
        .expect("valid type display");
    page.table(["Type"], [[display.table_cell()]])
        .expect("valid table");

    let markdown = page
        .finish()
        .expect("regex is literal, not a Markdown link");
    assert!(markdown.contains(&format!("`String (pattern: {pattern})`")));
    check_page(&markdown).expect("the rendered pattern passes page validation");
}

#[test]
fn map_length_type_display_keeps_key_constraints_and_value_links() {
    let key = scalar_display(
        ScalarDoc::String {
            pattern: Some("^[a-z]+$"),
            length: Some((Some(1), Some(8))),
            validate: false,
            to_dsl: None,
        },
        None,
    );
    let display = map_display(
        key,
        named_type(AttributeType::struct_("X", Vec::new())),
        MapConstraints {
            length: Some((Some(2), Some(5))),
        },
        None,
    );
    let mut page = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    page.table(["Type"], [[display.table_cell()]]).unwrap();
    page.push_str("\n");
    page.target_heading(Target::Options, Heading::plain(2, "X"))
        .unwrap();
    let markdown = page.finish().unwrap();
    assert!(
        markdown.contains("`String (pattern: ^[a-z]+$, len: 1..=8)`"),
        "{markdown}"
    );
    assert!(markdown.contains("[Struct(X)](#x)"), "{markdown}");
    assert!(markdown.contains("(entries: 2..=5)"), "{markdown}");
    check_page(&markdown).unwrap();
}

#[test]
fn type_display_keeps_nested_collection_links_outside_code_spans() {
    let link = || named_type(AttributeType::struct_("X", Vec::new()));
    let union = || {
        named_type(AttributeType::union(vec![
            AttributeType::string(),
            AttributeType::bool(),
        ]))
    };
    let string = || {
        scalar_display(
            ScalarDoc::String {
                pattern: None,
                length: None,
                validate: false,
                to_dsl: None,
            },
            None,
        )
    };
    let list = |item| {
        list_display(
            item,
            true,
            ListConstraints {
                length: None,
                elements: ListElements::AllowDuplicates,
            },
            None,
        )
    };
    let displays = [
        list(link()),
        map_display(string(), link(), MapConstraints::default(), None),
        list(map_display(
            string(),
            union(),
            MapConstraints::default(),
            None,
        )),
        union(),
    ];
    let mut page = PageBuilder::new();
    for display in &displays {
        display.write_to(&mut page);
        page.push_str("\n\n");
    }
    page.table(
        ["Type"],
        displays.iter().map(|display| [display.table_cell()]),
    )
    .unwrap();
    page.push_str("\n");
    page.target_heading(Target::Options, Heading::plain(2, "X"))
        .unwrap();

    let markdown = page.finish().expect("all collection links resolve");
    assert!(markdown.contains(r"List\<[Struct(X)](#x)\>"));
    render_like_registry(&markdown, |root, _, _| {
        assert_eq!(
            root.descendants()
                .filter(|node| matches!(node.data.borrow().value, NodeValue::Link(_)))
                .count(),
            displays.len() * 2,
            "each type link must render as a clickable link: {markdown}"
        );
        let code: Vec<_> = root
            .descendants()
            .filter_map(|node| match &node.data.borrow().value {
                NodeValue::Code(code) => Some(code.literal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            code, ["String"; 4],
            "only map keys are code literals: {markdown}"
        );
    });
}

#[test]
fn check_page_rejects_duplicated_enum_and_struct_sections() {
    let markdown = concat!(
        "# Page\n\n",
        "## Enum Values\n\n",
        "### protocol (Protocol)\n\n",
        "### protocol (Protocol)\n\n",
        "## Struct Definitions\n\n",
        "### Options\n\n",
        "### Options\n",
    );
    assert_eq!(
        violation_kinds(markdown),
        [
            PageViolationKind::DuplicateSectionHeading,
            PageViolationKind::DuplicateSectionHeading,
        ]
    );
}

#[test]
fn check_page_rejects_repeated_enum_or_struct_section_blocks() {
    assert_eq!(
        violation_kinds(
            "# Page\n\n## Enum Values\n\n### one (One)\n\n## Enum Values\n\n### two (Two)\n"
        ),
        [PageViolationKind::DuplicateSectionHeading]
    );
}

#[test]
fn check_page_accepts_fragment_from_registered_heading() {
    check_page("# Page\n\n[options](#options-1)\n\n### `options`\n\n### Options\n")
        .expect("fragment resolves to the second options heading");
}

#[test]
fn page_builder_resolves_links_from_headings_written_after_the_link() {
    let mut page = PageBuilder::new();
    page.heading(Heading::plain(1, "Page")).unwrap();
    page.push_str("- **Type:** ");
    page.link(Target::Options, "Struct(Options)");
    page.push_str("\n\n");
    page.heading(Heading::plain(2, "Example")).unwrap();
    page.heading(Heading::code(3, "options")).unwrap();
    page.heading(Heading::plain(2, "Struct Definitions"))
        .unwrap();
    page.target_heading(Target::Options, Heading::plain(3, "Options"))
        .unwrap();

    let markdown = page.finish().expect("valid finished page");

    assert!(
        markdown.contains("[Struct(Options)](#options-1)"),
        "{markdown}"
    );
    assert_eq!(
        heading_ids(&markdown),
        [
            "page",
            "example",
            "options",
            "struct-definitions",
            "options-1"
        ]
    );
}

#[test]
fn page_builder_allocates_example_collisions_in_actual_write_order() {
    let mut page = PageBuilder::new();
    page.heading(Heading::plain(1, "Page")).unwrap();
    page.link(Target::Options, "Struct(Example)");
    page.push_str("\n\n");
    page.heading(Heading::plain(2, "Example")).unwrap();
    page.heading(Heading::code(3, "example")).unwrap();
    page.target_heading(Target::Options, Heading::plain(3, "Example"))
        .unwrap();

    let markdown = page.finish().expect("valid finished page");
    assert!(
        markdown.contains("[Struct(Example)](#example-2)"),
        "{markdown}"
    );
}

#[test]
fn page_builder_rejects_a_link_to_a_target_that_was_never_written() {
    let mut page = PageBuilder::new();
    page.heading(Heading::plain(1, "Page")).unwrap();
    page.link(Target::Missing, "missing");
    page.heading(Heading::plain(2, "Missing")).unwrap();

    let error = page.finish().expect_err("missing target must fail");
    assert!(error.to_string().contains("never written"), "{error}");
}

#[test]
fn page_builder_rejects_writing_the_same_target_twice() {
    let mut page = PageBuilder::new();
    page.target_heading(Target::Options, Heading::plain(2, "Options"))
        .unwrap();

    let error = page
        .target_heading(Target::Options, Heading::plain(2, "Other Options"))
        .expect_err("duplicate target key must fail");
    assert!(error.to_string().contains("more than once"), "{error}");
}

#[test]
fn page_builder_rejects_a_heading_that_bypasses_the_builder() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.heading(Heading::plain(1, "Page")).unwrap();
    page.push_str("## Bypassed\n\n");

    let error = page.finish().expect_err("unregistered heading must fail");
    assert!(
        error.to_string().contains("heading order differed"),
        "{error}"
    );
}

#[test]
fn table_escapes_unescaped_pipes_in_text_cells() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.table(["Description"], [[TableCell::text(IPV6_DESCRIPTION)]])
        .expect("valid table");

    let markdown = page.finish().expect("valid finished page");

    assert_eq!(
        markdown,
        concat!(
            "| Description |\n",
            "|-------------|\n",
            r"| Indicates whether to enable Ipv6 Support for Vpc Attachment. Valid Values: enable \| disable |",
            "\n",
        )
    );
}

#[test]
fn table_preserves_already_escaped_pipes() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.table(
        ["Description"],
        [[TableCell::text(r"Valid Values: enable \| disable")]],
    )
    .expect("valid table");

    let markdown = page.finish().expect("valid finished page");

    assert!(
        markdown.contains(r"Valid Values: enable \| disable"),
        "{markdown}"
    );
    assert!(!markdown.contains(r"Valid Values: enable \\| disable"));
}

#[test]
fn table_collapses_newlines_in_cells() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.table(
        ["Description"],
        [[TableCell::text("first line\nsecond line\r\nthird line")]],
    )
    .expect("valid table");

    let markdown = page.finish().expect("valid finished page");

    assert!(
        markdown.contains("| first line second line third line |"),
        "{markdown}"
    );
}

#[test]
fn table_resolves_keyed_links_in_cells() {
    let mut page = PageBuilder::new();
    page.table(
        ["Type", "Example"],
        [[
            TableCell::link(Target::Options, "Struct(Options | Settings)"),
            TableCell::code("enable | disable"),
        ]],
    )
    .expect("valid table");
    page.push_str("\n");
    page.target_heading(Target::Options, Heading::plain(2, "Options"))
        .expect("valid target heading");

    let markdown = page.finish().expect("valid finished page");

    assert!(
        markdown.contains(r"| [Struct(Options \| Settings)](#options) | `enable \| disable` |"),
        "{markdown}"
    );
}

#[test]
fn table_renders_empty_code_as_an_empty_cell() {
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.table(["Value"], [[TableCell::code("")]])
        .expect("valid table");

    let markdown = page.finish().expect("valid finished page");

    assert_eq!(markdown, "| Value |\n|-------|\n|  |\n");
}

#[test]
fn table_escapes_brackets_in_link_labels() {
    let mut page = PageBuilder::new();
    page.table(
        ["Type"],
        [[TableCell::link(
            Target::Options,
            "Struct[Options] | Settings",
        )]],
    )
    .expect("valid table");
    page.push_str("\n");
    page.target_heading(Target::Options, Heading::plain(2, "Options"))
        .expect("valid target heading");

    let markdown = page.finish().expect("valid finished page");

    assert!(
        markdown.contains(r"| [Struct\[Options\] \| Settings](#options) |"),
        "{markdown}"
    );
}

#[test]
fn table_rejects_rows_with_wrong_cell_count() {
    let mut page: PageBuilder<Target> = PageBuilder::new();

    let result = page.table(["Field", "Description"], [[TableCell::text("field")]]);
    assert_std_error(&result);
    let error = result.expect_err("short row must fail");

    assert!(matches!(error, DocgenError::Build(_)), "{error}");
    assert!(
        error.to_string().contains("expected 2 cells, got 1"),
        "{error}"
    );
}

#[test]
fn check_page_rejects_table_rows_with_extra_unescaped_pipes() {
    let markdown = format!("| Description |\n|-------------|\n| {IPV6_DESCRIPTION} |\n");

    assert_eq!(
        violation_kinds(&markdown),
        [PageViolationKind::TableColumnMismatch]
    );
}

#[test]
fn check_page_accepts_table_rows_with_escaped_pipes() {
    let markdown = format!(
        "| Description |\n|-------------|\n| {} |\n",
        IPV6_DESCRIPTION.replace('|', r"\|")
    );

    check_page(&markdown).expect("escaped pipe stays within one table cell");
}

#[test]
fn check_page_accepts_an_escaped_final_pipe_and_keeps_backslash_parity() {
    check_page("| Value |\n|-------|\n| a \\|\n")
        .expect("escaped final pipe stays within one table cell");

    assert_eq!(
        violation_kinds("| Value |\n|-------|\n| a \\\\| b |\n"),
        [PageViolationKind::TableColumnMismatch]
    );
}

#[test]
fn check_page_handles_optional_outer_pipes_and_rejects_short_rows() {
    check_page("First | Second\n------|-------\none | two\n").expect("outer pipes are optional");

    assert_eq!(
        violation_kinds("First | Second\n------|-------\nonly one\n"),
        [PageViolationKind::TableColumnMismatch]
    );
}

#[test]
fn check_page_skips_table_like_rows_inside_fenced_code_blocks() {
    let markdown = concat!(
        "```markdown\n",
        "| Description |\n",
        "|-------------|\n",
        "| Indicates whether to enable Ipv6 Support for Vpc Attachment. Valid Values: enable | disable |\n",
        "```\n",
    );

    check_page(markdown).expect("fenced code does not contain a GFM table");
}
