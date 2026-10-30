//! Typed boundary for desired managed-resource values sent to AWS.

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, Value};
use carina_core::schema::ResourceSchema;

fn invalid_shape(resource: &Resource<impl Sized>, name: &str, expected: &str) -> ProviderError {
    ProviderError::invalid_input(format!("{name} must be {expected}"))
        .for_resource(resource.id.clone())
}

fn missing(resource: &Resource<impl Sized>, name: &str) -> ProviderError {
    ProviderError::invalid_input(format!("{name} is required")).for_resource(resource.id.clone())
}

pub(crate) fn optional_string(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<Option<String>> {
    match resource.get_attr(name) {
        Some(Value::Concrete(ConcreteValue::String(value))) => {
            Ok(Some(value.expose_plaintext().to_string()))
        }
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a string")),
    }
}

pub(crate) fn required_string(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<String> {
    optional_string(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_enum(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
    name: &str,
) -> ProviderResult<Option<String>> {
    resource
        .get_enum_attr(schema, name)
        .map(|value| value.map(|value| value.api_value().to_string()))
        .map_err(|error| crate::helpers::enum_attr_error(&resource.id, error))
}

pub(crate) fn required_enum(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
    name: &str,
) -> ProviderResult<String> {
    optional_enum(resource, schema, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_bool(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<Option<bool>> {
    match resource.get_attr(name) {
        Some(Value::Concrete(ConcreteValue::Bool(value))) => Ok(Some(*value)),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a boolean")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_bool(resource: &Resource<impl Sized>, name: &str) -> ProviderResult<bool> {
    optional_bool(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_int(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<Option<i64>> {
    match resource.get_attr(name) {
        Some(Value::Concrete(ConcreteValue::Int(value))) => Ok(Some(*value)),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "an integer")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_int(resource: &Resource<impl Sized>, name: &str) -> ProviderResult<i64> {
    optional_int(resource, name)?.ok_or_else(|| missing(resource, name))
}

#[allow(dead_code)]
pub(crate) fn optional_float(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<Option<f64>> {
    match resource.get_attr(name) {
        Some(Value::Concrete(ConcreteValue::Float(value))) => Ok(Some(*value)),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a floating-point number")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_float(resource: &Resource<impl Sized>, name: &str) -> ProviderResult<f64> {
    optional_float(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_value(
    resource: &Resource<impl Sized>,
    name: &str,
) -> ProviderResult<Option<Value>> {
    Ok(resource.get_attr(name).cloned())
}

pub(crate) fn required_value(resource: &Resource<impl Sized>, name: &str) -> ProviderResult<Value> {
    optional_value(resource, name)?.ok_or_else(|| missing(resource, name))
}
