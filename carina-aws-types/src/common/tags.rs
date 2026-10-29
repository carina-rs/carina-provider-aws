use carina_core::schema::AttributeType;

// ========== Tags ==========

/// Tags type for AWS resources (map of string values)
pub fn tags_type() -> AttributeType {
    AttributeType::map(AttributeType::string())
}
