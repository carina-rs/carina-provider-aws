use carina_core::schema::{
    Compiled, InputCondition, InputLiteral, InputPath, InputPredicate, InputRule, InputRuleCode,
    InputRuleScope, InputRuleTarget, NonEmptyString,
};
use carina_docgen::PageBuilder;

#[test]
fn input_rules_section_preserves_core_text_targets_conditions_and_order() {
    let rules: [InputRule; 3] = [
        InputRule::value(
            InputRuleScope::CreateOnly,
            InputPath::new("bucket").unwrap(),
            InputRuleCode::new("test.bucket.no_suffix").unwrap(),
            InputPredicate::NoSuffix(NonEmptyString::new("bad`|suffix").unwrap()),
        ),
        InputRule::value(
            InputRuleScope::CreateOnly,
            InputPath::new("bucket").unwrap(),
            InputRuleCode::new("test.bucket.no_suffix_an").unwrap(),
            InputPredicate::NoSuffix(NonEmptyString::new("-an").unwrap()),
        )
        .with_condition(InputCondition::Not(Box::new(InputCondition::Equals {
            path: InputPath::new("bucket_namespace").unwrap(),
            literal: InputLiteral::String("account-regional".into()),
        }))),
        InputRule::new(
            InputRuleScope::EveryWrite,
            InputRuleTarget::MapKeys(InputPath::new("tags").unwrap()),
            InputRuleCode::new("test.tags.no_prefix").unwrap(),
            InputPredicate::ForbiddenPrefix {
                prefix: NonEmptyString::new("aws:").unwrap(),
                ignore_ascii_case: true,
            },
        ),
    ];
    let mut page = PageBuilder::<()>::new();
    page.input_rules_section(&rules).unwrap();
    let markdown = page.finish().unwrap();

    assert_eq!(
        markdown,
        concat!(
            "## Additional Input Constraints\n\n",
            "| Input rule |\n",
            "|------------|\n",
            r#"| ``value at bucket: must not end with "bad`\|suffix" (on create)`` |"#,
            "\n",
            r#"| `value at bucket: must not end with "-an" when not (bucket_namespace equals "account-regional") (on create)` |"#,
            "\n",
            r#"| `map keys at tags: must not start with "aws:" (ignoring ASCII case) (on every write)` |"#,
            "\n\n",
        )
    );
    assert!(
        markdown
            .contains(r#"| ``value at bucket: must not end with "bad`\|suffix" (on create)`` |"#)
    );
}

#[test]
fn input_rules_section_accepts_compiled_rules() {
    let rules = [InputRule::<Compiled>::value(
        InputRuleScope::EveryWrite,
        InputPath::new("name").unwrap(),
        InputRuleCode::new("test.name.not_ipv4").unwrap(),
        InputPredicate::NotIpv4,
    )];
    let mut page = PageBuilder::<()>::new();
    page.input_rules_section(&rules).unwrap();
    assert_eq!(
        page.finish().unwrap(),
        concat!(
            "## Additional Input Constraints\n\n",
            "| Input rule |\n",
            "|------------|\n",
            "| `value at name: must not have IPv4 dotted-quad form (on every write)` |\n\n",
        )
    );
}

#[test]
fn input_rules_section_writes_nothing_for_empty_rules() {
    for prefix in ["", "Existing content.\n\n"] {
        let mut page = PageBuilder::<()>::new();
        page.push_str(prefix);
        page.input_rules_section::<carina_core::schema::Draft>(&[])
            .unwrap();
        assert_eq!(page.finish().unwrap(), prefix);
    }
}
