//! Explicit KMS string classification, keyed by resource and full property path.

use anyhow::{Result, bail};
use carina_smithy::{Shape, ShapeRef, SmithyModel};
use heck::ToSnakeCase;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Reference { arn_only: bool },
    NotReference(NotReference),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotReference {
    /// The path is deliberately absent from the final provider registry.
    Absent,
    Other(&'static str),
}

/// Shared path rules for Smithy classification and final-registry coverage.
/// Map keys label entries; only their values are KMS-reference inputs.
pub enum PathSegment<'a> {
    Field(&'a str),
    ListElement,
    MapValue,
}

pub fn property_path(parent: &str, segment: PathSegment<'_>) -> String {
    match segment {
        PathSegment::Field(name) => format!("{parent}.{name}"),
        PathSegment::ListElement => format!("{parent}[]"),
        PathSegment::MapValue => format!("{parent}{{}}"),
    }
}

#[derive(Debug)]
pub struct Entry {
    pub resource: &'static str,
    pub path: &'static str,
    pub classification: Classification,
}

/// Inventory of supported KMS-looking strings, including synthetic fields.
pub const CLASSIFICATIONS: &[Entry] = &[
    Entry {
        resource: "logs.LogGroup",
        path: "kms_key_id",
        classification: Classification::Reference { arn_only: true },
    },
    Entry {
        resource: "sqs.Queue",
        path: "kms_master_key_id",
        classification: Classification::Reference { arn_only: false },
    },
    Entry {
        resource: "s3.BucketServerSideEncryptionConfiguration",
        path: "rules[].apply_server_side_encryption_by_default.kms_master_key_id",
        classification: Classification::Reference { arn_only: false },
    },
    Entry {
        resource: "sqs.Queue",
        path: "kms_data_key_reuse_period_seconds",
        classification: Classification::NotReference(NotReference::Other(
            "Duration in seconds, represented as a string in the SQS Attributes map; emitted as Int",
        )),
    },
    Entry {
        resource: "s3.BucketReplicationConfiguration",
        path: "rules[].destination.encryption_configuration.replica_kms_key_id",
        // Omitted from the handwritten replication destination type.
        classification: Classification::NotReference(NotReference::Absent),
    },
];

pub fn classification(resource: &str, path: &str) -> Option<Classification> {
    CLASSIFICATIONS
        .iter()
        .find(|entry| entry.resource == resource && entry.path == path)
        .map(|entry| entry.classification)
}

/// A new candidate requires an explicit decision; it never inherits a name heuristic.
pub fn classify_string(
    resource: &str,
    path: &str,
    name: &str,
    pattern: Option<&str>,
    description: Option<&str>,
) -> Result<Option<Classification>> {
    let name = name.to_ascii_lowercase();
    let evidence = format!(
        "{} {}",
        pattern.unwrap_or_default(),
        description.unwrap_or_default()
    )
    .to_ascii_lowercase();
    let candidate = name.contains("kms")
        || name.contains("cmk")
        || evidence.contains(":kms:")
        || ((evidence.contains("kms") || evidence.contains("key management service"))
            && (evidence.contains("arn") || evidence.contains("amazon resource name")));
    let decision = classification(resource, path);
    if candidate && decision.is_none() {
        bail!("unclassified KMS string {resource}.{path}");
    }
    Ok(decision)
}

/// Constructor selection uses only the classified resource/path pair.
pub fn reference_type_code(resource: &str, path: &str) -> Option<&'static str> {
    match classification(resource, path) {
        Some(Classification::Reference { arn_only: true }) => {
            Some("carina_aws_types::kms_key_arn_reference()")
        }
        Some(Classification::Reference { arn_only: false }) => {
            Some("carina_aws_types::kms_key_reference()")
        }
        _ => None,
    }
}

/// Typed constructor and emitted expression come from the same classification.
pub fn member_definition(
    resource: &str,
    path: &str,
) -> Option<(carina_aws_types::IdentityDefinition, &'static str)> {
    match classification(resource, path) {
        Some(Classification::Reference { arn_only: true }) => Some((
            carina_aws_types::kms_key_arn_reference_definition(),
            "carina_aws_types::kms_key_arn_reference_definition()",
        )),
        Some(Classification::Reference { arn_only: false }) => Some((
            carina_aws_types::AwsType::KmsKeyReference.definition(),
            "carina_aws_types::AwsType::KmsKeyReference.definition()",
        )),
        _ => None,
    }
}

/// Synthetic fields still carry schema evidence through their update layout.
/// This covers the handwritten S3 Rules override before it hides nested shapes.
pub fn validate_synthetic_type(
    model: &SmithyModel,
    resource: &crate::resource_defs::ResourceDef,
    extra: &crate::resource_defs::ExtraField,
) -> Result<()> {
    use crate::resource_defs::FieldLayout;
    let path = extra.name.to_snake_case();
    classify_string(resource.name, &path, extra.name, None, extra.description)?;
    for operation in &resource.update_ops {
        if !operation.fields.field_names().contains(&extra.name) {
            continue;
        }
        let id = format!("{}#{}", resource.service_namespace, operation.operation);
        let Some(input) = model.operation_input(&id) else {
            continue;
        };
        let member = match &operation.fields {
            FieldLayout::Flat(_) => input.members.get(extra.name),
            FieldLayout::InsideStruct { name, .. } => input
                .members
                .get(*name)
                .and_then(|member| model.get_structure(&member.target))
                .and_then(|structure| structure.members.get(extra.name)),
            FieldLayout::CrossServiceFlat { .. } => None,
        };
        if let Some(member) = member {
            validate_smithy_type(model, resource.name, &path, extra.name, member)?;
        }
    }
    Ok(())
}

pub fn validate_data_source(ds: &crate::resource_defs::DataSourceDef) -> Result<()> {
    use crate::resource_defs::DataSourceShape;
    for input in &ds.inputs {
        classify_string(
            ds.name,
            input.name,
            input.name,
            None,
            Some(input.description),
        )?;
    }
    match &ds.shape {
        DataSourceShape::Single { output_attributes } => {
            for output in output_attributes {
                classify_string(
                    ds.name,
                    output.name,
                    output.name,
                    None,
                    Some(output.description),
                )?;
            }
        }
        DataSourceShape::ListAggregated { aggregated_outputs } => {
            for output in aggregated_outputs {
                classify_string(
                    ds.name,
                    &format!("{}[]", output.name),
                    output.name,
                    None,
                    Some(output.description),
                )?;
            }
        }
    }
    Ok(())
}

/// Follow targets, arrays, maps and shared structures with the full property path.
/// An ancestor guard stops recursion without suppressing other uses of a shared type.
pub fn validate_smithy_type(
    model: &SmithyModel,
    resource: &str,
    path: &str,
    name: &str,
    member: &ShapeRef,
) -> Result<()> {
    fn walk(
        model: &SmithyModel,
        resource: &str,
        path: &str,
        name: &str,
        member: &ShapeRef,
        inherited_description: &str,
        ancestors: &mut HashSet<String>,
    ) -> Result<()> {
        if !ancestors.insert(member.target.clone()) {
            return Ok(());
        }
        let description = format!(
            "{inherited_description} {}",
            SmithyModel::documentation(&member.traits).unwrap_or_default()
        );
        let scalar_traits = match model.get_shape(&member.target) {
            Some(Shape::String(shape)) => Some(&shape.traits),
            Some(Shape::Enum(shape)) => Some(&shape.traits),
            _ => None,
        };
        if let Some(traits) = scalar_traits {
            let pattern = member
                .traits
                .get("smithy.api#pattern")
                .or_else(|| traits.get("smithy.api#pattern"))
                .and_then(|v| v.as_str());
            let description = format!(
                "{description} {}",
                SmithyModel::documentation(traits).unwrap_or_default()
            );
            classify_string(resource, path, name, pattern, Some(&description))?;
        } else {
            match model.get_shape(&member.target) {
                Some(Shape::Structure(shape)) => {
                    for (name, member) in &shape.members {
                        walk(
                            model,
                            resource,
                            &property_path(path, PathSegment::Field(&name.to_snake_case())),
                            name,
                            member,
                            "",
                            ancestors,
                        )?;
                    }
                }
                Some(Shape::Union(shape)) => {
                    for (name, member) in &shape.members {
                        walk(
                            model,
                            resource,
                            &property_path(path, PathSegment::Field(&name.to_snake_case())),
                            name,
                            member,
                            "",
                            ancestors,
                        )?;
                    }
                }
                Some(Shape::List(shape)) => {
                    let description = format!(
                        "{description} {}",
                        SmithyModel::documentation(&shape.traits).unwrap_or_default()
                    );
                    walk(
                        model,
                        resource,
                        &property_path(path, PathSegment::ListElement),
                        name,
                        &shape.member,
                        &description,
                        ancestors,
                    )?;
                }
                Some(Shape::Map(shape)) => {
                    let description = format!(
                        "{description} {}",
                        SmithyModel::documentation(&shape.traits).unwrap_or_default()
                    );
                    walk(
                        model,
                        resource,
                        &property_path(path, PathSegment::MapValue),
                        name,
                        &shape.value,
                        &description,
                        ancestors,
                    )?;
                }
                None if member.target == "smithy.api#String" => {
                    classify_string(
                        resource,
                        path,
                        name,
                        member
                            .traits
                            .get("smithy.api#pattern")
                            .and_then(|v| v.as_str()),
                        Some(&description),
                    )?;
                }
                _ => {}
            }
        }
        ancestors.remove(&member.target);
        Ok(())
    }
    walk(model, resource, path, name, member, "", &mut HashSet::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unclassified_candidates_fail_closed() {
        for (name, pattern, description) in [
            ("kms_key_id", None, None),
            ("CmkIdentifier", None, None),
            (
                "encryption_key",
                Some("arn:aws:kms:[^:]+:[0-9]+:key/.+"),
                None,
            ),
            ("encryption_key", None, Some("The ARN of the KMS key.")),
        ] {
            let error = classify_string(
                "test.Resource",
                "settings[].encryption_key",
                name,
                pattern,
                description,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("test.Resource.settings[].encryption_key")
            );
        }
        assert_eq!(
            classify_string(
                "logs.LogGroup",
                "nested.kms_key_id",
                "kms_key_id",
                None,
                None
            )
            .unwrap_err()
            .to_string(),
            "unclassified KMS string logs.LogGroup.nested.kms_key_id"
        );
    }

    #[test]
    fn inventory_distinguishes_references_and_controls() {
        let mut keys = HashSet::new();
        for entry in CLASSIFICATIONS {
            assert!(
                keys.insert((entry.resource, entry.path)),
                "duplicate classification: {entry:?}"
            );
            if let Classification::NotReference(NotReference::Other(reason)) = entry.classification
            {
                assert!(!reason.is_empty());
            }
        }
        assert!(matches!(
            classify_string("logs.LogGroup", "kms_key_id", "kms_key_id", None, None).unwrap(),
            Some(Classification::Reference { .. })
        ));
        assert!(
            matches!(classify_string("sqs.Queue", "kms_data_key_reuse_period_seconds", "kms_data_key_reuse_period_seconds", None, None).unwrap(), Some(Classification::NotReference(NotReference::Other(reason))) if !reason.is_empty())
        );
        assert!(
            classify_string("sqs.Queue", "name", "name", None, None)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            classification(
                "s3.BucketReplicationConfiguration",
                "rules[].destination.encryption_configuration.replica_kms_key_id"
            ),
            Some(Classification::NotReference(NotReference::Absent))
        ));
    }

    #[test]
    fn coverage_follows_arrays_and_shared_targets_with_full_paths() {
        let model = carina_smithy::parse(r#"{
            "smithy":"2.0", "shapes": {
                "test#Rules":{"type":"list","member":{"target":"test#Rule"}},
                "test#Rule":{"type":"structure","members":{"ApplyServerSideEncryptionByDefault":{"target":"test#Shared"}}},
                "test#Shared":{"type":"structure","members":{"KMSMasterKeyID":{"target":"test#String"}}},
                "test#String":{"type":"string","traits":{"smithy.api#pattern":"arn:aws:kms:.*"}}
            }
        }"#).unwrap();
        let member = carina_smithy::ShapeRef {
            target: "test#Rules".into(),
            traits: Default::default(),
        };
        validate_smithy_type(
            &model,
            "s3.BucketServerSideEncryptionConfiguration",
            "rules",
            "Rules",
            &member,
        )
        .unwrap();
        let error = validate_smithy_type(
            &model,
            "s3.BucketServerSideEncryptionConfiguration",
            "other_rules",
            "Rules",
            &member,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains(
                "other_rules[].apply_server_side_encryption_by_default.kms_master_key_id"
            )
        );
    }

    #[test]
    fn coverage_keeps_member_evidence_through_string_containers_and_enums() {
        let model = carina_smithy::parse(r#"{
            "smithy":"2.0", "shapes": {
                "test#Input":{"type":"structure","members":{
                    "Keys":{"target":"test#List","traits":{"smithy.api#documentation":"KMS key ARNs"}},
                    "KeyMap":{"target":"test#Map","traits":{"smithy.api#documentation":"KMS key ARNs"}},
                    "Mode":{"target":"test#Enum","traits":{"smithy.api#documentation":"KMS key ARN"}}
                }},
                "test#List":{"type":"list","member":{"target":"test#String"}},
                "test#Map":{"type":"map","key":{"target":"test#String"},"value":{"target":"test#String"}},
                "test#Enum":{"type":"enum","members":{}},
                "test#String":{"type":"string"}
            }
        }"#).unwrap();
        for (name, path) in [
            ("Keys", "keys[]"),
            ("KeyMap", "key_map{}"),
            ("Mode", "mode"),
        ] {
            let member = &model.get_structure("test#Input").unwrap().members[name];
            let error =
                validate_smithy_type(&model, "test.Resource", &name.to_snake_case(), name, member)
                    .unwrap_err();
            assert!(error.to_string().contains(path), "{error}");
        }
    }

    #[test]
    fn coverage_walks_map_values_and_ignores_entry_keys() {
        let model = carina_smithy::parse(r#"{
            "smithy":"2.0", "shapes": {
                "test#Map":{"type":"map","key":{"target":"test#KmsString"},"value":{"target":"test#Values"}},
                "test#Values":{"type":"list","member":{"target":"test#Value"}},
                "test#Value":{"type":"structure","members":{"Name":{"target":"test#String"}}},
                "test#KmsString":{"type":"string","traits":{"smithy.api#documentation":"KMS key ARN"}},
                "test#String":{"type":"string"}
            }
        }"#).unwrap();
        let member = ShapeRef {
            target: "test#Map".into(),
            traits: Default::default(),
        };
        validate_smithy_type(&model, "test.Resource", "settings", "Settings", &member).unwrap();

        let model = carina_smithy::parse(r#"{
            "smithy":"2.0", "shapes": {
                "test#Map":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"test#Values"}},
                "test#Values":{"type":"list","member":{"target":"test#Value"}},
                "test#Value":{"type":"structure","members":{"Name":{"target":"test#String"}}},
                "test#String":{"type":"string","traits":{"smithy.api#documentation":"KMS key ARN"}}
            }
        }"#).unwrap();
        let error = validate_smithy_type(&model, "test.Resource", "settings", "Settings", &member)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "unclassified KMS string test.Resource.settings{}[].name"
        );
    }
}
