use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use super::provider_bare_type;

// ========== ARN validators ==========

/// Valid AWS partition values.
const VALID_PARTITIONS: &[&str] = &["aws", "aws-cn", "aws-us-gov"];

/// Validate basic ARN format (starts with "arn:", has 6+ colon-separated parts).
/// Enforces valid partition and non-empty service.
pub fn validate_arn(arn: &str) -> Result<(), String> {
    if !arn.starts_with("arn:") {
        return Err("must start with 'arn:'".to_string());
    }
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts.len() < 6 {
        return Err(
            "must have at least 6 colon-separated parts (arn:partition:service:region:account:resource)".to_string()
        );
    }
    if !VALID_PARTITIONS.contains(&parts[1]) {
        return Err(format!(
            "invalid partition '{}', must be one of: {}",
            parts[1],
            VALID_PARTITIONS.join(", ")
        ));
    }
    // Service must be non-empty (e.g., "s3", "iam", "ec2")
    if parts[2].is_empty() {
        return Err("service must not be empty (e.g., 's3', 'iam', 'ec2')".to_string());
    }
    Ok(())
}

/// Validate an ARN for a specific AWS service and optional resource prefix.
pub fn validate_service_arn(
    arn: &str,
    expected_service: &str,
    resource_prefix: Option<&str>,
) -> Result<(), String> {
    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != expected_service {
        return Err(format!(
            "expected {} service, got '{}'",
            expected_service, parts[2]
        ));
    }
    if let Some(prefix) = resource_prefix
        && !parts[5].starts_with(prefix)
    {
        return Err(format!(
            "expected resource starting with '{}', got '{}'",
            prefix, parts[5]
        ));
    }
    Ok(())
}

/// Validate an IAM ARN with strict checks on region, account, and resource name.
///
/// IAM ARNs have the form `arn:{partition}:iam::{account}:{resource_prefix}{name}`.
/// - Region (parts[3]) must be empty
/// - Account (parts[4]) must be `aws` (managed policy) or a 12-digit account ID
/// - Resource name after `resource_prefix` must be non-empty and contain only
///   valid IAM path/name characters
pub fn validate_iam_arn(arn: &str, resource_prefix: &str) -> Result<(), String> {
    // Derive type label from prefix: "policy/" -> "IAM Policy ARN", "role/" -> "IAM Role ARN"
    let resource_type = resource_prefix.trim_end_matches('/');
    let label = format!(
        "IAM {} ARN",
        resource_type
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string() + &resource_type[1..])
            .unwrap_or_default()
    );

    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != "iam" {
        return Err(format!(
            "expected {label} but service is '{}' in '{arn}'",
            parts[2]
        ));
    }
    if !parts[3].is_empty() {
        return Err(format!(
            "{label} region must be empty, got '{}' in '{arn}'",
            parts[3]
        ));
    }
    let account = parts[4];
    if account != "aws" && (account.len() != 12 || !account.chars().all(|c| c.is_ascii_digit())) {
        return Err(format!(
            "{label} account must be 'aws' or a 12-digit ID, got '{account}' in '{arn}'"
        ));
    }
    if !parts[5].starts_with(resource_prefix) {
        return Err(format!(
            "{label} resource must begin with '{resource_prefix}', but got '{}' in '{arn}'",
            parts[5]
        ));
    }
    let resource_name = &parts[5][resource_prefix.len()..];
    if resource_name.is_empty() {
        return Err(format!(
            "{label} name after '{resource_prefix}' must not be empty in '{arn}'"
        ));
    }
    // IAM names/paths allow: alphanumeric, plus +, =, ,, ., @, -, _, /
    if !resource_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+=,.@-_/".contains(c))
    {
        return Err(format!(
            "{label} name contains invalid characters: '{resource_name}' in '{arn}'"
        ));
    }
    Ok(())
}

/// ARN type (e.g., "arn:aws:s3:::my-bucket")
pub fn arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_bare_type(&[], "Arn")),
        AttributeType::string(),
        Some("^arn:(aws|aws-cn|aws-us-gov):[^:]+:.*$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_arn(s).map_err(|reason| format!("Invalid ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
