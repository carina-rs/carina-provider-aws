use carina_docgen::{
    DocgenError, Heading, PageBuilder, PageViolationKind, TableCell, TypeDisplay, check_page,
    heading_ids,
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

#[test]
fn type_display_link_labels_are_literal() {
    let display = TypeDisplay::sequence([
        TypeDisplay::link(Target::Options, "A`"),
        TypeDisplay::link(Target::Options, "B"),
        TypeDisplay::code("c"),
    ]);
    let mut page = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    page.target_heading(Target::Options, Heading::plain(2, "X"))
        .unwrap();
    let markdown = page.finish().expect("valid linked type display");
    render_like_registry(&markdown, |_, html, visible| {
        assert_eq!(visible, "A`BcX", "{html}");
        assert_eq!(html.matches("<a ").count(), 2, "{html}");
        assert_eq!(html.matches("<code>").count(), 1, "{html}");
        assert!(
            html.contains("<a href=\"#x\">A`</a><a href=\"#x\">B</a><code>c</code>"),
            "{html}"
        );
        assert!(!html.contains("<del>"), "{html}");
    });
}

#[test]
fn type_display_text_preserves_literals_with_registry_extensions() {
    for literal in [
        "~~String~~",
        "~String~",
        "[^footnote]",
        "[x] task",
        "[ ] task",
        "- [x] task",
        "+ [ ] task",
        "1. task",
        "1) task",
        "---",
        "*String* _String_ `String` [String](#missing) <String> &amp;",
    ] {
        assert_literal_display(literal);
    }
}

#[test]
fn type_display_text_disables_registry_autolinks() {
    for literal in [
        "https://example.com",
        "http://example.com",
        "ftp://example.com",
        "www.example.com",
        "person@example.com",
        "mailto:person@example.com",
        "xmpp:person@example.com",
        "<https://example.com>",
        "www.example.com/path?a=b&c=d",
    ] {
        assert_literal_display(literal);
    }
}

#[test]
fn type_display_link_labels_escape_registry_syntax() {
    for label in [
        "A`",
        "[Options]",
        "~~String~~",
        "*String* _String_",
        "https://example.com",
        "www.example.com",
        "person@example.com",
        r"\|",
        "<Options> &amp;",
        "[^footnote]",
        "[x] task",
    ] {
        for in_table in [false, true] {
            let display = TypeDisplay::link(Target::Options, label);
            let mut page = PageBuilder::new();
            if in_table {
                page.table(["Label"], [[display.table_cell()]]).unwrap();
            } else {
                display.write_to(&mut page);
            }
            page.push_str("\n\n");
            page.target_heading(Target::Options, Heading::plain(2, "X"))
                .unwrap();
            let markdown = page.finish().expect("literal link label");
            render_like_registry(&markdown, |_, html, visible| {
                let mut escaped = String::new();
                comrak::html::escape(&mut escaped, label).unwrap();
                assert!(
                    html.contains(&format!("<a href=\"#x\">{escaped}</a>")),
                    "{html}"
                );
                assert_eq!(html.matches("<a ").count(), 1, "{html}");
                assert!(!html.contains("<code>"), "{html}");
                assert!(!html.contains("<del>"), "{html}");
                let expected = if in_table {
                    format!("Label{label}X")
                } else {
                    format!("{label}X")
                };
                assert_eq!(visible, expected, "{html}");
            });
        }
    }
}

#[test]
fn type_display_split_literals_cannot_form_autolinks() {
    for parts in [
        ["https:", "//example.com"],
        ["ww", "w.example.com"],
        ["person", "@example.com"],
    ] {
        let literal = parts.concat();
        let mut page: PageBuilder<Target> = PageBuilder::new();
        for part in parts {
            TypeDisplay::text(part).write_to(&mut page);
        }
        let markdown = page.finish().expect("literal sequence");
        render_like_registry(&markdown, |_, html, visible| {
            assert_eq!(visible, literal);
            assert!(!html.contains("<a "), "{html}");
            assert!(!html.contains("<del>"), "{html}");
            assert!(!html.contains("<code>"), "{html}");
        });
    }
}

fn assert_literal_display(literal: &str) {
    for in_table in [false, true] {
        let mut page: PageBuilder<Target> = PageBuilder::new();
        let display = TypeDisplay::text(literal);
        if in_table {
            page.table(["Literal"], [[display.table_cell()]]).unwrap();
        } else {
            display.write_to(&mut page);
        }
        let markdown = page.finish().expect("literal text introduces no links");
        render_like_registry(&markdown, |_, html, visible| {
            let expected = if in_table {
                format!("Literal{literal}")
            } else {
                literal.to_string()
            };
            assert_eq!(visible, expected, "{html}");
            assert!(
                !html.contains("<a "),
                "literal was autolinked: {literal:?}: {html}"
            );
            assert!(
                !html.contains("<del>"),
                "literal became strikethrough: {literal:?}: {html}"
            );
            assert!(
                !html.contains("<code>"),
                "literal became code: {literal:?}: {html}"
            );
            let mut escaped = String::new();
            comrak::html::escape(&mut escaped, literal).unwrap();
            if in_table {
                assert!(html.contains(&format!("<td>{escaped}</td>")), "{html}");
            } else {
                assert_eq!(html, format!("<p>{escaped}</p>\n"));
            }
        });
    }
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
    let display = TypeDisplay::sequence([
        TypeDisplay::text("String(pattern: "),
        TypeDisplay::code(pattern),
        TypeDisplay::text(")"),
    ]);
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.attribute_reference_entry("name", display.table_cell(), None)
        .expect("valid type display");
    page.table(["Type"], [[display.table_cell()]])
        .expect("valid table");

    let markdown = page
        .finish()
        .expect("regex is literal, not a Markdown link");
    assert!(markdown.contains(&format!("String(pattern: `{pattern}`)")));
    check_page(&markdown).expect("the rendered pattern passes page validation");
}

#[test]
fn type_display_keeps_nested_collection_links_outside_code_spans() {
    let link = || TypeDisplay::link(Target::Options, "Struct(X)");
    let displays = [
        TypeDisplay::list(link()),
        TypeDisplay::map(link()),
        TypeDisplay::list(TypeDisplay::map(TypeDisplay::union([
            TypeDisplay::text("String"),
            link(),
        ]))),
        TypeDisplay::union([TypeDisplay::list(link()), TypeDisplay::text("Bool")]),
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
        assert!(
            root.descendants()
                .all(|node| !matches!(node.data.borrow().value, NodeValue::Code(_)))
        );
    });
}

#[test]
fn type_display_adjacent_code_parts_preserve_links() {
    let parts = || {
        [
            TypeDisplay::code("a"),
            TypeDisplay::code("`"),
            TypeDisplay::link(Target::Options, "X"),
            TypeDisplay::code("`"),
        ]
    };
    let displays = [
        vec![TypeDisplay::sequence(parts())],
        vec![TypeDisplay::sequence([
            TypeDisplay::sequence([TypeDisplay::code("a"), TypeDisplay::code("")]),
            TypeDisplay::text(""),
            TypeDisplay::sequence([
                TypeDisplay::code("`"),
                TypeDisplay::link(Target::Options, "X"),
                TypeDisplay::code("`"),
            ]),
        ])],
        Vec::from(parts()),
    ];
    for parts in displays {
        for in_table in [false, true] {
            let mut page = PageBuilder::new();
            if in_table {
                page.table(
                    ["Type"],
                    [[TypeDisplay::sequence(parts.clone()).table_cell()]],
                )
                .unwrap();
            } else {
                for part in &parts {
                    part.write_to(&mut page);
                }
            }
            page.push_str("\n\n");
            page.target_heading(Target::Options, Heading::plain(2, "X"))
                .unwrap();

            let markdown = page.finish().expect("the page checker accepts the display");
            check_page(&markdown).expect("the finished page remains valid");
            render_like_registry(&markdown, |root, _, _| {
                let links: Vec<String> = root
                    .descendants()
                    .filter_map(|node| match &node.data.borrow().value {
                        NodeValue::Link(link) => Some(link.url.clone()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(links, ["#x"], "the link must remain clickable: {markdown}");
                let code: Vec<String> = root
                    .descendants()
                    .filter_map(|node| match &node.data.borrow().value {
                        NodeValue::Code(code) => Some(code.literal.clone()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    code,
                    ["a`", "`"],
                    "adjacent code literals coalesce: {markdown}"
                );
                assert!(markdown.contains("`` a` ``[X](#x)`` ` ``"), "{markdown}");
            });
        }
    }
}

#[test]
fn type_display_table_code_preserves_backslashes_before_pipes() {
    let literals = [r"\|", "|", r"\\|", r"\\\|", r"\|\|", r"`\|`"];
    let split_literal = TypeDisplay::sequence([TypeDisplay::code("\\"), TypeDisplay::code("|")]);
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.table(
        ["Pattern"],
        literals
            .map(|literal| [TypeDisplay::code(literal).table_cell()])
            .into_iter()
            .chain([[split_literal.table_cell()]]),
    )
    .unwrap();
    let markdown = page.finish().expect("escaped pipes stay in their cells");
    render_like_registry(&markdown, |root, html, _| {
        let code: Vec<String> = root
            .descendants()
            .filter(|node| matches!(node.data.borrow().value, NodeValue::TableRow(false)))
            .flat_map(|row| row.children())
            .map(|cell| {
                cell.descendants()
                    .filter_map(|node| match &node.data.borrow().value {
                        NodeValue::Code(code) => Some(code.literal.clone()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .collect();
        assert_eq!(
            code,
            literals.into_iter().chain([r"\|"]).collect::<Vec<_>>(),
            "GFM must preserve literal code text: {markdown}"
        );
        assert!(
            html.contains("<td><code>\\</code><!-- raw HTML omitted --><code>|</code></td>"),
            "{html}"
        );
        assert!(html.contains("<code>|</code>"), "{html}");
    });
}

#[test]
fn type_display_code_preserves_backticks_spaces_and_pipes() {
    let literals = ["`[x](#literal)``", "a|b", " value ", "  "];
    let mut page: PageBuilder<Target> = PageBuilder::new();
    for literal in literals {
        TypeDisplay::code(literal).write_to(&mut page);
        page.push_str("\n\n");
    }
    page.table(
        ["Type"],
        literals.map(|literal| [TypeDisplay::code(literal).table_cell()]),
    )
    .unwrap();

    let markdown = page.finish().expect("literal code passes validation");
    render_like_registry(&markdown, |root, _, _| {
        let actual: Vec<String> = root
            .descendants()
            .filter_map(|node| match &node.data.borrow().value {
                NodeValue::Code(code) => Some(code.literal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(actual, literals.repeat(2), "{markdown}");
    });
}

#[test]
fn type_display_text_cannot_introduce_links_or_code() {
    let display = TypeDisplay::text("[Struct(X)](#absent) and `literal`");
    let mut page: PageBuilder<Target> = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    TypeDisplay::list(display).write_to(&mut page);

    let markdown = page.finish().expect("text never introduces a link");
    render_like_registry(&markdown, |root, _, _| {
        assert!(
            root.descendants()
                .all(|node| !matches!(node.data.borrow().value, NodeValue::Link(_)))
        );
        let code: Vec<String> = root
            .descendants()
            .filter_map(|node| match &node.data.borrow().value {
                NodeValue::Code(code) => Some(code.literal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(code, ["List<[Struct(X)](#absent) and `literal`>"]);
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
