use super::*;
use carina_core::schema::CompiledPattern;
use carina_core::schema::InputRuleScope;

#[test]
fn role_path_keeps_smithy_search_and_adds_a_full_rule() {
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
fn nested_input_targets_must_exist_and_be_user_authored() {
    let attr = tests::test_attr(
        "Options",
        "options",
        "super::sqs_redrive_policy()",
        false,
        false,
        false,
        None,
    );
    let patterns = BTreeMap::from([(
        "options.absent".into(),
        vec![PatternSpec::new("^x$", PatternMatch::Full)],
    )]);
    let error = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap_err();
    assert!(error.to_string().contains("test.Nested.options.absent"));
}

#[test]
fn nested_provider_populated_child_is_rejected() {
    use carina_core::schema::StructField;
    let mut attr = tests::test_attr(
        "Options",
        "options",
        "AttributeType::string()",
        false,
        false,
        false,
        None,
    );
    attr.attribute_type = AttributeType::struct_(
        "Options",
        vec![
            StructField::provider_populated(
                "child",
                AttributeType::struct_(
                    "Child",
                    vec![StructField::optional("label", AttributeType::string()).build()],
                ),
            )
            .build(),
        ],
    );
    let patterns = BTreeMap::from([(
        "options.child.label".into(),
        vec![PatternSpec::new("^x$", PatternMatch::Full)],
    )]);
    let error = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("test.Nested.options.child.label")
            && error.to_string().contains("provider-populated"),
        "{error}"
    );
}

#[test]
fn create_only_child_controls_the_whole_path_scope() {
    use carina_core::schema::StructField;
    let mut attr = tests::test_attr(
        "Options",
        "options",
        "AttributeType::string()",
        false,
        false,
        false,
        None,
    );
    let child = AttributeType::struct_(
        "Child",
        vec![StructField::optional("label", AttributeType::string()).build()],
    );
    attr.attribute_type = AttributeType::struct_(
        "Options",
        vec![
            StructField::optional("fixed", child.clone())
                .create_only()
                .build(),
            StructField::optional("mutable", child).build(),
        ],
    );
    let patterns = ["options.fixed.label", "options.mutable.label"]
        .into_iter()
        .map(|p| (p.into(), vec![PatternSpec::new("^x$", PatternMatch::Full)]))
        .collect();
    let rules = bind_input_patterns("test.Nested", &[attr], &patterns).unwrap();
    assert_eq!(rules[0].scope(), InputRuleScope::CreateOnly);
    assert_eq!(rules[1].scope(), InputRuleScope::EveryWrite);
}
