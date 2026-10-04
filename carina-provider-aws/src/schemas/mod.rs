//! AWS resource schema definitions

pub mod config;
pub mod generated;

use carina_core::schema::ResourceSchema;

/// Returns all AWS schemas
pub fn all_schemas() -> Vec<ResourceSchema> {
    generated::configs().into_iter().map(|c| c.schema).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use carina_core::differ::{Diff, diff};
    use carina_core::executor::normalized::apply_desired_normalization;
    use carina_core::provider::{ProviderFactory, ProviderNormalizer};
    use carina_core::resource::{ConcreteValue, ResolvedResource, Resource, State, Value};
    use carina_core::schema::{
        AttributeType, RawShape, ResourceSchema, SchemaKind, SchemaRegistry, Shape,
        ShapeWalkBudget, TypeInSchema, UniqueNameSpec,
    };
    use indexmap::IndexMap;

    use crate::{AwsNormalizer, AwsProviderFactory};

    #[test]
    fn configs_register_s3_bucket_under_both_kinds() {
        let configs = super::generated::configs();
        let managed = configs
            .iter()
            .find(|c| c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::Resource);
        let data_source = configs.iter().find(|c| {
            c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::DataSource
        });
        assert!(
            managed.is_some(),
            "Managed s3.Bucket missing from configs()"
        );
        assert!(
            data_source.is_some(),
            "DataSource s3.Bucket missing from configs()"
        );
    }

    fn collect_enum_identities(
        attr: &AttributeType,
        path: &str,
        identities: &mut Vec<(String, String)>,
    ) {
        match attr.shape_ref_free().expect("generated schema is Ref-free") {
            Shape::Enum { identity, .. } => {
                identities.push((path.to_string(), identity.to_string()))
            }
            Shape::List { element_type, .. } => {
                collect_enum_identities(element_type, &format!("{path}[]"), identities)
            }
            Shape::Map { key, value } => {
                collect_enum_identities(key, &format!("{path}.<key>"), identities);
                collect_enum_identities(value, &format!("{path}.<value>"), identities);
            }
            Shape::Struct { .. } => {
                let mut budget = carina_core::schema::ShapeWalkBudget::new(64);
                let Some(fields) = attr
                    .struct_fields_ref_free_with_budget(&mut budget)
                    .expect("generated schema is Ref-free")
                else {
                    return;
                };
                for field in fields {
                    collect_enum_identities(
                        &field.field_type,
                        &format!("{path}.{}", field.name),
                        identities,
                    );
                }
            }
            Shape::Union => {
                let mut budget = carina_core::schema::ShapeWalkBudget::new(64);
                let Some(members) = attr
                    .union_members_ref_free_with_budget(&mut budget)
                    .expect("generated schema is Ref-free")
                else {
                    return;
                };
                for (idx, member) in members.iter().enumerate() {
                    let member = member.expect("generated schema union member is Ref-free");
                    collect_enum_identities(
                        member.as_attr(),
                        &format!("{path}.<union:{idx}>"),
                        identities,
                    );
                }
            }
            _ => {}
        }
    }

    fn attr_type<'a>(schema: &'a ResourceSchema, name: &str) -> &'a AttributeType {
        &schema
            .attributes
            .get(name)
            .unwrap_or_else(|| panic!("missing attribute {name}"))
            .attr_type
    }

    fn struct_field_type<'a>(
        schema: &'a ResourceSchema,
        attr_name: &str,
        field_name: &str,
    ) -> &'a AttributeType {
        let mut budget = ShapeWalkBudget::new(8);
        &schema
            .struct_fields_with_budget(attr_type(schema, attr_name), &mut budget)
            .unwrap_or_else(|| panic!("{attr_name} is not a struct"))
            .iter()
            .find(|field| field.name == field_name)
            .unwrap_or_else(|| panic!("missing {attr_name}.{field_name}"))
            .field_type
    }

    fn alias_target_value(hosted_zone_id: ConcreteValue) -> Value {
        Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "dns_name".to_string(),
                Value::Concrete(ConcreteValue::String("target.example.com".to_string())),
            ),
            (
                "evaluate_target_health".to_string(),
                Value::Concrete(ConcreteValue::Bool(false)),
            ),
            (
                "hosted_zone_id".to_string(),
                Value::Concrete(hosted_zone_id),
            ),
        ])))
    }

    fn aws_schema_registry() -> SchemaRegistry {
        let mut registry = SchemaRegistry::new();
        for schema in super::all_schemas() {
            registry.insert("aws", schema);
        }
        registry
    }

    fn assert_refined_string(
        attr: &AttributeType,
        expected_identity: &str,
        expected_pattern: Option<&str>,
    ) {
        let RawShape::String {
            identity, pattern, ..
        } = attr.raw_shape()
        else {
            panic!("expected refined String, got {:?}", attr.raw_shape());
        };
        assert_eq!(
            identity.map(|id| id.to_string()).as_deref(),
            Some(expected_identity)
        );
        assert_eq!(pattern, expected_pattern);
    }

    fn assert_refined_int_range(attr: &AttributeType, expected_range: (Option<i64>, Option<i64>)) {
        let RawShape::Int { range, .. } = attr.raw_shape() else {
            panic!("expected refined Int, got {:?}", attr.raw_shape());
        };
        assert_eq!(range, Some(expected_range));
    }

    #[test]
    fn acm_certificate_nested_string_enum_identities_are_structural_and_unique() {
        let config = super::generated::acm::certificate::acm_certificate_config();
        let mut identities = Vec::new();
        for (name, attr) in &config.schema.attributes {
            collect_enum_identities(&attr.attr_type, name, &mut identities);
        }

        let mut by_identity: HashMap<&str, Vec<&str>> = HashMap::new();
        for (path, identity) in &identities {
            by_identity
                .entry(identity.as_str())
                .or_default()
                .push(path.as_str());
        }
        let duplicates: Vec<_> = by_identity
            .into_iter()
            .filter(|(_, paths)| paths.len() > 1)
            .collect();
        assert!(
            duplicates.is_empty(),
            "enum identities must be unique within acm.Certificate; duplicates: {duplicates:?}"
        );

        let by_path: HashMap<_, _> = identities.into_iter().collect();
        assert_eq!(
            by_path.get("options.certificate_transparency_logging_preference"),
            Some(
                &"aws.acm.Certificate.CertificateOptions.CertificateTransparencyLoggingPreference"
                    .to_string()
            )
        );
        assert_eq!(
            by_path.get("domain_validation_options[].resource_record.type"),
            Some(&"aws.acm.Certificate.DomainValidation.ResourceRecord.Type".to_string())
        );
        assert_eq!(
            by_path.get("domain_validation_options[].validation_method"),
            Some(&"aws.acm.Certificate.DomainValidation.ValidationMethod".to_string())
        );
        assert_eq!(
            by_path.get("renewal_summary.domain_validation_options[].resource_record.type"),
            Some(
                &"aws.acm.Certificate.RenewalSummary.DomainValidation.ResourceRecord.Type"
                    .to_string()
            )
        );
        assert_eq!(
            by_path.get("renewal_summary.domain_validation_options[].validation_method"),
            Some(
                &"aws.acm.Certificate.RenewalSummary.DomainValidation.ValidationMethod".to_string()
            )
        );
    }

    #[test]
    fn generated_acceptance_refined_primitives_are_direct_shapes() {
        let iam_role = super::generated::iam::role::iam_role_config();
        assert_refined_int_range(
            attr_type(&iam_role.schema, "max_session_duration"),
            (Some(3600), Some(43200)),
        );
        assert_refined_string(
            attr_type(&iam_role.schema, "arn"),
            "aws.iam.Role.Arn",
            Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$"),
        );

        let route53_record_set = super::generated::route53::record_set::route53_record_set_config();
        assert_refined_int_range(
            attr_type(&route53_record_set.schema, "ttl"),
            (Some(0), Some(2147483647)),
        );

        let s3_bucket = super::generated::s3::bucket_data_source::s3_bucket_data_source_config();
        assert_refined_string(
            attr_type(&s3_bucket.schema, "arn"),
            "aws.s3.Bucket.Arn",
            Some("^arn:(aws|aws-cn|aws-us-gov):s3:::.+$"),
        );

        let ec2_vpc = super::generated::ec2::vpc::ec2_vpc_config();
        assert_refined_string(
            attr_type(&ec2_vpc.schema, "vpc_id"),
            "aws.ec2.Vpc.Id",
            Some("^vpc-[0-9a-f]{8,}$"),
        );

        let ec2_nat_gateway = super::generated::ec2::nat_gateway::ec2_nat_gateway_config();
        assert_refined_string(
            attr_type(&ec2_nat_gateway.schema, "subnet_id"),
            "aws.ec2.Subnet.Id",
            Some("^subnet-[0-9a-f]{8,}$"),
        );
        assert_refined_string(
            attr_type(&ec2_nat_gateway.schema, "vpc_id"),
            "aws.ec2.Vpc.Id",
            Some("^vpc-[0-9a-f]{8,}$"),
        );
    }

    #[test]
    fn generated_resource_unique_name_specs_follow_writable_identifiers_and_overrides() {
        let iam_role = super::generated::iam::role::iam_role_config();
        assert_eq!(
            iam_role.schema.unique_name,
            UniqueNameSpec::Attribute("role_name".to_string())
        );

        let ec2_vpc = super::generated::ec2::vpc::ec2_vpc_config();
        assert_eq!(ec2_vpc.schema.unique_name, UniqueNameSpec::Coexisting);

        let ec2_route = super::generated::ec2::route::ec2_route_config();
        assert_eq!(ec2_route.schema.unique_name, UniqueNameSpec::Conflicting);

        let ec2_vpc_gateway_attachment =
            super::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config();
        assert_eq!(
            ec2_vpc_gateway_attachment.schema.unique_name,
            UniqueNameSpec::Conflicting
        );

        let route53_record_set = super::generated::route53::record_set::route53_record_set_config();
        assert_eq!(
            route53_record_set.schema.unique_name,
            UniqueNameSpec::Conflicting
        );
    }

    #[test]
    fn route53_alias_target_hosted_zone_id_enforces_assignable_source_identities() {
        let record_set = super::generated::route53::record_set::route53_record_set_config();
        let sink = struct_field_type(&record_set.schema, "alias_target", "hosted_zone_id");
        assert!(matches!(record_set.schema.shape_of(sink), Shape::Union));

        let route53_id = carina_aws_types::route53_hosted_zone_id();
        assert!(
            TypeInSchema::schemaless(&route53_id)
                .is_assignable_to(record_set.schema.type_in_schema(sink)),
            "Route 53 hosted-zone IDs must be assignable to alias_target.hosted_zone_id"
        );

        let cloudfront_id = carina_aws_types::cloudfront_hosted_zone_id();
        assert!(
            TypeInSchema::schemaless(&cloudfront_id)
                .is_assignable_to(record_set.schema.type_in_schema(sink)),
            "the CloudFront global hosted-zone enum must remain assignable"
        );

        let plain_string = AttributeType::string();
        assert!(
            !TypeInSchema::schemaless(&plain_string)
                .is_assignable_to(record_set.schema.type_in_schema(sink)),
            "an unrefined String source must not satisfy an identified sink"
        );

        let unrelated_id = carina_aws_types::iam_role_arn();
        assert!(
            !TypeInSchema::schemaless(&unrelated_id)
                .is_assignable_to(record_set.schema.type_in_schema(sink)),
            "an unrelated identified String must not satisfy the hosted-zone sink"
        );
    }

    #[test]
    fn route53_alias_target_hosted_zone_id_validates_cloudfront_and_route53_values() {
        let record_set = super::generated::route53::record_set::route53_record_set_config();
        let accepted = [
            (
                "CloudFront namespaced constant",
                ConcreteValue::enum_identifier("aws.cloudfront.HostedZoneId.global"),
            ),
            (
                "CloudFront wire literal",
                ConcreteValue::String("Z2FDTNDATAQYW2".to_string()),
            ),
            (
                "ALB hosted-zone literal",
                ConcreteValue::String("Z14GRHDCWA56QT".to_string()),
            ),
            (
                "measured Route 53 hosted-zone literal",
                ConcreteValue::String("Z05136711ZXUHBDOA8D5O".to_string()),
            ),
        ];

        for (label, hosted_zone_id) in accepted {
            let attributes = HashMap::from([
                (
                    "alias_target".to_string(),
                    alias_target_value(hosted_zone_id),
                ),
                (
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String("www.example.com".to_string())),
                ),
                (
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier(
                        "aws.route53.RecordSet.Type.a",
                    )),
                ),
            ]);
            let result = record_set.schema.validate(&attributes);
            assert!(result.is_ok(), "{label} should validate: {result:?}");
        }

        let too_long = format!("Z{}", "A".repeat(32));
        assert_eq!(too_long.len(), 33);
        for (label, hosted_zone_id) in [
            ("empty literal", ""),
            ("bare enum member as a quoted string", "global"),
            (
                "qualified DSL path as a quoted string",
                "aws.cloudfront.HostedZoneId.global",
            ),
            ("lowercase hosted-zone literal", "z14grhdcwa56qt"),
            (
                "Route 53 API path-prefixed ID",
                "/hostedzone/Z05136711ZXUHBDOA8D5O",
            ),
            ("33-character hosted-zone literal", too_long.as_str()),
        ] {
            let attributes = HashMap::from([
                (
                    "alias_target".to_string(),
                    alias_target_value(ConcreteValue::String(hosted_zone_id.to_string())),
                ),
                (
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String("www.example.com".to_string())),
                ),
                (
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier(
                        "aws.route53.RecordSet.Type.a",
                    )),
                ),
            ]);
            let result = record_set.schema.validate(&attributes);
            assert!(result.is_err(), "{label} should be rejected");
        }
    }

    async fn normalize_alias_target_hosted_zone_id_and_assert_no_change(
        label: &str,
        desired_hosted_zone_id: ConcreteValue,
        read_back_hosted_zone_id: &str,
    ) -> ConcreteValue {
        let registry = aws_schema_registry();
        let factories: Vec<Box<dyn ProviderFactory>> = vec![Box::new(AwsProviderFactory)];
        let mut desired = Resource::with_provider("aws", "route53.RecordSet", "record", None);
        desired.set_attr("alias_target", alias_target_value(desired_hosted_zone_id));
        let id = desired.id.clone();

        let normalized =
            apply_desired_normalization(desired, &[], &AwsNormalizer, &factories, &registry).await;
        let Some(Value::Concrete(ConcreteValue::Map(alias_target))) =
            normalized.as_resource().get_attr("alias_target")
        else {
            panic!("normalized alias_target must remain a map");
        };
        let Some(Value::Concrete(normalized_hosted_zone_id)) = alias_target.get("hosted_zone_id")
        else {
            panic!("normalized hosted_zone_id must be concrete");
        };
        let normalized_hosted_zone_id = normalized_hosted_zone_id.clone();

        let current_attributes = HashMap::from([(
            "alias_target".to_string(),
            alias_target_value(ConcreteValue::String(read_back_hosted_zone_id.to_string())),
        )]);
        let mut current_states =
            HashMap::from([(id.clone(), State::existing(id.clone(), current_attributes))]);
        carina_core::value::canonicalize_states_with_schemas(&mut current_states, &registry);
        AwsNormalizer.normalize_state(&mut current_states).await;
        let current = current_states.remove(&id).expect("current RecordSet state");
        let schema = registry
            .get("aws", "route53.RecordSet", SchemaKind::Resource)
            .expect("RecordSet resource schema");
        let result = diff(
            &ResolvedResource::new(normalized.as_resource().clone()),
            &current,
            None,
            None,
            Some(schema),
        );
        assert!(
            matches!(result, Diff::NoChange(_)),
            "{label}: read-back wire value must not cause a phantom diff: {result:?}"
        );

        normalized_hosted_zone_id
    }

    #[tokio::test]
    async fn route53_alias_target_cloudfront_global_normalizes_and_diffs_without_change() {
        let hosted_zone_id = normalize_alias_target_hosted_zone_id_and_assert_no_change(
            "CloudFront namespaced constant",
            ConcreteValue::enum_identifier("aws.cloudfront.HostedZoneId.global"),
            "Z2FDTNDATAQYW2",
        )
        .await;
        let ConcreteValue::CanonicalEnum(canonical) = hosted_zone_id else {
            panic!("CloudFront global must canonicalize to CanonicalEnum");
        };
        assert_eq!(canonical.api_value(), "Z2FDTNDATAQYW2");
    }

    #[tokio::test]
    async fn route53_alias_target_literal_hosted_zone_ids_normalize_and_diff_without_change() {
        for (label, hosted_zone_id) in [
            ("ALB hosted-zone literal", "Z14GRHDCWA56QT"),
            ("CloudFront wire literal", "Z2FDTNDATAQYW2"),
        ] {
            let normalized = normalize_alias_target_hosted_zone_id_and_assert_no_change(
                label,
                ConcreteValue::String(hosted_zone_id.to_string()),
                hosted_zone_id,
            )
            .await;
            match normalized {
                ConcreteValue::CanonicalEnum(canonical) => {
                    assert_eq!(canonical.api_value(), hosted_zone_id, "{label}")
                }
                ConcreteValue::String(value) => assert_eq!(value, hosted_zone_id, "{label}"),
                other => panic!("{label}: unexpected normalized value: {other:?}"),
            }
        }
    }

    #[test]
    fn s3_bucket_hosted_zone_id_outputs_are_assignable_to_route53_alias_target() {
        let record_set = super::generated::route53::record_set::route53_record_set_config();
        let sink = struct_field_type(&record_set.schema, "alias_target", "hosted_zone_id");
        let bucket = super::generated::s3::bucket::s3_bucket_config();
        let bucket_data_source =
            super::generated::s3::bucket_data_source::s3_bucket_data_source_config();

        for (label, source_schema) in [
            ("s3.Bucket resource", &bucket.schema),
            ("s3.Bucket data source", &bucket_data_source.schema),
        ] {
            let source = attr_type(source_schema, "hosted_zone_id");
            assert_refined_string(source, "aws.route53.HostedZone.Id", Some("^Z[A-Z0-9]+$"));
            assert!(
                source_schema
                    .type_in_schema(source)
                    .is_assignable_to(record_set.schema.type_in_schema(sink)),
                "{label} hosted_zone_id must be assignable to the Route 53 alias target"
            );
        }
    }

    #[test]
    fn security_group_ip_protocol_values_are_canonical_and_aliases_reverse() {
        for resource_type in ["ec2.SecurityGroupIngress", "ec2.SecurityGroupEgress"] {
            let values = super::generated::get_enum_valid_values(resource_type, "ip_protocol")
                .unwrap_or_else(|| panic!("{resource_type}.ip_protocol values should exist"));

            assert!(
                values.contains(&"tcp")
                    && values.contains(&"udp")
                    && values.contains(&"icmp")
                    && values.contains(&"icmpv6")
                    && values.contains(&"-1"),
                "{resource_type}.ip_protocol should include canonical wire values: {values:?}"
            );
            assert!(
                !values.contains(&"all") && !values.contains(&"_1"),
                "{resource_type}.ip_protocol values must not include DSL aliases: {values:?}"
            );

            assert_eq!(
                super::generated::get_enum_alias_reverse(resource_type, "ip_protocol", "all"),
                Some("-1")
            );
            assert_eq!(
                super::generated::get_enum_alias_reverse(resource_type, "ip_protocol", "_1"),
                Some("-1")
            );

            let aliases = super::generated::build_enum_aliases_map();
            let ip_protocol_aliases = aliases
                .get(resource_type)
                .and_then(|attrs| attrs.get("ip_protocol"))
                .unwrap_or_else(|| panic!("{resource_type}.ip_protocol aliases should exist"));
            assert_eq!(
                ip_protocol_aliases.get("all").map(String::as_str),
                Some("-1")
            );
            assert_eq!(
                ip_protocol_aliases.get("_1").map(String::as_str),
                Some("-1")
            );
        }
    }
}
