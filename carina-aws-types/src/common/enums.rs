use carina_core::schema::AttributeType;
use heck::ToSnakeCase;

// ========== Enum helpers ==========

/// Convert an AWS API enum value to its DSL (snake_case) spelling.
///
/// Shared by aws and awscc provider codegen; this defines what users type in
/// their `.crn` files. Per the naming-conventions design D7
/// (`carina-rs/carina/docs/specs/2026-04-22-naming-conventions-design.md`):
/// SHOUTY_SNAKE → lowercase; PascalCase → snake_case; kebab → snake;
/// numeric/dotted-numeric pass through unchanged.
pub fn dsl_enum_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return value.to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_uppercase())
    {
        return value.to_ascii_lowercase();
    }
    if !value.chars().any(|c| c.is_ascii_uppercase()) {
        // No uppercase letter: kebab, slash, colon, and dotted-numeric
        // separators collapse to snake_case so DSL identifiers stay parseable.
        // The strict validator (carina#2980) gates on the DSL spelling.
        return value.replace(['-', '.', ':', '/'], "_");
    }
    // Special case: acronym + lowercase + digits (e.g. "IPv4", "IPv6").
    // Heck's snake_case splits these as "i_pv4" which loses the acronym
    // structure. Treat them as a single all-lowercase word.
    if let Some(idx) = value.chars().position(|c| c.is_ascii_lowercase())
        && idx >= 1
        && value[..idx].chars().all(|c| c.is_ascii_uppercase())
        && value[idx..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return value.to_ascii_lowercase();
    }

    value.to_snake_case()
}

/// Build a `dsl_aliases` pair list for a `StringEnum`'s `values`.
///
/// Returns `(api, dsl)` pairs for **every** value, including identity
/// rows where the DSL spelling equals the API spelling. The exhaustive
/// table is what makes the carina-core strict-DSL validator (see
/// `carina-rs/carina#2980`) treat the whole enum uniformly — every
/// enum that has at least one rewrite triggers strict mode for the
/// whole variant set, and identity rows ensure values like
/// `("Subnet", "subnet")` / `("STANDARD_IA", "standard_ia")` accept
/// the DSL spelling cleanly without falling back to the legacy lax
/// canonical-fallback.
fn dsl_aliases_for(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|v| (v.to_string(), dsl_enum_value(v)))
        .collect()
}

/// Build an enum `AttributeType` whose DSL alias table is
/// derived from `values` via [`dsl_aliases_for`]. Hand-written
/// `carina-aws-types` enum sites must call this constructor instead of
/// the raw [`AttributeType::enum_`] so that aliases cannot be
/// silently omitted — every call site sources the alias table from the
/// same `values` slice that defines the canonical AWS API values, making
/// "closed enum with empty `dsl_aliases`" impossible by construction.
///
/// Core's enum resolver relies on this alias table to rewrite DSL
/// spellings (e.g. `aes256`) to the AWS canonical (`AES256`) before the
/// value crosses the provider boundary. A closed enum whose alias table is
/// empty can silently forward the raw alias spelling to the AWS SDK and
/// trigger `MalformedXML` / `Unknown(...)` errors.
/// See `carina-rs/carina-provider-aws#390`.
///
/// The raw [`AttributeType::enum_`] constructor remains in use
/// only for the `ConditionOperator` site, which derives its alias table
/// dynamically from a different source.
pub(crate) fn enum_with_dsl_aliases(
    values: &[&str],
    identity: carina_core::schema::TypeIdentity,
) -> AttributeType {
    let owned_values: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    let aliases = dsl_aliases_for(values);
    AttributeType::enum_(identity, Some(owned_values), aliases, None, None)
}

/// Check if `input` matches any of `valid_values` using enum matching rules:
/// exact match, case-insensitive, or underscore-to-hyphen (case-insensitive).
/// Returns the matched valid value if found.
pub fn find_matching_enum_value<'a>(input: &str, valid_values: &[&'a str]) -> Option<&'a str> {
    // Exact match
    if let Some(&v) = valid_values.iter().find(|&&v| v == input) {
        return Some(v);
    }
    // Case-insensitive match
    if let Some(&v) = valid_values
        .iter()
        .find(|&&v| v.eq_ignore_ascii_case(input))
    {
        return Some(v);
    }
    // Underscore-to-hyphen match (case-insensitive)
    let hyphenated = input.replace('_', "-");
    if let Some(&v) = valid_values
        .iter()
        .find(|&&v| v.eq_ignore_ascii_case(&hyphenated))
    {
        return Some(v);
    }
    None
}

/// Canonicalize an enum value by matching against valid values.
/// Handles exact match, case-insensitive match, and underscore-to-hyphen conversion.
pub fn canonicalize_enum_value(raw: &str, valid_values: &[&str]) -> String {
    find_matching_enum_value(raw, valid_values)
        .unwrap_or(raw)
        .to_string()
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
        assert_eq!(dsl_enum_value("1.0"), "1.0");
    }

    #[test]
    fn mixed_dotted_values_become_snake() {
        assert_eq!(dsl_enum_value("ipsec.1"), "ipsec_1");
    }

    #[test]
    fn slash_values_become_snake() {
        assert_eq!(dsl_enum_value("text/plain"), "text_plain");
        assert_eq!(dsl_enum_value("application/json"), "application_json");
        assert_eq!(
            dsl_enum_value("application/javascript"),
            "application_javascript"
        );
    }

    #[test]
    fn colon_values_become_snake() {
        assert_eq!(dsl_enum_value("aws:kms"), "aws_kms");
    }

    #[test]
    fn acronym_lowercase_digits_lowercase_as_one_word() {
        assert_eq!(dsl_enum_value("IPv4"), "ipv4");
        assert_eq!(dsl_enum_value("IPv6"), "ipv6");
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
    fn already_snake_or_kebab_normalizes_separators() {
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
        assert_eq!(dsl_enum_value("A"), "a");
        assert_eq!(dsl_enum_value("AAAA"), "aaaa");
        assert_eq!(dsl_enum_value("CNAME"), "cname");
        assert_eq!(dsl_enum_value("MX"), "mx");
        assert_eq!(dsl_enum_value("TXT"), "txt");
    }
}
