use carina_core::schema::{
    Compiled, InputCondition, InputLiteral, InputPath, InputPredicate, InputRule, InputRuleCode,
    InputRuleDisplay, InputRuleScope, InputRuleTarget, NonEmptyString, PatternForm,
};
use carina_docgen::{Heading, PageBuilder, TableCell};

fn expected_section<R: PatternForm>(rules: &[InputRule<R>]) -> String {
    let mut page = PageBuilder::<()>::new();
    page.heading(Heading::plain(2, "Additional Input Constraints"))
        .unwrap();
    page.table(
        ["Input rule"],
        rules
            .iter()
            .map(|rule| [TableCell::code(InputRuleDisplay::new(rule).to_string())]),
    )
    .unwrap();
    page.push_str("\n");
    page.finish().unwrap()
}

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
            InputRuleCode::new("test.bucket.not_ipv4").unwrap(),
            InputPredicate::NotIpv4,
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

    assert_eq!(markdown, expected_section(&rules));
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
    assert_eq!(page.finish().unwrap(), expected_section(&rules));
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
