// ========== AWS Account ID ==========

/// Validate a 12-digit AWS Account ID.
pub fn validate_aws_account_id(id: &str) -> Result<(), String> {
    if id.len() != 12 {
        return Err(format!(
            "must be exactly 12 digits, got {} characters",
            id.len()
        ));
    }
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("must contain only digits".to_string());
    }
    Ok(())
}
