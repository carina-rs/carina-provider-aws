use crate::validate_service_arn;

/// Logs accepts key ARNs only, with the Smithy KmsKeyId length bound.
pub fn kms_key_arn_reference() -> carina_core::schema::AttributeType {
    carina_core::schema::AttributeType::reference(
        crate::AwsType::KmsKeyReference.identity(),
        Some(KMS_KEY_ARN_PATTERN.to_string()),
        Some((Some(0), Some(256))),
    )
}

pub const KMS_KEY_ARN_PATTERN: &str = r"^arn:(aws|aws-cn|aws-us-gov):kms:[a-z]{2}(-[a-z0-9]+)+-[0-9]+:[0-9]{12}:key/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|mrk-[0-9a-fA-F]{32})$";
pub const KMS_KEY_REFERENCE_PATTERN: &str = r"^((arn:(aws|aws-cn|aws-us-gov):kms:[a-z]{2}(-[a-z0-9]+)+-[0-9]+:[0-9]{12}:(key/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|mrk-[0-9a-fA-F]{32})|alias/[a-zA-Z0-9:/_-]+))|(alias/[a-zA-Z0-9:/_-]+)|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})|(mrk-[0-9a-fA-F]{32}))$";

/// Validate the canonical, key-only output type used by both guests.
pub fn validate_kms_key_arn(value: &str) -> Result<(), String> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    if PATTERN
        .get_or_init(|| regex::Regex::new(KMS_KEY_ARN_PATTERN).unwrap())
        .is_match(value)
    {
        Ok(())
    } else {
        Err("expected a KMS key ARN with a UUID or multi-region key ID".into())
    }
}

/// Offline syntax validation, using the reference leaf's portable constraints.
pub fn validate_kms_key_reference(value: &str) -> Result<(), String> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    if value.len() <= 2048
        && PATTERN
            .get_or_init(|| regex::Regex::new(KMS_KEY_REFERENCE_PATTERN).unwrap())
            .is_match(value)
    {
        Ok(())
    } else {
        Err("expected a KMS key ARN, alias ARN, alias name, UUID or multi-region key ID".into())
    }
}

// ========== KMS Key ID ==========

/// Check if a string is a valid UUID (8-4-4-4-12 hex digits).
pub fn is_uuid(s: &str) -> bool {
    let expected_lens = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip(expected_lens.iter())
            .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Validate a KMS Key ID (ARN, alias, or UUID format).
pub fn validate_kms_key_id(value: &str) -> Result<(), String> {
    // Accept KMS ARNs with key/ or alias/ resource prefix
    if value.starts_with("arn:") {
        validate_service_arn(value, "kms", None)?;
        let parts: Vec<&str> = value.splitn(6, ':').collect();
        let resource = parts[5];
        if !resource.starts_with("key/") && !resource.starts_with("alias/") {
            return Err(format!(
                "KMS ARN resource '{}' must start with 'key/' or 'alias/'",
                resource
            ));
        }
        return Ok(());
    }
    // Accept alias format: alias/<name>
    if value.starts_with("alias/") {
        if value.len() <= "alias/".len() {
            return Err("missing alias name after 'alias/'".to_string());
        }
        return Ok(());
    }
    // Accept bare key ID (UUID format: 8-4-4-4-12 hex digits)
    if is_uuid(value) {
        return Ok(());
    }
    Err(
        "expected a key ARN, alias ARN, alias name (alias/...), or key ID (UUID format)"
            .to_string(),
    )
}
