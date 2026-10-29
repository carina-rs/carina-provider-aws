use carina_docgen::heading_ids;
use serde::Deserialize;

#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    headings: Vec<String>,
    ids: Vec<String>,
}

#[test]
fn heading_slug_fixture_matches_registry_renderer() {
    // Source of truth: carina-rs/registry origin/main src/docs_bundle.rs.
    let cases: Vec<FixtureCase> = serde_json::from_str(include_str!("fixtures/heading-slugs.json"))
        .expect("valid heading-slug fixture");

    for case in cases {
        let markdown = case.headings.join("\n\n");
        let actual = heading_ids(&markdown);
        assert_eq!(actual, case.ids, "{}", case.name);
    }
}
