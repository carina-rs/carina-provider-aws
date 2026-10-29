//! Typed boundary for managed-resource reads.

use std::collections::HashMap;
use std::sync::OnceLock;

use carina_core::provider::{AppliedIdentifier, ProviderError, ProviderResult, ReadOutcome};
use carina_core::resource::{ConcreteValue, ResourceId, Value};
use carina_core::schema::{ResourceSchema, ValidatedResourceSchema};

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Implemented only by generated exhaustive read-model structs.
pub(crate) trait ManagedReadModel: sealed::Sealed {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>>;
}

/// Require a typed SDK-extracted value that the canonical model cannot omit.
pub(crate) fn require_read_value(
    id: &ResourceId,
    value: Option<Value>,
    name: &str,
) -> ProviderResult<Value> {
    value.ok_or_else(|| {
        ProviderError::internal(format!(
            "AWS read omitted required canonical attribute {name}"
        ))
        .for_resource(id.clone())
    })
}

pub(crate) fn empty_map_value() -> Value {
    Value::Concrete(carina_core::resource::ConcreteValue::Map(
        indexmap::IndexMap::new(),
    ))
}

/// Materialize a resource attribute that is canonically observable through
/// the provider's applied identifier even when the AWS response body omits it.
pub(crate) fn applied_identifier_value(identifier: &AppliedIdentifier) -> Value {
    Value::Concrete(ConcreteValue::String(
        identifier.as_str().to_string().into(),
    ))
}

/// The sole managed-read boundary that turns an exhaustive typed model into
/// core's attribute map.
pub(crate) fn found_managed_read(
    id: &ResourceId,
    identifier: &AppliedIdentifier,
    model: impl ManagedReadModel,
) -> ProviderResult<ReadOutcome> {
    let attributes = model
        .into_attributes()
        .map_err(|error| error.for_resource(id.clone()))?;
    Ok(crate::helpers::found_read(id, identifier, attributes))
}

/// Return the one validated schema instance used by a generated read model.
pub(crate) fn cached_validated_read_schema(
    resource_type: &str,
    cache: &'static OnceLock<Result<ValidatedResourceSchema, String>>,
    build: impl FnOnce() -> ResourceSchema,
) -> ProviderResult<&'static ValidatedResourceSchema> {
    cache
        .get_or_init(|| build().validate().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| {
            ProviderError::internal(format!(
                "generated read schema for {resource_type} is invalid: {error}"
            ))
        })
}

/// Validate every generated `Value` field against the same cached schema the
/// differ consumes. In particular, this catches malformed hand-built nested
/// values at the one map-conversion seam.
pub(crate) fn validate_read_attributes(
    resource_type: &str,
    schema: &ValidatedResourceSchema,
    attributes: &mut HashMap<String, Value>,
) -> ProviderResult<()> {
    for (name, value) in attributes {
        let attribute = schema.effective_attribute(name).ok_or_else(|| {
            ProviderError::internal(format!(
                "generated {resource_type} read model emitted unknown attribute {name}"
            ))
        })?;
        *value = attribute.canonicalize_state(value.clone());
        attribute.validate(value).map_err(|error| {
            ProviderError::internal(format!(
                "generated {resource_type} read model emitted invalid {name}: {error}"
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn rust_files_below(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("read provider source directory") {
            let path = entry.expect("read provider source entry").path();
            if path.is_dir() {
                rust_files_below(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    #[test]
    fn every_managed_read_uses_the_typed_boundary() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files_below(&src.join("services"), &mut files);
        files.push(src.join("ec2_security_group_rules.rs"));

        for path in files {
            let source = fs::read_to_string(&path).expect("read managed-resource source");
            assert!(
                !source.contains("crate::helpers::found_read(")
                    && !source.contains("ReadOutcome::found("),
                "{} bypasses the generated managed-read model",
                path.display()
            );
            if source.contains("pub(crate) async fn read_") {
                assert!(
                    source.contains("found_managed_read(")
                        || source.contains("read_ec2_security_group_rule("),
                    "{} declares a managed read without the typed boundary",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn managed_reads_do_not_use_string_keyed_take_helpers() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files_below(&src.join("services"), &mut files);
        files.push(src.join("ec2_security_group_rules.rs"));

        for path in files {
            let source = fs::read_to_string(&path).expect("read managed-resource source");
            assert!(
                !source.contains("take_optional(") && !source.contains("take_required("),
                "{} still moves extracted read state through string keys",
                path.display()
            );
        }
    }

    #[test]
    fn generated_extractors_do_not_accept_attribute_maps() {
        let generated = include_str!("provider_generated.rs");
        assert!(
            !generated.contains("attributes: &mut") && !generated.contains("attributes.insert("),
            "generated SDK extractors must return typed fields directly"
        );
    }

    #[test]
    fn every_generated_read_fields_type_is_exhaustively_consumed() {
        let generated = include_str!("provider_generated.rs");
        let type_names = generated.lines().filter_map(|line| {
            line.strip_prefix("pub(crate) struct ")
                .and_then(|line| line.strip_suffix("ReadFields {"))
                .map(|prefix| format!("{prefix}ReadFields"))
        });

        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files_below(&src.join("services"), &mut files);
        files.push(src.join("ec2_security_group_rules.rs"));
        let managed_reads = files
            .iter()
            .map(|path| fs::read_to_string(path).expect("read managed-resource source"))
            .collect::<Vec<_>>()
            .join("\n");

        for type_name in type_names {
            assert!(
                managed_reads.contains(&type_name),
                "{type_name} is generated but no managed read consumes it"
            );
        }
        for binding in managed_reads.split("let ").skip(1) {
            let Some(start) = binding.find("ReadFields {") else {
                continue;
            };
            if binding[..start].contains('\n') {
                continue;
            }
            let pattern = binding[start + "ReadFields {".len()..]
                .split_once('}')
                .map(|(pattern, _)| pattern)
                .expect("read-fields destructuring pattern is closed");
            assert!(
                !pattern.contains(".."),
                "managed reads must exhaustively destructure generated read fields"
            );
        }
    }
}
