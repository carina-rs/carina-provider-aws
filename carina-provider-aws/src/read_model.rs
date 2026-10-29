//! Typed boundary for managed-resource reads.

use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ProviderError, ProviderResult, ReadOutcome};
use carina_core::resource::{ResourceId, Value};
use carina_core::schema::ResourceSchema;

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Implemented only by generated exhaustive read-model structs.
pub(crate) trait ManagedReadModel: sealed::Sealed {
    fn into_attributes(self) -> ProviderResult<HashMap<String, Value>>;
}

/// Consume one optional value from an existing SDK-extraction map while a
/// hand-written read is migrated into its exhaustive model literal.
pub(crate) fn take_optional(attributes: &mut HashMap<String, Value>, name: &str) -> Option<Value> {
    attributes.remove(name)
}

/// Consume a value that the canonical read model requires.
pub(crate) fn take_required(
    id: &ResourceId,
    attributes: &mut HashMap<String, Value>,
    name: &str,
) -> ProviderResult<Value> {
    attributes.remove(name).ok_or_else(|| {
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

/// Validate every generated `Value` field against the same schema the differ
/// consumes. In particular, this catches malformed hand-built nested values at
/// the one map-conversion seam.
pub(crate) fn validate_read_attributes(
    resource_type: &str,
    schema: ResourceSchema,
    attributes: &mut HashMap<String, Value>,
) -> ProviderResult<()> {
    let schema = schema.validate().map_err(|error| {
        ProviderError::internal(format!(
            "generated read schema for {resource_type} is invalid: {error}"
        ))
    })?;

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
}
