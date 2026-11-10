use std::sync::LazyLock;

use crate::{arn_partition_pattern, arn_regex, validate_service_arn};

/// Canonical KMS key ARN grammar over all supported partitions.
pub fn kms_key_arn_pattern() -> &'static str {
    static PATTERN: LazyLock<String> = LazyLock::new(|| {
        arn_regex(
            r"kms:[a-z]{2,4}(-[a-z0-9]+)+-[0-9]+:[0-9]{12}:key/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|mrk-[0-9a-fA-F]{32})$",
        )
    });
    &PATTERN
}

/// KMS reference grammar, retaining aliases and bare key IDs alongside ARNs.
pub fn kms_key_reference_pattern() -> &'static str {
    static PATTERN: LazyLock<String> = LazyLock::new(|| {
        [
            "^((arn:",
            arn_partition_pattern(),
            r":kms:[a-z]{2,4}(-[a-z0-9]+)+-[0-9]+:[0-9]{12}:(key/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|mrk-[0-9a-fA-F]{32})|alias/[a-zA-Z0-9:/_-]+))|(alias/[a-zA-Z0-9:/_-]+)|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})|(mrk-[0-9a-fA-F]{32}))$",
        ].concat()
    });
    &PATTERN
}

/// Validate the canonical, key-only output type used by both guests.
pub fn validate_kms_key_arn(value: &str) -> Result<(), String> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    if PATTERN
        .get_or_init(|| regex::Regex::new(kms_key_arn_pattern()).unwrap())
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
            .get_or_init(|| regex::Regex::new(kms_key_reference_pattern()).unwrap())
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

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_IDS: [&str; 4] = [
        "1234abcd-12ab-34cd-56ef-1234567890ab",
        "1234ABCD-12AB-34CD-56EF-1234567890AB",
        "mrk-0123456789abcdef0123456789abcdef",
        "mrk-0123456789ABCDEF0123456789ABCDEF",
    ];

    #[test]
    fn patterns_accept_current_partitions_and_regions() {
        let key_pattern = regex::Regex::new(kms_key_arn_pattern()).unwrap();
        let reference_pattern = regex::Regex::new(kms_key_reference_pattern()).unwrap();
        for (partition, region) in [
            ("aws", "us-east-1"),
            ("aws", "ap-northeast-1"),
            ("aws-cn", "cn-north-1"),
            ("aws-us-gov", "us-gov-west-1"),
            ("aws-iso", "us-iso-east-1"),
            ("aws-iso-b", "us-isob-east-1"),
            ("aws-iso-e", "eu-isoe-west-1"),
            ("aws-iso-f", "us-isof-south-1"),
            ("aws-eusc", "eusc-de-east-1"),
            ("aws", "il-central-1"),
            ("aws", "ap-southeast-7"),
            ("aws", "mx-central-1"),
        ] {
            for key_id in KEY_IDS {
                let arn = format!("arn:{partition}:kms:{region}:123456789012:key/{key_id}");
                assert!(key_pattern.is_match(&arn), "{arn}");
                assert!(reference_pattern.is_match(&arn), "{arn}");
                assert!(validate_kms_key_arn(&arn).is_ok(), "{arn}");
                assert!(validate_kms_key_reference(&arn).is_ok(), "{arn}");
            }
            let alias = format!("arn:{partition}:kms:{region}:123456789012:alias/customer");
            assert!(!key_pattern.is_match(&alias), "{alias}");
            assert!(reference_pattern.is_match(&alias), "{alias}");
            assert!(validate_kms_key_reference(&alias).is_ok(), "{alias}");
        }
    }

    #[test]
    fn reference_pattern_accepts_uuid_and_mrk_ids_in_both_cases() {
        let pattern = regex::Regex::new(kms_key_reference_pattern()).unwrap();
        for key_id in KEY_IDS {
            assert!(pattern.is_match(key_id), "{key_id}");
            assert!(validate_kms_key_reference(key_id).is_ok(), "{key_id}");
            assert!(validate_kms_key_arn(key_id).is_err(), "{key_id}");
        }
    }

    #[test]
    fn patterns_reject_malformed_arns() {
        let key_pattern = regex::Regex::new(kms_key_arn_pattern()).unwrap();
        let reference_pattern = regex::Regex::new(kms_key_reference_pattern()).unwrap();
        for (partition, region, account, resource) in [
            (
                "aws-xx",
                "us-east-1",
                "123456789012",
                "key/1234abcd-12ab-34cd-56ef-1234567890ab",
            ),
            (
                "AWS",
                "us-east-1",
                "123456789012",
                "key/1234abcd-12ab-34cd-56ef-1234567890ab",
            ),
            (
                "aws",
                "us-east-1",
                "12345678901",
                "key/1234abcd-12ab-34cd-56ef-1234567890ab",
            ),
            ("aws", "us-east-1", "123456789012", "key/not-a-uuid"),
            (
                "aws",
                "us-east-1",
                "123456789012",
                "key/mrk-0123456789abcdef0123456789abcde",
            ),
            (
                "aws",
                "us-east-1",
                "123456789012",
                "key/mrk-0123456789abcdef0123456789abcdeg",
            ),
            ("aws-xx", "us-east-1", "123456789012", "alias/customer"),
            ("AWS", "us-east-1", "123456789012", "alias/customer"),
            ("aws", "us-east-1", "12345678901", "alias/customer"),
            ("aws", "u-east-1", "123456789012", "alias/customer"),
            ("aws", "euscc-de-east-1", "123456789012", "alias/customer"),
        ] {
            let arn = format!("arn:{partition}:kms:{region}:{account}:{resource}");
            assert!(!key_pattern.is_match(&arn), "{arn}");
            assert!(!reference_pattern.is_match(&arn), "{arn}");
            assert!(validate_kms_key_arn(&arn).is_err(), "{arn}");
            assert!(validate_kms_key_reference(&arn).is_err(), "{arn}");
        }
    }
}
