//! Typed boundary for desired managed-resource values sent to AWS.

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, Value,
    enum_attr_at_schema_path,
};
use carina_core::schema::CompiledResourceSchema;
use indexmap::IndexMap;

pub(crate) type StringList = Vec<String>;
pub(crate) type Tags = IndexMap<String, String>;

/// Attribute access shared by live create inputs and lazy update inputs.
pub(crate) trait SendResource {
    fn id(&self) -> &ResourceId;
    fn get_attr(&self, name: &str) -> ProviderResult<Option<&Value>>;
}

impl<S, D> SendResource for Resource<S, NoPlaceholder, AbsoluteBindingAddress, D> {
    fn id(&self) -> &ResourceId {
        &self.id
    }

    fn get_attr(&self, name: &str) -> ProviderResult<Option<&Value>> {
        Ok(self.get_attr(name))
    }
}

impl SendResource for crate::helpers::UpdateResource {
    fn id(&self) -> &ResourceId {
        &self.id
    }

    fn get_attr(&self, name: &str) -> ProviderResult<Option<&Value>> {
        self.get_attr(name)
    }
}

pub(crate) fn optional_tags_value(tags: Option<Tags>) -> Option<Value> {
    tags.map(|tags| {
        Value::Concrete(ConcreteValue::Map(
            tags.into_iter()
                .map(|(key, value)| (key, Value::Concrete(ConcreteValue::String(value.into()))))
                .collect(),
        ))
    })
}

/// One generated send part that hand-written code must consume explicitly.
///
/// The guard intentionally runs only in tests/debug builds: the exhaustive
/// named destructure gives compile-time field-name coverage, while integration
/// tests exercise each create/update path and turn a discarded wrapper into a
/// precise panic. The residual gap is deliberate evasion with
/// `mem::forget`/`ManuallyDrop`, or a semantic mistake after `into_inner()`;
/// the request-capture contract test covers actual request propagation instead.
#[must_use = "generated send fields must be consumed with Sent::into_inner"]
pub(crate) struct Sent<T> {
    #[cfg(any(test, debug_assertions))]
    name: &'static str,
    value: Option<T>,
}

impl<T> Sent<T> {
    pub(crate) fn new(name: &'static str, value: T) -> Self {
        #[cfg(not(any(test, debug_assertions)))]
        let _ = name;
        Self {
            #[cfg(any(test, debug_assertions))]
            name,
            value: Some(value),
        }
    }

    pub(crate) fn into_inner(mut self) -> T {
        self.value
            .take()
            .expect("generated send part can only be consumed once")
    }
}

impl<T> Drop for Sent<T> {
    fn drop(&mut self) {
        #[cfg(any(test, debug_assertions))]
        if self.value.is_some() && !std::thread::panicking() {
            panic!(
                "generated send field `{}` was dropped without explicit consumption",
                self.name
            );
        }
    }
}

fn invalid_shape(resource: &impl SendResource, name: &str, expected: &str) -> ProviderError {
    invalid_shape_for_id(resource.id(), name, expected)
}

fn invalid_shape_for_id(id: &ResourceId, name: &str, expected: &str) -> ProviderError {
    ProviderError::invalid_input(format!("{name} must be {expected}")).for_resource(id.clone())
}

fn missing(resource: &impl SendResource, name: &str) -> ProviderError {
    ProviderError::invalid_input(format!("{name} is required")).for_resource(resource.id().clone())
}

pub(crate) fn optional_string(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<String>> {
    match resource.get_attr(name)? {
        Some(Value::Concrete(ConcreteValue::String(value))) => {
            Ok(Some(value.expose_plaintext().to_string()))
        }
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a string")),
    }
}

pub(crate) fn required_string(resource: &impl SendResource, name: &str) -> ProviderResult<String> {
    optional_string(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_string_list(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<StringList>> {
    optional_string_list_value(resource.id(), name, resource.get_attr(name)?)
}

pub(crate) fn optional_string_list_value(
    id: &ResourceId,
    name: &str,
    value: Option<&Value>,
) -> ProviderResult<Option<StringList>> {
    match value {
        Some(Value::Concrete(ConcreteValue::StringList(values))) => Ok(Some(
            values
                .iter()
                .map(|value| value.expose_plaintext().to_string())
                .collect(),
        )),
        Some(Value::Concrete(ConcreteValue::List(values))) => values
            .iter()
            .map(|value| match value {
                Value::Concrete(ConcreteValue::String(value)) => {
                    Ok(value.expose_plaintext().to_string())
                }
                _ => Err(invalid_shape_for_id(id, name, "a list of strings")),
            })
            .collect::<ProviderResult<Vec<_>>>()
            .map(Some),
        None => Ok(None),
        Some(_) => Err(invalid_shape_for_id(id, name, "a list of strings")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_string_list(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<StringList> {
    optional_string_list(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_tags(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<Tags>> {
    let Some(value) = resource.get_attr(name)? else {
        return Ok(None);
    };
    let Value::Concrete(ConcreteValue::Map(values)) = value else {
        return Err(invalid_shape(resource, name, "a map of strings"));
    };
    values
        .iter()
        .map(|(key, value)| match value {
            Value::Concrete(ConcreteValue::String(value)) => {
                Ok((key.clone(), value.expose_plaintext().to_string()))
            }
            _ => Err(invalid_shape(resource, name, "a map of strings")),
        })
        .collect::<ProviderResult<Tags>>()
        .map(Some)
}

#[allow(dead_code)]
pub(crate) fn required_tags(resource: &impl SendResource, name: &str) -> ProviderResult<Tags> {
    optional_tags(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_enum(
    resource: &impl SendResource,
    schema: &CompiledResourceSchema,
    name: &str,
) -> ProviderResult<Option<String>> {
    enum_attr_at_schema_path(schema, name, &[], resource.get_attr(name)?)
        .map(|value| value.map(|value| value.api_value().to_string()))
        .map_err(|error| crate::helpers::enum_attr_error(resource.id(), error))
}

pub(crate) fn required_enum(
    resource: &impl SendResource,
    schema: &CompiledResourceSchema,
    name: &str,
) -> ProviderResult<String> {
    optional_enum(resource, schema, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_bool(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<bool>> {
    match resource.get_attr(name)? {
        Some(Value::Concrete(ConcreteValue::Bool(value))) => Ok(Some(*value)),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a boolean")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_bool(resource: &impl SendResource, name: &str) -> ProviderResult<bool> {
    optional_bool(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_int(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<i64>> {
    match resource.get_attr(name)? {
        Some(Value::Concrete(ConcreteValue::Int(value))) => Ok(Some(*value)),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "an integer")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_int(resource: &impl SendResource, name: &str) -> ProviderResult<i64> {
    optional_int(resource, name)?.ok_or_else(|| missing(resource, name))
}

/// Narrow during decoding; no Sent guard has been created yet.
pub(crate) fn optional_i32(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<i32>> {
    optional_int(resource, name)?
        .map(|value| crate::helpers::sdk_i32(resource.id(), name, value))
        .transpose()
}

#[allow(dead_code)]
pub(crate) fn required_i32(resource: &impl SendResource, name: &str) -> ProviderResult<i32> {
    optional_i32(resource, name)?.ok_or_else(|| missing(resource, name))
}

/// Preflight nested wire integers before into_parts creates guards. Unknown
/// or differently shaped values remain the responsibility of schema/SDK packing.
pub(crate) fn check_i32_value(
    resource: &impl SendResource,
    name: &str,
    value: &Value,
    paths: &[&[&str]],
) -> ProviderResult<()> {
    fn check(id: &ResourceId, name: &str, value: &Value, path: &[&str]) -> ProviderResult<()> {
        let Some((head, tail)) = path.split_first() else {
            if let Value::Concrete(ConcreteValue::Int(value)) = value {
                crate::helpers::sdk_i32(id, name, *value)?;
            }
            return Ok(());
        };
        match (*head, value) {
            ("[]", Value::Concrete(ConcreteValue::List(items))) => {
                for item in items {
                    check(id, &format!("{name}[]"), item, tail)?;
                }
            }
            ("{}", Value::Concrete(ConcreteValue::Map(items))) => {
                for item in items.values() {
                    check(id, &format!("{name}{{}}"), item, tail)?;
                }
            }
            (field, Value::Concrete(ConcreteValue::Map(fields))) => {
                if let Some(value) = fields.get(field) {
                    check(id, &format!("{name}.{field}"), value, tail)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for path in paths {
        check(resource.id(), name, value, path)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn optional_float(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<f64>> {
    match resource.get_attr(name)? {
        Some(Value::Concrete(ConcreteValue::Float(value))) => Ok(Some(value.get())),
        None => Ok(None),
        Some(_) => Err(invalid_shape(resource, name, "a floating-point number")),
    }
}

#[allow(dead_code)]
pub(crate) fn required_float(resource: &impl SendResource, name: &str) -> ProviderResult<f64> {
    optional_float(resource, name)?.ok_or_else(|| missing(resource, name))
}

pub(crate) fn optional_value(
    resource: &impl SendResource,
    name: &str,
) -> ProviderResult<Option<Value>> {
    Ok(resource.get_attr(name)?.cloned())
}

pub(crate) fn required_value(resource: &impl SendResource, name: &str) -> ProviderResult<Value> {
    optional_value(resource, name)?.ok_or_else(|| missing(resource, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource_with(name: &str, value: Value) -> Resource {
        let mut resource = Resource::with_provider("aws", "test.Send", "fixture", None);
        resource.set_attr(name, value);
        resource
    }

    #[test]
    fn tags_reject_non_map_and_non_string_values() {
        let non_map = resource_with("tags", Value::Concrete(ConcreteValue::Bool(true)));
        assert!(optional_tags(&non_map, "tags").is_err());

        let non_string = resource_with(
            "tags",
            Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Count".to_string(),
                Value::Concrete(ConcreteValue::Int(1)),
            )]))),
        );
        assert!(optional_tags(&non_string, "tags").is_err());
    }

    #[test]
    fn string_lists_reject_non_list_and_non_string_elements() {
        let non_list = resource_with("names", Value::Concrete(ConcreteValue::Bool(true)));
        assert!(optional_string_list(&non_list, "names").is_err());

        let non_string = resource_with(
            "names",
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Int(1),
            )])),
        );
        assert!(optional_string_list(&non_string, "names").is_err());
    }

    #[test]
    fn sent_guard_allows_explicit_consumption() {
        assert_eq!(Sent::new("test.field", 42).into_inner(), 42);
    }

    #[test]
    #[should_panic(expected = "test.field")]
    fn sent_guard_panics_when_discarded() {
        drop(Sent::new("test.field", 42));
    }
}
