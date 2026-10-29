use carina_docgen::{Heading, PageBuilder, PageViolationKind, check_page, heading_ids};

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
fn check_page_rejects_markdown_links_inside_code_spans() {
    assert_eq!(
        violation_kinds("# Page\n\n`List<[Struct(X)](#x)>`\n\n## X\n"),
        [PageViolationKind::LinkInsideCodeSpan]
    );
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
