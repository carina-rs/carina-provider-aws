use super::*;
use carina_core::schema::CompiledPattern;

#[test]
fn ecma_whitespace_is_exact() {
    let whitespace = [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}',
        '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
    ];
    for (source, inverted) in [
        (r"^\s$", false),
        (r"^\S$", true),
        (r"^[^\s]$", true),
        (r"^[\S]$", true),
    ] {
        let pattern = string_members::smithy_pattern("test.Space", "label", source).unwrap();
        let pattern = CompiledPattern::try_new(&pattern).unwrap();
        for c in whitespace {
            assert_eq!(
                pattern.is_match(&c.to_string()),
                !inverted,
                "{source}, U+{:04X}",
                c as u32
            );
        }
        for c in ['a', '\u{85}', '\u{200b}'] {
            assert_eq!(
                pattern.is_match(&c.to_string()),
                inverted,
                "{source}, U+{:04X}",
                c as u32
            );
        }
    }
}

#[test]
fn negative_prefix_guard_is_not_acm_specific() {
    for (source, allowed, forbidden) in [
        (
            r"^(?!_)[A-Z_]{0,3}[A-Z]$",
            vec!["A", "AB", "A_A"],
            vec!["_A", "_", "AAAAA"],
        ),
        (
            r"^(?!x)[a-z]{1,4}$",
            vec!["a", "ax", "yyyy"],
            vec!["x", "xx", "abcde"],
        ),
        (
            r"^(?!x)[\w-]{1,4}$",
            vec!["a", "a-", "_a"],
            vec!["x", "é", "abcde"],
        ),
    ] {
        let pattern = string_members::smithy_pattern("test.Lookahead", "text", source)
            .expect("general negative-prefix translation");
        let matcher = CompiledPattern::try_new(&pattern).unwrap();
        for value in allowed {
            assert!(matcher.is_match(value), "{source}: {value}");
        }
        for value in forbidden {
            assert!(!matcher.is_match(value), "{source}: {value}");
        }
    }
}

#[test]
fn translated_patterns_match_the_ecma_oracle() {
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
fn nullable_following_element_is_not_silently_rewritten() {
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
fn mandatory_following_class_still_translates() {
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

#[test]
fn ecma_class_punctuation_is_literal() {
    for (source, valid, invalid) in [
        ("^[a&&b]+$", vec!["a", "b", "&", "a&b"], vec!["c"]),
        ("^[a-z--]+$", vec!["a", "z", "-"], vec!["&"]),
        ("^[--a]+$", vec!["a", "A", "-", "["], vec!["b", "z"]),
        ("^[a~~b]+$", vec!["a", "b", "~"], vec!["c"]),
        ("^[a[b]+$", vec!["a", "b", "["], vec!["]"]),
    ] {
        let pattern = string_members::smithy_pattern("test.Classes", "text", source).unwrap();
        let matcher = CompiledPattern::try_new(&pattern).unwrap();
        for text in valid {
            assert!(matcher.is_match(text), "{source}: {text:?}");
        }
        for text in invalid {
            assert!(!matcher.is_match(text), "{source}: {text:?}");
        }
    }
}
