use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::AttributeType;

// ========== Tags ==========

/// Tags type for AWS resources (map of string values)
pub fn tags_type() -> AttributeType {
    AttributeType::map(AttributeType::string())
}

/// Validate that a tags map does not use Key/Value pair list structure.
///
/// Detects when a tags map contains both `key` and `value` as keys (case-insensitive),
/// which indicates the user wrote a Key/Value pair list instead of a flat map:
///   Wrong: `tags = { key = 'Name', value = '...' }`
///   Right: `tags = { Name = '...' }`
pub fn validate_tags_map(
    attributes: &std::collections::HashMap<String, Value>,
) -> Result<(), Vec<carina_core::schema::TypeError>> {
    if let Some(Value::Concrete(ConcreteValue::Map(map))) = attributes.get("tags") {
        let has_key = map.keys().any(|k| k.eq_ignore_ascii_case("key"));
        let has_value = map.keys().any(|k| k.eq_ignore_ascii_case("value"));
        if has_key && has_value {
            return Err(vec![carina_core::schema::TypeError::ResourceValidationFailed {
                message: "tags map contains both 'key' and 'value' as keys, which looks like a Key/Value pair list. Use flat map syntax instead: tags = { Name = '...' }".to_string(),
                attribute: Some("tags".to_string()),
            }]);
        }
    }
    Ok(())
}
