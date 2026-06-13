use carina_core::schema::AttributeType;

use super::provider_bare_type;

// ========== HTTP Response Status Code ==========

/// Validate a 3-digit HTTP response status code restricted to 2XX/4XX/5XX.
pub fn validate_http_response_status_code(code: &str) -> Result<(), String> {
    if code.len() != 3 {
        return Err(format!(
            "must be exactly 3 digits, got {} characters",
            code.len()
        ));
    }
    if !code.chars().all(|c| c.is_ascii_digit()) {
        return Err("must contain only digits".to_string());
    }
    let first = code.chars().next().expect("length checked above");
    if !matches!(first, '2' | '4' | '5') {
        return Err(format!("must start with 2, 4, or 5 (got '{first}')"));
    }
    Ok(())
}

/// HTTP response status code (3-digit string, 2XX/4XX/5XX only)
pub fn http_response_status_code() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_bare_type(&[], "HttpResponseStatusCode")),
        Some(r"^(2|4|5)\d{2}$".to_string()),
        Some((Some(3), Some(3))),
        None,
    )
}
