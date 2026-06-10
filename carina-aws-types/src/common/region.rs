use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, CompletionValue, DslTransform, legacy_validator};
use carina_core::utils::validate_enum_namespace;

use super::provider_bare_type;

// ========== Region constants ==========

/// AWS regions with display names. Single source of truth for validation and completions.
pub const REGIONS: &[(&str, &str)] = &[
    // Africa
    ("af-south-1", "Africa (Cape Town)"),
    // Asia Pacific
    ("ap-east-1", "Asia Pacific (Hong Kong)"),
    ("ap-east-2", "Asia Pacific (Malaysia)"),
    ("ap-northeast-1", "Asia Pacific (Tokyo)"),
    ("ap-northeast-2", "Asia Pacific (Seoul)"),
    ("ap-northeast-3", "Asia Pacific (Osaka)"),
    ("ap-south-1", "Asia Pacific (Mumbai)"),
    ("ap-south-2", "Asia Pacific (Hyderabad)"),
    ("ap-southeast-1", "Asia Pacific (Singapore)"),
    ("ap-southeast-2", "Asia Pacific (Sydney)"),
    ("ap-southeast-3", "Asia Pacific (Jakarta)"),
    ("ap-southeast-4", "Asia Pacific (Melbourne)"),
    ("ap-southeast-5", "Asia Pacific (Auckland)"),
    ("ap-southeast-6", "Asia Pacific (Thailand)"),
    ("ap-southeast-7", "Asia Pacific (Taiwan)"),
    // Canada
    ("ca-central-1", "Canada (Central)"),
    ("ca-west-1", "Canada West (Calgary)"),
    // China
    ("cn-north-1", "China (Beijing)"),
    ("cn-northwest-1", "China (Ningxia)"),
    // Europe
    ("eu-central-1", "Europe (Frankfurt)"),
    ("eu-central-2", "Europe (Zurich)"),
    ("eu-north-1", "Europe (Stockholm)"),
    ("eu-south-1", "Europe (Milan)"),
    ("eu-south-2", "Europe (Spain)"),
    ("eu-west-1", "Europe (Ireland)"),
    ("eu-west-2", "Europe (London)"),
    ("eu-west-3", "Europe (Paris)"),
    // Israel
    ("il-central-1", "Israel (Tel Aviv)"),
    // Middle East
    ("me-central-1", "Middle East (UAE)"),
    ("me-south-1", "Middle East (Bahrain)"),
    // Mexico
    ("mx-central-1", "Mexico (Central)"),
    // South America
    ("sa-east-1", "South America (Sao Paulo)"),
    // US
    ("us-east-1", "US East (N. Virginia)"),
    ("us-east-2", "US East (Ohio)"),
    ("us-gov-east-1", "AWS GovCloud (US-East)"),
    ("us-gov-west-1", "AWS GovCloud (US-West)"),
    ("us-west-1", "US West (N. California)"),
    ("us-west-2", "US West (Oregon)"),
];

/// Check if a region code is valid.
pub fn is_valid_region(region: &str) -> bool {
    REGIONS.iter().any(|(code, _)| *code == region)
}

/// Format valid region codes as a comma-separated string for error messages.
pub fn valid_regions_display() -> String {
    REGIONS
        .iter()
        .map(|(code, _)| *code)
        .collect::<Vec<_>>()
        .join(", ")
}

fn strip_region_prefix(value: &str) -> &str {
    value
        .strip_prefix("aws.Region.")
        .or_else(|| value.strip_prefix("Region."))
        .unwrap_or(value)
}

/// Region API spelling -> DSL spelling pairs for the carina-core /
/// carina-provider-protocol `StringEnum.dsl_aliases` field.
///
/// AWS region codes carry hyphens (`ap-northeast-1`) but the DSL
/// identifier form replaces them with underscores (`ap_northeast_1`).
/// Provider crates emit this list verbatim so the alias table
/// survives the WASM-component boundary as data — a `fn` pointer
/// would not (carina#2831).
pub fn region_dsl_aliases() -> Vec<(String, String)> {
    REGIONS
        .iter()
        .filter_map(|(code, _)| {
            let api = (*code).to_string();
            let dsl = api.replace('-', "_");
            (api != dsl).then_some((api, dsl))
        })
        .collect()
}

/// Generate region completion values for a given provider prefix (e.g., "aws" or "awscc").
///
/// Converts AWS region format (`ap-northeast-1`) to DSL format (`ap_northeast_1`)
/// and prefixes with `{prefix}.Region.`.
pub fn region_completions(prefix: &str) -> Vec<CompletionValue> {
    REGIONS
        .iter()
        .map(|(code, name)| {
            let dsl_code = code.replace('-', "_");
            CompletionValue::new(format!("{}.Region.{}", prefix, dsl_code), *name)
        })
        .collect()
}

/// AWS region type with custom validation
/// Accepts:
/// - DSL format: aws.Region.ap_northeast_1
/// - AWS string format: "ap-northeast-1"
/// - Shorthand: ap_northeast_1
pub fn aws_region() -> AttributeType {
    AttributeType::enum_(
        provider_bare_type(&[], "Region"),
        None,
        vec![],
        Some(legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                let id = provider_bare_type(&[], "Region");
                validate_enum_namespace(s, &id)
                    .map_err(|reason| format!("Invalid region '{}': {}", s, reason))?;
                // Normalize the input to AWS format (hyphens)
                let normalized = strip_region_prefix(s).replace('_', "-");
                if is_valid_region(&normalized) {
                    Ok(())
                } else {
                    Err(format!(
                        "Invalid region '{}', expected one of: {} or DSL format like aws.Region.ap_northeast_1",
                        s,
                        valid_regions_display()
                    ))
                }
            } else {
                Err("Expected string".to_string())
            }
        })),
        Some(DslTransform::HyphenToUnderscore),
    )
}
