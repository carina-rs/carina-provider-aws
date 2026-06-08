use carina_core::schema::AttributeType;

// ========== Enum helpers ==========

/// Convert an AWS API enum value to its DSL (snake_case) spelling.
///
/// Behaviorally identical to `carina-codegen-aws::dsl::dsl_enum_value` and
/// `carina-provider-awscc::bin::codegen::dsl_enum_value`. The three
/// implementations must stay in sync — they collectively define what users
/// type in their `.crn` files. Per the naming-conventions design D7
/// (`carina-rs/carina/docs/specs/2026-04-22-naming-conventions-design.md`):
/// SHOUTY_SNAKE → lowercase; PascalCase → snake_case; kebab → snake;
/// numeric/dotted-numeric pass through unchanged.
pub(crate) fn dsl_enum_value(value: &str) -> String {
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
        // No uppercase letter: kebab and dotted-numeric (e.g.
        // `cloud-watch-logs`, `ipsec.1`) collapse to snake_case so DSL
        // identifiers stay parseable. Dotted-numeric values would
        // otherwise need to be quoted (`'ipsec.1'`), and the strict
        // validator (carina#2980) gates on the DSL spelling.
        return value.replace(['-', '.'], "_");
    }
    // PascalCase / mixed → snake_case (no heck dep, hand-rolled to keep
    // `carina-aws-types` heck-free).
    let mut out = String::with_capacity(value.len() + 4);
    for (i, c) in value.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
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
/// `AwsNormalizer::api_canonicalize_recursive` relies on this alias
/// table to rewrite DSL spellings (e.g. `aes256`) to the AWS canonical
/// (`AES256`) before the wire call. A closed enum whose alias table is
/// empty silently forwards the raw alias spelling to the AWS SDK and
/// triggers `MalformedXML` / `Unknown(...)` errors.
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
