//! Provider-level `assume_role` block: chain `sts:AssumeRole` on top of
//! the ambient credential chain so cross-account workflows can be
//! expressed in `.crn`.
//!
//! Mirrors the Terraform AWS provider's `assume_role` block. The MVP
//! field set is `role_arn`, `session_name`, `external_id`, `duration`;
//! Terraform's broader surface (`transitive_tag_keys`, `source_identity`,
//! `policy`, `policy_arns`, `tags`) is deferred to follow-up issues.

use std::time::Duration;

use carina_core::resource::{ConcreteValue, Value};

/// Parsed, validated `assume_role` block.
///
/// `role_arn` is required; the optional fields are `None` when absent
/// from the DSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssumeRoleConfig {
    pub role_arn: String,
    pub session_name: Option<String>,
    pub external_id: Option<String>,
    pub duration: Option<Duration>,
}

/// Extract the 12-digit AWS account id from an IAM role ARN.
///
/// `arn:aws:iam::123456789012:role/example` → `Some("123456789012")`.
/// Returns `None` for any ARN that does not match the IAM-role shape;
/// callers should treat that as "cannot determine account, skip
/// cross-account guard" rather than as a fatal error — schema-level
/// ARN validation is the host's job, not this helper's.
pub fn account_id_from_role_arn(arn: &str) -> Option<&str> {
    let parts: Vec<&str> = arn.split(':').collect();
    if parts.len() < 6 {
        return None;
    }
    if parts[0] != "arn" || parts[2] != "iam" {
        return None;
    }
    let account = parts[4];
    if account.len() == 12 && account.chars().all(|c| c.is_ascii_digit()) {
        Some(account)
    } else {
        None
    }
}

/// Cross-account guardrail. Returns `Err` when `role_arn`'s account id
/// is determinable and is **not** in `allowed_account_ids`.
///
/// No-op when:
/// - `allowed_account_ids` is empty (guard not configured), or
/// - the role ARN's account id cannot be extracted (treated as
///   unknowable — fail open here, the STS call itself will surface the
///   real error).
pub fn check_cross_account(role_arn: &str, allowed_account_ids: &[String]) -> Result<(), String> {
    if allowed_account_ids.is_empty() {
        return Ok(());
    }
    let Some(target) = account_id_from_role_arn(role_arn) else {
        return Ok(());
    };
    if allowed_account_ids.iter().any(|a| a == target) {
        Ok(())
    } else {
        Err(format!(
            "assume_role.role_arn '{role_arn}' targets account {target}, \
             which is not in allowed_account_ids {allowed_account_ids:?}; \
             refusing to configure provider with this cross-account role"
        ))
    }
}

/// Pull an `assume_role` Struct value out of provider config attrs.
///
/// Returns `Ok(None)` when the attribute is absent (assume_role is
/// optional). Returns `Err` with a human-readable reason when the
/// attribute is present but malformed (missing required field, wrong
/// shape, unparseable duration, etc.).
pub fn extract_assume_role(value: Option<&Value>) -> Result<Option<AssumeRoleConfig>, String> {
    let map = match value {
        None => return Ok(None),
        Some(Value::Concrete(ConcreteValue::Map(m))) => m,
        Some(_) => return Err("assume_role must be a block / map value".to_string()),
    };

    let role_arn = match map.get("role_arn") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        Some(_) => return Err("assume_role.role_arn must be a string".to_string()),
        None => return Err("assume_role.role_arn is required".to_string()),
    };

    let session_name = match map.get("session_name") {
        None => None,
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.clone()),
        Some(_) => return Err("assume_role.session_name must be a string".to_string()),
    };

    let external_id = match map.get("external_id") {
        None => None,
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.clone()),
        Some(_) => return Err("assume_role.external_id must be a string".to_string()),
    };

    // `duration` is declared as `AttributeType::Duration` in the schema.
    // The in-process path (factory.rs) delivers the literal value as
    // `ConcreteValue::Duration`; the WASM/proto path (main.rs) currently
    // delivers it as `ConcreteValue::Int(seconds)` because schema-aware
    // inbound re-typing across the WIT boundary is the deferred
    // follow-up flagged at `carina-plugin-host/src/wasm_convert.rs:60-76`.
    // Accept both so provider behaves the same regardless of host path.
    let duration = match map.get("duration") {
        None => None,
        Some(Value::Concrete(ConcreteValue::Duration(d))) => Some(*d),
        Some(Value::Concrete(ConcreteValue::Int(secs))) if *secs >= 0 => {
            Some(Duration::from_secs(*secs as u64))
        }
        Some(_) => {
            return Err(
                "assume_role.duration must be a Duration literal (e.g., 30min, 1h)".to_string(),
            );
        }
    };

    Ok(Some(AssumeRoleConfig {
        role_arn,
        session_name,
        external_id,
        duration,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn val_str(s: &str) -> Value {
        Value::Concrete(ConcreteValue::String(s.to_string()))
    }

    fn val_map(items: &[(&str, Value)]) -> Value {
        let mut m = indexmap::IndexMap::new();
        for (k, v) in items {
            m.insert((*k).to_string(), v.clone());
        }
        Value::Concrete(ConcreteValue::Map(m))
    }

    fn val_duration(secs: u64) -> Value {
        Value::Concrete(ConcreteValue::Duration(Duration::from_secs(secs)))
    }

    fn val_int(n: i64) -> Value {
        Value::Concrete(ConcreteValue::Int(n))
    }

    #[test]
    fn account_id_from_role_arn_extracts() {
        assert_eq!(
            account_id_from_role_arn("arn:aws:iam::412038850359:role/foo"),
            Some("412038850359")
        );
    }

    #[test]
    fn account_id_from_role_arn_rejects_non_iam() {
        assert_eq!(
            account_id_from_role_arn("arn:aws:s3:::my-bucket"),
            None,
            "S3 ARNs don't have an account id field"
        );
    }

    #[test]
    fn account_id_from_role_arn_rejects_non_12_digit() {
        assert_eq!(account_id_from_role_arn("arn:aws:iam::abc:role/foo"), None);
        assert_eq!(account_id_from_role_arn("arn:aws:iam::123:role/foo"), None);
    }

    #[test]
    fn check_cross_account_no_allow_list_passes() {
        assert!(check_cross_account("arn:aws:iam::412038850359:role/foo", &[]).is_ok());
    }

    #[test]
    fn check_cross_account_target_in_allow_list_passes() {
        let allowed = vec!["412038850359".to_string()];
        assert!(check_cross_account("arn:aws:iam::412038850359:role/foo", &allowed).is_ok());
    }

    #[test]
    fn check_cross_account_target_not_in_allow_list_fails() {
        let allowed = vec!["111111111111".to_string()];
        let err = check_cross_account("arn:aws:iam::412038850359:role/foo", &allowed)
            .expect_err("should reject cross-account role");
        assert!(err.contains("412038850359"), "must name target: {err}");
        assert!(err.contains("111111111111"), "must name allow list: {err}");
        assert!(
            err.contains("assume_role.role_arn"),
            "must name rule: {err}"
        );
    }

    #[test]
    fn check_cross_account_unparseable_arn_fails_open() {
        // When the ARN's account id is not extractable, the guard does
        // not fire. Schema-level ARN validation is the host's job; we
        // let the STS call itself surface the real error rather than
        // double-reporting.
        let allowed = vec!["111111111111".to_string()];
        assert!(check_cross_account("not-an-arn", &allowed).is_ok());
    }

    #[test]
    fn extract_assume_role_absent_returns_none() {
        let attrs: HashMap<String, Value> = HashMap::new();
        assert_eq!(extract_assume_role(attrs.get("assume_role")).unwrap(), None);
    }

    #[test]
    fn extract_assume_role_role_arn_only() {
        let v = val_map(&[("role_arn", val_str("arn:aws:iam::412038850359:role/foo"))]);
        let got = extract_assume_role(Some(&v)).unwrap().unwrap();
        assert_eq!(got.role_arn, "arn:aws:iam::412038850359:role/foo");
        assert_eq!(got.session_name, None);
        assert_eq!(got.external_id, None);
        assert_eq!(got.duration, None);
    }

    #[test]
    fn extract_assume_role_all_fields_with_duration_literal() {
        // In-process path: `duration = 30min` arrives as
        // `ConcreteValue::Duration` (the parser's native Duration form).
        let v = val_map(&[
            ("role_arn", val_str("arn:aws:iam::412038850359:role/foo")),
            ("session_name", val_str("carina")),
            ("external_id", val_str("xid")),
            ("duration", val_duration(1800)),
        ]);
        let got = extract_assume_role(Some(&v)).unwrap().unwrap();
        assert_eq!(got.session_name.as_deref(), Some("carina"));
        assert_eq!(got.external_id.as_deref(), Some("xid"));
        assert_eq!(got.duration, Some(Duration::from_secs(1800)));
    }

    #[test]
    fn extract_assume_role_duration_from_int_seconds() {
        // WASM/proto path: until the schema-aware inbound re-typing
        // follow-up lands (see carina-plugin-host wasm_convert.rs:60-76),
        // a Duration-typed attribute crosses back as
        // `ConcreteValue::Int(seconds)`. Provider must still accept it
        // so behavior matches across in-process vs WASM hosting.
        let v = val_map(&[
            ("role_arn", val_str("arn:aws:iam::412038850359:role/foo")),
            ("duration", val_int(3600)),
        ]);
        let got = extract_assume_role(Some(&v)).unwrap().unwrap();
        assert_eq!(got.duration, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn extract_assume_role_missing_role_arn_fails() {
        let v = val_map(&[("session_name", val_str("oops"))]);
        let err = extract_assume_role(Some(&v)).unwrap_err();
        assert!(err.contains("role_arn is required"), "got: {err}");
    }

    #[test]
    fn extract_assume_role_non_map_fails() {
        let err = extract_assume_role(Some(&val_str("oops"))).unwrap_err();
        assert!(err.contains("block / map"), "got: {err}");
    }

    #[test]
    fn extract_assume_role_string_duration_fails() {
        // Old String-typed schema is gone; a string value at the
        // duration slot is now a schema-level type error rather than
        // a provider-side parsable shape. Surfaces a clear error.
        let v = val_map(&[
            ("role_arn", val_str("arn:aws:iam::412038850359:role/foo")),
            ("duration", val_str("30m")),
        ]);
        let err = extract_assume_role(Some(&v)).unwrap_err();
        assert!(err.contains("Duration literal"), "got: {err}");
    }

    #[test]
    fn extract_assume_role_negative_int_duration_fails() {
        // `Int(secs)` < 0 cannot map to a non-negative `Duration`; this
        // would never happen if the host validates the schema, but the
        // extractor is the last line of defense.
        let v = val_map(&[
            ("role_arn", val_str("arn:aws:iam::412038850359:role/foo")),
            ("duration", val_int(-1)),
        ]);
        let err = extract_assume_role(Some(&v)).unwrap_err();
        assert!(err.contains("Duration literal"), "got: {err}");
    }
}
