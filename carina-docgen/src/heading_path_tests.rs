use crate::{Heading, HeadingPath, PageBuilder, TypeEdge, heading_ids, heading_slug};

#[test]
fn heading_path_renders_list_of_struct() {
    let path = HeadingPath::root("domain_validation_options")
        .child(TypeEdge::Item)
        .child(TypeEdge::Field("resource_record"))
        .child(TypeEdge::Field("type"));
    assert_eq!(
        path.to_string(),
        "domain_validation_options[].resource_record.type"
    );
}

#[test]
fn heading_path_renders_map_of_struct() {
    let path = HeadingPath::root("tags")
        .child(TypeEdge::Value)
        .child(TypeEdge::Field("value"));
    assert_eq!(path.to_string(), "tags['<key>'].value");
}

#[test]
fn heading_path_renders_nested_lists() {
    let path = HeadingPath::root("a")
        .child(TypeEdge::Item)
        .child(TypeEdge::Item)
        .child(TypeEdge::Field("b"));
    assert_eq!(path.to_string(), "a[][].b");
}

#[test]
fn heading_path_members_preserve_identity_without_rendering() {
    let root = HeadingPath::root("choice");
    let first = root.child(TypeEdge::Member(0));
    let second = root.child(TypeEdge::Member(1));
    assert_ne!(first, second);
    assert_eq!(first.to_string(), "choice");
    assert_eq!(second.to_string(), "choice");
    assert_eq!(first.visible(), root);
    assert_eq!(second.visible(), root);
}

#[test]
fn heading_path_visible_preserves_every_non_member_edge() {
    let path = HeadingPath::root("policy")
        .child(TypeEdge::Field("statement"))
        .child(TypeEdge::Item)
        .child(TypeEdge::Field("condition"));
    let member_path = HeadingPath::root("policy")
        .child(TypeEdge::Member(0))
        .child(TypeEdge::Field("statement"))
        .child(TypeEdge::Item)
        .child(TypeEdge::Member(1))
        .child(TypeEdge::Field("condition"));
    for edge in [TypeEdge::Key, TypeEdge::Value] {
        assert_eq!(member_path.child(edge).visible(), path.child(edge));
    }
    assert_ne!(
        path.child(TypeEdge::Key).visible(),
        path.child(TypeEdge::Value).visible()
    );
}

#[test]
fn heading_path_renders_map_keys_distinctly_from_values() {
    let path = HeadingPath::root("policy")
        .child(TypeEdge::Field("statement"))
        .child(TypeEdge::Item)
        .child(TypeEdge::Field("condition"));
    let key = path.child(TypeEdge::Key);
    let value = path.child(TypeEdge::Value);
    let mut page = PageBuilder::<()>::new();
    page.heading(Heading::plain(3, format!("{key} (ConditionOperator)")))
        .unwrap();
    page.heading(Heading::plain(3, format!("{value} (ConditionOperator)")))
        .unwrap();
    let markdown = page.finish().unwrap();
    assert!(markdown.contains("### policy.statement[].condition keys (ConditionOperator)\n"));
    assert!(
        markdown.contains("### policy.statement[].condition['\\<key\\>'] (ConditionOperator)\n")
    );
    let ids = heading_ids(&markdown);
    assert_eq!(
        ids,
        [
            "policystatementcondition-keys-conditionoperator",
            "policystatementconditionkey-conditionoperator",
        ]
    );
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn heading_path_list_slug_preserves_the_old_dotted_slug() {
    let path = HeadingPath::root("a")
        .child(TypeEdge::Item)
        .child(TypeEdge::Field("b"));
    assert_eq!(heading_slug(&path.to_string()), "ab");
    assert_eq!(heading_slug(&path.to_string()), heading_slug("a.b"));
}

#[test]
fn heading_path_map_slug_adds_key_and_dedupes() {
    let path = HeadingPath::root("tags")
        .child(TypeEdge::Value)
        .child(TypeEdge::Field("value"));
    assert_eq!(heading_slug(&path.to_string()), "tagskeyvalue");
    let mut page = PageBuilder::<()>::new();
    page.heading(Heading::plain(2, "tagskeyvalue")).unwrap();
    page.heading(Heading::plain(3, path.to_string())).unwrap();
    let markdown = page.finish().unwrap();
    assert_eq!(heading_ids(&markdown), ["tagskeyvalue", "tagskeyvalue-1"]);
}
