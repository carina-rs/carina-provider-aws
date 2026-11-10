use super::*;
use carina_core::schema::{CompiledPattern, PatternMatch};

#[test]
fn identity_662_round6_02_translated_patterns_match_the_ecma_oracle() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../tests/fixtures/identity-pattern-semantics.json"
    ))
    .unwrap();
    let fits = |text: &str, bounds: &serde_json::Value| {
        let count = text.chars().count() as u64;
        bounds[0].as_u64().is_none_or(|min| count >= min)
            && bounds[1].as_u64().is_none_or(|max| count <= max)
    };
    for row in rows {
        let pattern = string_members::smithy_pattern(
            row["resource"].as_str().unwrap(),
            row["path"].as_str().unwrap(),
            row["smithy_pattern"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            matches!(pattern.mode(), PatternMatch::Full),
            row["translated_mode"] == "Full"
        );
        let source = CompiledPattern::try_new(&pattern).unwrap();
        let old = CompiledPattern::try_new(&PatternSpec::new(
            row["old_pattern"].as_str().unwrap(),
            PatternMatch::Full,
        ))
        .unwrap();
        for case in row["cases"].as_array().unwrap() {
            let value = case["value"].as_str().unwrap();
            assert_eq!(
                source.is_match(value) && fits(value, &row["smithy_length"]),
                case["smithy"].as_bool().unwrap(),
                "Smithy {}.{} {value:?}",
                row["resource"],
                row["path"]
            );
            assert_eq!(
                old.is_match(value) && fits(value, &row["old_length"]),
                case["old"].as_bool().unwrap(),
                "old {}.{} {value:?}",
                row["resource"],
                row["path"]
            );
        }
    }
}

#[test]
fn identity_662_round6_02_role_path_keeps_smithy_search_and_adds_a_full_rule() {
    let model=carina_smithy::parse(&serde_json::json!({"smithy":"2.0","shapes":{
        "test#Path":{"type":"string","traits":{"smithy.api#pattern":r"^(\u002F)|(\u002F[\u0021-\u007E]+\u002F)$","smithy.api#length":{"min":1,"max":512}}}
    }}).to_string()).unwrap();
    let declarations = TypeDeclarations::default();
    let member = carina_smithy::ShapeRef {
        target: "test#Path".into(),
        traits: Default::default(),
    };
    let value = member_string_value(
        &model,
        "iam.Role",
        &member,
        "Path",
        "path",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap();
    let RawShape::String {
        member_layer:
            carina_core::schema::StringMemberParts {
                pattern: Some(source),
                ..
            },
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("Smithy pattern")
    };
    assert!(matches!(source.mode(), PatternMatch::Substring { .. }));
    assert!(
        CompiledPattern::try_new(source)
            .unwrap()
            .is_match("/no-trailing")
    );
    let rules = declarations.input_patterns.borrow();
    let rule = rules
        .get("path")
        .and_then(|rules| rules.first())
        .expect("whole-value CreateRole narrowing");
    let matcher = CompiledPattern::try_new(rule).unwrap();
    for value in ["/", "/a/", "/application/component/"] {
        assert!(matcher.is_match(value));
    }
    for value in ["/no-trailing", "x/a/", "/a/\n", "/\u{7f}/"] {
        assert!(!matcher.is_match(value), "{value:?}");
    }
}

#[test]
fn identity_662_round6_06_nullable_following_element_is_not_silently_rewritten() {
    for source in [
        "^(?!-)[a-z]*[0-9]?-x$",
        "^(?!-)[a-z]*[0-9]*-x$",
        "^(?!-)[a-z]*[0-9]{0,2}-x$",
        "^(?!-)[a-z]*[0-9]{0}-x$",
    ] {
        let error = string_members::smithy_pattern("test.Guard", "text", source)
            .expect_err("unproven nullable guard must be a codegen error");
        let message = format!("{error:#}");
        assert!(
            message.contains("test.Guard.text") && message.contains(source),
            "{message}"
        );
    }
}

#[test]
fn identity_662_round6_06_mandatory_following_class_still_translates() {
    for source in [
        "^(?!-)[a-z]*[0-9]-x$",
        "^(?!-)[a-z]*[0-9]+-x$",
        "^(?!-)[a-z]*[0-9]{1,2}-x$",
    ] {
        let pattern = string_members::smithy_pattern("test.Guard", "text", source).unwrap();
        let matcher = CompiledPattern::try_new(&pattern).unwrap();
        assert!(matcher.is_match("a1-x"));
        assert!(!matcher.is_match("-x"));
    }
}
