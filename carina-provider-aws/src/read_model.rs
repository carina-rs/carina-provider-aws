//! Typed boundary for managed-resource reads.

use std::collections::HashMap;
use std::sync::OnceLock;

use carina_core::provider::{
    AppliedIdentifier, AppliedState, ProviderError, ProviderResult, ReadOutcome,
};
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
    Ok(ReadOutcome::found(AppliedState::from_identifier(
        id.clone(),
        identifier.clone(),
        attributes,
    )))
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

/// Canonicalize every generated `Value` field against the same cached schema
/// the differ consumes, and reject attributes that the generated model should
/// not have been able to emit.
///
/// Values returned by AWS are deliberately not validated here. AWS can add a
/// new enum variant before the pinned Smithy model learns about it, and a read
/// must preserve that value rather than making refresh impossible.
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use carina_core::resource::{ConcreteValue, Value};

    use super::validate_read_attributes;

    #[test]
    fn aws_enum_value_newer_than_the_schema_is_preserved() {
        let schema = crate::schemas::generated::acm::certificate::acm_certificate_config()
            .schema
            .validate()
            .expect("generated ACM schema");
        let future_value = Value::Concrete(ConcreteValue::String(
            "POST_QUANTUM_2048".to_string().into(),
        ));
        let mut attributes = HashMap::from([("key_algorithm".to_string(), future_value.clone())]);

        validate_read_attributes("acm.Certificate", &schema, &mut attributes)
            .expect("AWS-returned values must not be rejected by the pinned schema");

        assert_eq!(attributes.get("key_algorithm"), Some(&future_value));
    }
}
