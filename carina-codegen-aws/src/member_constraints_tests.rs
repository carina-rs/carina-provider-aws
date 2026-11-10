use super::*;

fn resolve_fixture(
    resource: &str,
    path: &str,
    target: &str,
    model: &SmithyModel,
) -> Result<String> {
    resolve_type(
        &mut TypeResolutionContext {
            model,
            resource_name: resource,
            namespace: "aws.test.Constraints",
            struct_path: Vec::new(),
            resolving_shapes: Vec::new(),
            property_path: path.into(),
            type_overrides: &TypeDeclarations::default(),
            all_enums: &mut BTreeMap::new(),
            current_root_attr: None,
            deferred_populate_struct_fields: &HashSet::new(),
            nested_write_only_struct_fields: &HashSet::new(),
            nested_write_only_emissions: &mut HashMap::new(),
            struct_field_type_overrides: &HashMap::new(),
        },
        target,
        "Label",
    )
    .map(|value| value.0)
}

fn model(pattern: &str) -> SmithyModel {
    let text = format!(
        r#"{{"smithy":"2.0","shapes":{{
        "test#Text":{{"type":"string","traits":{{"smithy.api#length":{{"min":3,"max":9}},"smithy.api#pattern":{pattern:?}}}}},
        "test#Items":{{"type":"list","member":{{"target":"test#Text"}}}},
        "test#Entries":{{"type":"map","key":{{"target":"test#Text"}},"value":{{"target":"test#Text"}}}},
        "test#Object":{{"type":"structure","members":{{"Value":{{"target":"test#Text"}}}}}}
        }}}}"#
    );
    carina_smithy::parse(&text).unwrap()
}

#[test]
fn identity_662_named_member_keeps_smithy_constraints() {
    let code = resolve_fixture(
        "route53.RecordSet",
        "hosted_zone_id",
        "test#Text",
        &model("^zone-[A-Z]+$"),
    )
    .unwrap();
    assert!(code.contains("zone-[A-Z]+"), "{code}");
    assert!(code.contains("Some((Some(3), Some(9)))"), "{code}");
    assert!(code.contains("Route53HostedZoneId"), "{code}");
}

#[test]
fn identity_662_unnamed_member_gets_smithy_constraints() {
    let code =
        resolve_fixture("test.Constraints", "label", "test#Text", &model(r"^\w+\d$")).unwrap();
    assert!(code.contains("[A-Za-z0-9_]"), "{code}");
    assert!(code.contains("[0-9]"), "{code}");
    assert!(code.contains("Some((Some(3), Some(9)))"), "{code}");
}

#[test]
fn identity_662_recursive_members_keep_smithy_constraints() {
    for target in ["test#Items", "test#Entries", "test#Object"] {
        let code =
            resolve_fixture("test.Constraints", "items", target, &model("^zone-[A-Z]+$")).unwrap();
        assert!(code.contains("zone-[A-Z]+"), "{target}: {code}");
        assert!(
            code.contains("Some((Some(3), Some(9)))"),
            "{target}: {code}"
        );
        if target == "test#Entries" {
            assert_eq!(
                code.matches("zone-[A-Z]+").count(),
                2,
                "keys and values: {code}"
            );
        }
    }
}

#[test]
fn identity_662_untranslatable_pattern_reports_its_path() {
    let error = resolve_fixture(
        "test.Constraints",
        "items[].label",
        "test#Text",
        &model("(?<=x)y"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("test.Constraints.items[].label"), "{error}");
    assert!(error.contains("(?<=x)y"), "{error}");
}

#[test]
fn identity_662_unanchored_patterns_keep_search_semantics() {
    let code =
        resolve_fixture("test.Constraints", "label", "test#Text", &model("prefix-")).unwrap();
    assert!(code.contains("PatternMatch::Substring"), "{code}");
    assert!(code.contains("Smithy pattern is unanchored"), "{code}");
}

fn validate_string(ty: AttributeType, value: &str) -> bool {
    use carina_core::resource::{ConcreteValue, Value};
    let is_enum = matches!(ty.raw_shape(), RawShape::Enum { .. });
    let schema = carina_core::schema::ValidatedConfigSchema::validate(BTreeMap::from([(
        "value".into(),
        ty,
    )]))
    .unwrap();
    let value: Value = if is_enum {
        Value::Concrete(ConcreteValue::enum_identifier(value))
    } else {
        Value::Concrete(ConcreteValue::String(value.into()))
    };
    schema
        .rootless_type("value")
        .unwrap()
        .validate(&value)
        .is_ok()
}

#[test]
fn identity_662_member_traits_override_target_traits() {
    let model = model("^target$");
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: BTreeMap::from([
            (
                "smithy.api#length".into(),
                serde_json::json!({"min":4,"max":7}),
            ),
            ("smithy.api#pattern".into(), serde_json::json!("^member$")),
        ]),
    };
    let value = string_members::derive(&model, &member, "test.Constraints", "label", &[]).unwrap();
    assert!(value.type_code.contains("Some((Some(4), Some(7)))"));
    assert!(validate_string(value.attribute_type.clone(), "member"));
    assert!(!validate_string(value.attribute_type, "target"));
}

#[test]
fn identity_662_recursive_member_traits_reach_the_string_seam() {
    let mut model = model("^target$");
    let traits = BTreeMap::from([
        (
            "smithy.api#length".into(),
            serde_json::json!({"min":4,"max":7}),
        ),
        ("smithy.api#pattern".into(), serde_json::json!("^member$")),
    ]);
    if let Some(Shape::List(list)) = model.shapes.get_mut("test#Items") {
        list.member.traits = traits.clone();
    }
    if let Some(Shape::Map(map)) = model.shapes.get_mut("test#Entries") {
        map.key.traits = traits.clone();
        map.value.traits = traits.clone();
    }
    if let Some(Shape::Structure(object)) = model.shapes.get_mut("test#Object") {
        object.members.get_mut("Value").unwrap().traits = traits;
    }
    for target in ["test#Items", "test#Entries", "test#Object"] {
        let code = resolve_fixture("test.Constraints", "items", target, &model).unwrap();
        assert!(code.contains("^member$"), "{code}");
        assert!(!code.contains("^target$"), "{code}");
        assert!(code.contains("Some((Some(4), Some(7)))"), "{code}");
    }
}

#[test]
fn identity_662_ascii_shorthands_and_partial_anchors_validate_as_declared() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    for (pattern, accepted, rejected) in [
        (r"^\w+$", "abc", "ééé"),
        (r"^\d+$", "123", "١٢٣"),
        (r"^\s+$", " \t\u{a0}", "\u{85}\u{85}\u{85}"),
        ("^a|b$", "xxb", "xxx"),
        ("prefix-", "xprefix-", "nothing"),
    ] {
        let value =
            string_members::derive(&model(pattern), &member, "test.Constraints", "label", &[])
                .unwrap();
        assert!(
            validate_string(value.attribute_type.clone(), accepted),
            "{pattern}: {accepted:?}"
        );
        assert!(
            !validate_string(value.attribute_type, rejected),
            "{pattern}: {rejected:?}"
        );
    }
}

#[test]
fn identity_662_callbacks_remain_bound_to_the_member_identity() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value = string_members::derive(
        &model("^allowed.*$"),
        &member,
        "test.Constraints",
        "label",
        &[TypeOverride::Identity(AwsType::Ec2SecurityGroupName)],
    )
    .unwrap();
    assert!(validate_string(value.attribute_type.clone(), "allowed-x"));
    assert!(
        !validate_string(value.attribute_type, "allowed\t"),
        "the callback rejects a value admitted by the member pattern and length"
    );
}

#[test]
fn identity_662_conflicting_patterns_keep_smithy_and_collect_an_input_rule() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let mut declarations = TypeDeclarations::default();
    let extra = PatternSpec::new("^override$", PatternMatch::Full);
    declarations
        .members
        .insert("Label", TypeOverride::StringPattern(extra.clone()));
    let value = member_string_value(
        &model("^model$"),
        "test.Conflict",
        &member,
        "Label",
        "label",
        &declarations,
        &HashMap::new(),
        None,
    )
    .unwrap();
    assert!(validate_string(value.attribute_type, "model"));
    assert_eq!(declarations.input_patterns.borrow()["label"], vec![extra]);
    assert!(
        !string_members::issues()
            .iter()
            .any(|issue| issue["resource"] == "test.Conflict")
    );
}

#[test]
fn identity_662_bounds_intersect_and_empty_intersections_error() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value = string_members::derive(
        &model(".*"),
        &member,
        "test.Constraints",
        "label",
        &[TypeOverride::StringBounds { min: 5, max: 30 }],
    )
    .unwrap();
    assert!(matches!(
        value.attribute_type.raw_shape(),
        RawShape::String {
            member_layer: carina_core::schema::StringMemberParts {
                length: Some((Some(5), Some(9))),
                ..
            },
            ..
        }
    ));
    let error = string_members::derive(
        &model(".*"),
        &member,
        "test.Constraints",
        "label",
        &[TypeOverride::StringBounds { min: 10, max: 30 }],
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("test.Constraints.label"));
}

#[test]
fn identity_662_finite_enum_constraints_are_folded_into_the_allowed_subset() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value = string_members::derive(
        &model("^RSA_[0-9]+$"),
        &member,
        "test.Constraints",
        "algorithm",
        &[TypeOverride::NamedValues {
            identity: AwsType::AcmCertificateKeyAlgorithm,
            values: &["RSA_2048"],
        }],
    )
    .unwrap();
    assert!(validate_string(value.attribute_type.clone(), "rsa_2048"));
    assert!(!validate_string(
        value.attribute_type.clone(),
        "ec_prime256v1"
    ));
    let RawShape::Enum {
        base,
        values: Some(values),
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("finite enum")
    };
    assert_eq!(values, ["RSA_2048"]);
    assert!(matches!(
        base.raw_shape(),
        RawShape::String {
            member_layer: carina_core::schema::StringMemberParts {
                pattern: None,
                length: None,
                ..
            },
            ..
        }
    ));
}

#[test]
fn identity_662_dot_translation_keeps_ecma_line_terminator_semantics() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    let value =
        string_members::derive(&model("^...$"), &member, "test.Constraints", "label", &[]).unwrap();
    assert!(validate_string(value.attribute_type.clone(), "abc"));
    for value_with_line_break in ["a\nb", "a\rb", "a\u{2028}b", "a\u{2029}b"] {
        assert!(!validate_string(
            value.attribute_type.clone(),
            value_with_line_break
        ));
    }
}

pub(super) const ACCOUNT_ARN_SOURCE: &str =
    r"^arn:aws:organizations::[0-9]{12}:account\/o-[a-z0-9]{10,32}\/[0-9]{12}$";
pub(super) const ORGANIZATION_ARN_SOURCE: &str =
    r"^arn:aws:organizations::[0-9]{12}:organization\/o-[a-z0-9]{10,32}$";

fn arn_fixture(source: Option<&str>) -> (SmithyModel, carina_smithy::ShapeRef) {
    let mut traits = carina_smithy::Traits::from([(
        "smithy.api#length".into(),
        serde_json::json!({"min":20,"max":2048}),
    )]);
    if let Some(source) = source {
        traits.insert("smithy.api#pattern".into(), serde_json::json!(source));
    }
    let model = carina_smithy::parse(&serde_json::json!({"smithy":"2.0","shapes":{"test#Arn":{"type":"string","traits":traits}}}).to_string()).unwrap();
    (
        model,
        carina_smithy::ShapeRef {
            target: "test#Arn".into(),
            traits: Default::default(),
        },
    )
}

#[test]
fn identity_662_round2_arn_partition_accepts_every_provider_partition() {
    for (resource, path, row, source, rest) in [
        (
            "organizations.Account",
            "arn",
            AwsType::OrganizationsAccountArn,
            ACCOUNT_ARN_SOURCE,
            "organizations::123456789012:account/o-0123456789/123456789012",
        ),
        (
            "organizations.Organization",
            "arn",
            AwsType::OrganizationsOrganizationArn,
            ORGANIZATION_ARN_SOURCE,
            "organizations::123456789012:organization/o-0123456789",
        ),
        (
            "organizations.Organization",
            "master_account_arn",
            AwsType::OrganizationsAccountArn,
            ACCOUNT_ARN_SOURCE,
            "organizations::123456789012:account/o-0123456789/123456789012",
        ),
    ] {
        let (model, member) = arn_fixture(Some(source));
        let value = string_members::derive(
            &model,
            &member,
            resource,
            path,
            &[TypeOverride::Identity(row)],
        )
        .unwrap();
        for partition in carina_aws_types::Partition::ALL {
            let arn = format!("arn:{}:{rest}", partition.id());
            assert!(
                validate_string(value.attribute_type.clone(), &arn),
                "{resource}.{path}: {arn}"
            );
        }
        for invalid in [
            format!("arn:aws-xx:{rest}"),
            format!("arn:aws-cn:{}", rest.replace("123456789012", "1234")),
            format!("arn:aws-us-gov:{}", rest.replace("o-0123456789", "o-short")),
            format!("arn:aws:{}", rest.replace("organizations:", "iam:")),
        ] {
            assert!(
                !validate_string(value.attribute_type.clone(), &invalid),
                "{resource}.{path}: {invalid}"
            );
        }
        let RawShape::String {
            identity_layer: carina_core::schema::StringIdentityParts { identity, .. },
            member_layer:
                carina_core::schema::StringMemberParts {
                    pattern: Some(pattern),
                    length,
                },
        } = value.attribute_type.raw_shape()
        else {
            panic!("named ARN String")
        };
        assert_eq!(
            pattern.as_str(),
            carina_aws_types::arn_regex(source.strip_prefix("^arn:aws:").unwrap())
        );
        assert_eq!(length, Some((Some(20), Some(2048))));
        assert_eq!(identity, Some(&row.identity()));
    }
}

#[test]
fn identity_662_round2_arn_partition_source_drift_is_an_error() {
    for source in [
        None,
        Some("^arn:aws-cn:organizations:.*$"),
        Some("arn:aws:organizations:.*"),
        Some("^ARN:aws:organizations:.*$"),
    ] {
        let (model, member) = arn_fixture(source);
        let error = string_members::derive(&model, &member, "organizations.Account", "arn", &[])
            .err()
            .expect("the typed correction must reject source drift");
        let message = format!("{error:#}");
        assert!(message.contains("organizations.Account.arn"), "{message}");
        assert!(message.contains("^arn:aws:"), "{message}");
    }
}

#[test]
fn identity_662_round2_arn_partition_note_reaches_attribute_docs() {
    for (resource, path, row, source) in [
        (
            "organizations.Account",
            "arn",
            AwsType::OrganizationsAccountArn,
            ACCOUNT_ARN_SOURCE,
        ),
        (
            "organizations.Organization",
            "arn",
            AwsType::OrganizationsOrganizationArn,
            ORGANIZATION_ARN_SOURCE,
        ),
        (
            "organizations.Organization",
            "master_account_arn",
            AwsType::OrganizationsAccountArn,
            ACCOUNT_ARN_SOURCE,
        ),
    ] {
        let (model, member) = arn_fixture(Some(source));
        let value = string_members::derive(
            &model,
            &member,
            resource,
            path,
            &[TypeOverride::Identity(row)],
        )
        .unwrap();
        let display = value
            .markdown(
                &HeadingPath::root(path),
                &format!("aws.{resource}"),
                &mut BTreeMap::new(),
                &mut MarkdownTypes::default(),
            )
            .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        let markdown = page.finish().unwrap();
        assert!(
            markdown.contains("Partition widened from the Smithy model"),
            "{resource}.{path}: {markdown}"
        );
    }
}

#[test]
fn identity_662_round6_only_truly_equivalent_pattern_overrides_are_absent() {
    for site in [
        ("iam.Roles", "path_prefix"),
        ("organizations.Organization", "id"),
        ("logs.LogGroup", "log_group_name"),
    ] {
        assert!(
            !resource_type_overrides().contains_key(&site),
            "{site:?}: derive these constraints from Smithy"
        );
    }
    assert!(resource_type_overrides().contains_key(&("organizations.Account", "account_name")));
    assert!(matches!(
        resource_type_overrides().get(&("iam.Role", "path")),
        Some(TypeOverride::StringPattern(_))
    ));
}

#[test]
fn identity_662_round4_extra_pattern_on_list_or_map_position_errors() {
    let member = carina_smithy::ShapeRef {
        target: "test#Text".into(),
        traits: Default::default(),
    };
    for path in ["items[]", "entries{}", "entries{key}", "items[].label"] {
        let error = string_members::derive(
            &model(".*"),
            &member,
            "test.InputPattern",
            path,
            &[TypeOverride::StringPattern(PatternSpec::new(
                "^extra$",
                PatternMatch::Full,
            ))],
        )
        .err()
        .expect("unaddressable pattern tightening must fail codegen");
        let message = format!("{error:#}");
        assert!(
            message.contains(&format!("test.InputPattern.{path}")),
            "{message}"
        );
        assert!(message.contains("InputRule"), "{message}");
    }
}

#[test]
fn identity_662_round4_data_source_representation_cannot_swallow_a_pattern() {
    let source = DataSourceType {
        representation: Representation::UnorderedStrings,
        decoration: Some(TypeOverride::StringPattern(PatternSpec::new(
            "^extra$",
            PatternMatch::Full,
        ))),
    };
    let error = data_source_type(&model(".*"), "test.DataSource", "items", "Items", &source)
        .err()
        .expect("InputRules do not apply to provider-populated data-source outputs");
    let message = format!("{error:#}");
    assert!(message.contains("test.DataSource.items"), "{message}");
    assert!(message.contains("InputRule"), "{message}");
}
