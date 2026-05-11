//! DSL-side spelling helpers for AWS provider codegen.
//!
//! Per the naming-conventions design (D7) in
//! `carina-rs/carina/docs/specs/2026-04-22-naming-conventions-design.md`,
//! every `StringEnum` value emitted by codegen must have a snake_case DSL
//! spelling so the validator and generated docs agree with the project-wide
//! convention.
//!
//! Sibling implementation: the awscc provider's
//! `carina-provider-awscc/src/bin/codegen.rs::dsl_enum_value`. The two must
//! stay behaviorally identical so the providers don't drift on what users
//! type in their `.crn` files.
//!
//! Rules from D7:
//! - Empty input passes through unchanged.
//! - Numeric / dotted-numeric values (`"1"`, `"ipsec.1"`) pass through.
//! - SHOUTY_SNAKE (`GROUP`, `AWS_ACCOUNT`, `AES256`) → lowercase.
//! - Already lowercase / kebab (no uppercase letters) → `-` to `_`.
//! - PascalCase / mixed (`Enabled`, `BucketOwnerEnforced`) → `to_snake_case`.

use heck::ToSnakeCase;

/// Convert an AWS API enum value to its DSL (snake_case) spelling.
///
/// See module-level docs for the full rules.
pub fn dsl_enum_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    // Passthrough for already-numeric or dotted values (e.g. "ipsec.1", "1").
    if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return value.to_string();
    }

    // SHOUTY_SNAKE: uppercase ASCII letters (+ underscores / digits), with at
    // least one uppercase letter to distinguish from pure-digit strings.
    if value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_uppercase())
    {
        return value.to_ascii_lowercase();
    }

    // Already snake / kebab (no uppercase): normalize hyphens and
    // dotted-numeric splits (e.g. `cloud-watch-logs` → `cloud_watch_logs`,
    // `ipsec.1` → `ipsec_1`). Dotted-numeric values would otherwise need
    // to be quoted (`'ipsec.1'`) in DSL position, and the strict-DSL
    // validator (carina#2980) gates acceptance on the DSL spelling.
    if !value.chars().any(|c| c.is_ascii_uppercase()) {
        return value.replace(['-', '.'], "_");
    }

    // PascalCase or anything else mixed: route through heck's ToSnakeCase.
    value.to_snake_case()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_passes_through() {
        assert_eq!(dsl_enum_value(""), "");
    }

    #[test]
    fn pure_digits_pass_through() {
        assert_eq!(dsl_enum_value("1"), "1");
        assert_eq!(dsl_enum_value("123"), "123");
    }

    #[test]
    fn pure_dotted_numeric_passes_through() {
        // All-digit / all-dot strings stay as-is so they can be quoted in
        // DSL position (`'1.0'`). Only mixed dotted values that contain
        // letters get the `.` → `_` rewrite below.
        assert_eq!(dsl_enum_value("1.0"), "1.0");
    }

    #[test]
    fn mixed_dotted_values_become_snake() {
        // IPSec / IKE-style values that AWS APIs occasionally surface.
        // The `.` would force a quoted-literal in DSL position; rewriting
        // to `_` keeps the DSL spelling a bare identifier and the strict
        // validator (carina#2980) can gate on it cleanly.
        assert_eq!(dsl_enum_value("ipsec.1"), "ipsec_1");
    }

    #[test]
    fn shouty_snake_lowercases() {
        assert_eq!(dsl_enum_value("GROUP"), "group");
        assert_eq!(dsl_enum_value("AWS_ACCOUNT"), "aws_account");
        assert_eq!(dsl_enum_value("AES256"), "aes256");
        assert_eq!(dsl_enum_value("STANDARD_IA"), "standard_ia");
        assert_eq!(dsl_enum_value("ALL"), "all");
        assert_eq!(dsl_enum_value("VPC"), "vpc");
    }

    #[test]
    fn already_snake_or_kebab_normalizes_hyphens() {
        // ap-northeast-1 has digits, hyphens, lowercase only.
        assert_eq!(dsl_enum_value("ap-northeast-1"), "ap_northeast_1");
        assert_eq!(dsl_enum_value("already_snake"), "already_snake");
        assert_eq!(dsl_enum_value("with-dashes"), "with_dashes");
    }

    #[test]
    fn pascal_case_to_snake_case() {
        assert_eq!(dsl_enum_value("Enabled"), "enabled");
        assert_eq!(dsl_enum_value("Suspended"), "suspended");
        assert_eq!(dsl_enum_value("VersioningStatus"), "versioning_status");
        assert_eq!(
            dsl_enum_value("BucketOwnerEnforced"),
            "bucket_owner_enforced"
        );
        assert_eq!(dsl_enum_value("ObjectWriter"), "object_writer");
        assert_eq!(dsl_enum_value("Gateway"), "gateway");
    }

    #[test]
    fn route53_record_types_lowercase() {
        // Pure SHOUTY abbreviations from RecordSet.Type.
        assert_eq!(dsl_enum_value("A"), "a");
        assert_eq!(dsl_enum_value("AAAA"), "aaaa");
        assert_eq!(dsl_enum_value("CNAME"), "cname");
        assert_eq!(dsl_enum_value("MX"), "mx");
        assert_eq!(dsl_enum_value("TXT"), "txt");
    }
}
