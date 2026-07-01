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

    use carina_core::schema::{AttributeType, RawShape, ResourceSchema, SchemaKind, Shape};

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
                    collect_enum_identities(member, &format!("{path}.<union:{idx}>"), identities);
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
    fn generated_resource_unique_name_attributes_follow_writable_identifiers() {
        let iam_role = super::generated::iam::role::iam_role_config();
        assert_eq!(
            iam_role.schema.unique_name_attribute,
            Some("role_name".to_string())
        );

        let ec2_vpc = super::generated::ec2::vpc::ec2_vpc_config();
        assert_eq!(ec2_vpc.schema.unique_name_attribute, None);

        let ec2_route = super::generated::ec2::route::ec2_route_config();
        assert_eq!(ec2_route.schema.unique_name_attribute, None);

        let ec2_vpc_gateway_attachment =
            super::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config();
        assert_eq!(
            ec2_vpc_gateway_attachment.schema.unique_name_attribute,
            None
        );

        let route53_record_set = super::generated::route53::record_set::route53_record_set_config();
        assert_eq!(route53_record_set.schema.unique_name_attribute, None);
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
