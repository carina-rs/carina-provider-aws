// Tests for the crate-private display primitives. Assertions are preserved
// from the public integration suite when the constructors were sealed.
use crate::{
    DocgenError, Heading, ListConstraints, PageBuilder, TableCell, TypeDisplay, check_page,
    heading_ids, list_display,
};
use carina_core::schema::ListElements;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Target {
    Options,
}

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
fn code_entry_points_render_controls_visibly() {
    let literal = "a\n\t\r\u{7}b";
    let expected = r"a\n\t\u{d}\u{7}b";
    let mut page: PageBuilder<Target> = PageBuilder::new();
    page.heading(Heading::code(2, literal)).unwrap();
    TypeDisplay::code(literal).write_to(&mut page);
    page.push_str("\n\n");
    page.table(["Literal"], [[TableCell::code(literal)]])
        .unwrap();
    let markdown = page.finish().unwrap();
    render_like_registry(&markdown, |root, _, _| {
        let code: Vec<_> = root
            .descendants()
            .filter_map(|node| match &node.data.borrow().value {
                NodeValue::Code(code) => Some(code.literal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(code, [expected; 3]);
    });
    assert_eq!(heading_ids(&markdown), [crate::heading_slug(expected)]);
}

#[test]
fn inline_code_entry_points_reject_empty_literals_with_a_source() {
    use std::error::Error;

    let mut heading: PageBuilder<Target> = PageBuilder::new();
    let heading_error = heading.heading(Heading::code(2, "")).unwrap_err();
    let mut inline: PageBuilder<Target> = PageBuilder::new();
    TypeDisplay::code("").write_to(&mut inline);
    for error in [heading_error, inline.finish().unwrap_err()] {
        assert!(matches!(error, DocgenError::Literal { .. }), "{error}");
        assert!(error.to_string().contains("literal \"\""), "{error}");
        let cause = error
            .source()
            .expect("literal errors preserve their source")
            .to_string();
        assert!(cause.contains("empty literal"), "{cause}");
        let chain = format!("{:#}", anyhow::Error::new(error));
        assert_eq!(chain.matches(&cause).count(), 1, "{chain}");
    }
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
    let literal = "[Struct(X)](#absent) and `literal`";
    let display = TypeDisplay::text(literal);
    let mut page: PageBuilder<Target> = PageBuilder::new();
    display.write_to(&mut page);
    page.push_str("\n\n");
    list_display(
        display,
        true,
        ListConstraints {
            length: None,
            elements: ListElements::AllowDuplicates,
        },
        None,
    )
    .write_to(&mut page);

    let markdown = page.finish().expect("text never introduces a link");
    render_like_registry(&markdown, |root, _, visible| {
        assert_eq!(visible, format!("{literal}List<{literal}> (ordered)"));
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
        assert!(
            code.is_empty(),
            "text stays literal inside collections: {markdown}"
        );
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
