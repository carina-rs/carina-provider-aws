//! Compile/admit actual generated schemas without linking the provider's SDKs.
#![allow(dead_code)]

pub mod config {
    pub use carina_aws_types::*;
    pub struct ReadBackRequirement;
    impl ReadBackRequirement {
        pub fn new(_: &'static str, _: ReadBackPredicate) -> Self {
            Self
        }
    }
    pub struct AwsSchemaConfig {
        pub aws_type_name: &'static str,
        pub resource_type_name: &'static str,
        pub has_tags: bool,
        pub await_on_read_back: Vec<ReadBackRequirement>,
        pub schema: carina_core::schema::ResourceSchema,
    }
}
pub mod schemas {
    pub use super::config;
}
#[path = "../../carina-provider-aws/src/schemas/generated/mod.rs"]
mod generated;

#[path = "support/identity_dispatch.rs"]
mod identity_dispatch;

#[test]
fn arn_helpers_match_the_attribute_declaration() {
    use carina_core::schema::{AttributeType, RawShape};
    fn metadata(ty: &AttributeType) -> String {
        let RawShape::String {
            identity_layer:
                carina_core::schema::StringIdentityParts {
                    identity,
                    pattern,
                    length,
                    validate,
                    ..
                },
            ..
        } = ty.raw_shape()
        else {
            panic!("String")
        };
        format!("{identity:?} {pattern:?} {length:?} {}", validate.is_some())
    }
    for (config, helper) in [
        (
            generated::iam::role::iam_role_config(),
            generated::iam::role::arn(),
        ),
        (
            generated::organizations::account::organizations_account_config(),
            generated::organizations::account::arn(),
        ),
        (
            generated::organizations::organization::organizations_organization_config(),
            generated::organizations::organization::arn(),
        ),
        (
            generated::s3::bucket_data_source::s3_bucket_data_source_config(),
            generated::s3::bucket_data_source::arn(),
        ),
        (
            generated::sts::caller_identity::sts_caller_identity_config(),
            generated::sts::caller_identity::arn(),
        ),
    ] {
        let attribute = config.schema.attributes().get("arn").unwrap();
        assert_eq!(
            metadata(attribute.attr_type()),
            metadata(&helper),
            "{}",
            config.resource_type_name
        );
    }
}

#[test]
fn role_path_rule_is_create_only() {
    use carina_core::schema::{InputPredicate, InputRuleScope};
    let config = generated::configs()
        .into_iter()
        .find(|c| c.resource_type_name == "iam.Role")
        .unwrap();
    let rule = config
        .schema
        .input_rules()
        .iter()
        .find(|r| {
            r.path().as_str() == "path" && matches!(r.predicate(), InputPredicate::Matches(_))
        })
        .expect("CreateRole path narrowing");
    assert_eq!(rule.scope(), InputRuleScope::CreateOnly);
}

#[test]
fn bucket_hosted_zone_id_rejects_40_characters() {
    use carina_core::schema::ValidatedConfigSchema;
    use std::collections::BTreeMap;
    let mut views = 0;
    for config in generated::configs()
        .into_iter()
        .filter(|c| c.resource_type_name == "s3.Bucket")
    {
        views += 1;
        let ty = config
            .schema
            .attributes()
            .get("hosted_zone_id")
            .unwrap()
            .attr_type()
            .clone();
        let schema =
            ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
        let value: carina_core::resource::Value = carina_core::resource::Value::Concrete(
            carina_core::resource::ConcreteValue::String(format!("Z{}", "A".repeat(39)).into()),
        );
        assert!(
            schema
                .rootless_type("value")
                .unwrap()
                .validate(&value)
                .is_err()
        );
    }
    assert_eq!(views, 2, "resource and data-source views");
}

#[test]
fn flow_log_enum_metadata_uses_real_top_level_values() {
    let (_, entries) = generated::ec2::flow_log::enum_valid_values();
    let values = entries
        .iter()
        .find(|(path, _)| *path == "resource_type")
        .expect("root resource_type")
        .1;
    assert_eq!(
        values,
        [
            "NetworkInterface",
            "RegionalNatGateway",
            "Subnet",
            "TransitGateway",
            "TransitGatewayAttachment",
            "VPC"
        ]
    );
}

#[test]
fn generated_schemas_compile_and_admit() {
    let configs = generated::configs();
    assert!(!configs.is_empty());
    for config in configs {
        config
            .schema
            .validate()
            .unwrap_or_else(|error| panic!("{}: {error}", config.resource_type_name));
    }
}

#[test]
fn account_name_rule_and_smithy_constraints_both_apply() {
    use carina_core::schema::{CompiledPattern, InputPredicate, InputRuleScope, RawShape};
    let config = generated::configs()
        .into_iter()
        .find(|config| config.resource_type_name == "organizations.Account")
        .unwrap();
    let rule = config
        .schema
        .input_rules()
        .iter()
        .find(|rule| {
            rule.path().as_str() == "account_name"
                && matches!(rule.predicate(), InputPredicate::Matches(_))
        })
        .expect("account_name must have a portable Matches input rule");
    assert_eq!(rule.scope(), InputRuleScope::EveryWrite);
    let InputPredicate::Matches(pattern) = rule.predicate() else {
        unreachable!()
    };
    let matcher = CompiledPattern::try_new(pattern).unwrap();
    assert!(matcher.is_match("ab"));
    assert!(!matcher.is_match("a<b"));
    assert!(!matcher.is_match("a>b"));
    let attribute = config.schema.attributes().get("account_name").unwrap();
    let RawShape::String {
        member_layer:
            carina_core::schema::StringMemberParts {
                pattern: Some(source),
                length,
                ..
            },
        ..
    } = attribute.attr_type().raw_shape()
    else {
        panic!("Smithy String constraints")
    };
    assert_eq!(source.as_str(), r"^[\u0020-\u007E]+$");
    assert_eq!(length, Some((Some(1), Some(50))));
    let source = CompiledPattern::try_new(source).unwrap();
    assert!(
        source.is_match("a<b"),
        "the extra rule must not occupy the Smithy pattern slot"
    );
    assert!(!source.is_match("a\nb"));
}

#[test]
fn log_group_class_preserves_main_values() {
    let config = generated::configs()
        .into_iter()
        .find(|config| config.resource_type_name == "logs.LogGroup")
        .unwrap();
    let attribute = config.schema.attributes().get("log_group_class").unwrap();
    let carina_core::schema::RawShape::Enum {
        values: Some(values),
        ..
    } = attribute.attr_type().raw_shape()
    else {
        panic!("closed LogGroupClass")
    };
    let mut values = values.to_vec();
    values.sort();
    assert_eq!(
        values,
        ["DELIVERY", "ESSENTIALS", "INFREQUENT_ACCESS", "STANDARD"]
    );
}

#[test]
fn email_keeps_builtin_identity_and_smithy_member_layers() {
    use carina_core::schema::{RawShape, TypeIdentity};
    let config = generated::configs()
        .into_iter()
        .find(|c| c.resource_type_name == "identitystore.User")
        .unwrap();
    let RawShape::List { element_type, .. } =
        config.schema.attributes()["emails"].attr_type().raw_shape()
    else {
        panic!("email list")
    };
    let RawShape::String {
        identity_layer,
        member_layer,
    } = element_type.raw_shape()
    else {
        panic!("Email")
    };
    assert_eq!(identity_layer.identity, Some(&TypeIdentity::bare("Email")));
    assert!(
        identity_layer.pattern.is_none()
            && identity_layer.length.is_none()
            && identity_layer.validate.is_some()
    );
    assert!(member_layer.pattern.is_some());
    assert_eq!(member_layer.length, Some((Some(1), Some(1024))));
}
