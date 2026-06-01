//! AWS resource schema definitions

pub mod generated;
pub mod types;

use carina_core::schema::ResourceSchema;

/// Returns all AWS schemas
pub fn all_schemas() -> Vec<ResourceSchema> {
    generated::configs().into_iter().map(|c| c.schema).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use carina_core::schema::{AttributeType, SchemaKind, Shape};

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

    fn collect_string_enum_identities(
        attr: &AttributeType,
        path: &str,
        identities: &mut Vec<(String, String)>,
    ) {
        match attr.shape_ref_free().expect("generated schema is Ref-free") {
            Shape::StringEnum {
                identity: Some(identity),
                ..
            } => identities.push((path.to_string(), identity.to_string())),
            Shape::List { inner, .. } => {
                collect_string_enum_identities(inner, &format!("{path}[]"), identities)
            }
            Shape::Map { key, value } => {
                collect_string_enum_identities(key, &format!("{path}.<key>"), identities);
                collect_string_enum_identities(value, &format!("{path}.<value>"), identities);
            }
            Shape::Struct { fields, .. } => {
                for field in fields {
                    collect_string_enum_identities(
                        &field.field_type,
                        &format!("{path}.{}", field.name),
                        identities,
                    );
                }
            }
            Shape::Union(members) => {
                for (idx, member) in members.iter().enumerate() {
                    collect_string_enum_identities(
                        member,
                        &format!("{path}.<union:{idx}>"),
                        identities,
                    );
                }
            }
            _ => {}
        }
    }

    #[test]
    fn acm_certificate_nested_string_enum_identities_are_structural_and_unique() {
        let config = super::generated::acm::certificate::acm_certificate_config();
        let mut identities = Vec::new();
        for (name, attr) in &config.schema.attributes {
            collect_string_enum_identities(&attr.attr_type, name, &mut identities);
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
            "StringEnum identities must be unique within acm.Certificate; duplicates: {duplicates:?}"
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
}
