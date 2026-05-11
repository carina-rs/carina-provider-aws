//! Provider-level account guard: enforce `allowed_account_ids` and
//! `forbidden_account_ids` against the caller's STS identity.
//!
//! Mirrors Terraform AWS provider's `allowed_account_ids` /
//! `forbidden_account_ids`. The check runs once during provider
//! initialization (before any read/plan/apply mutation) and fails fast
//! when the credentials in use point at the wrong AWS account.

use carina_core::resource::{ConcreteValue, Value};

use crate::AwsProvider;
use crate::helpers::sdk_error_message;

/// Pure policy check. Returns `Ok(())` when the caller's account is
/// permitted by the configured allow/forbid lists, otherwise an error
/// string that names both the rule and the actual account ID.
///
/// Both lists empty is treated as "no check configured" and always
/// passes — preserves the pre-issue-233 behavior for users who did
/// not opt in to the guard.
pub fn check_account_id(
    account_id: &str,
    allowed: &[String],
    forbidden: &[String],
) -> Result<(), String> {
    if !forbidden.is_empty() && forbidden.iter().any(|a| a == account_id) {
        return Err(format!(
            "AWS account ID {account_id} is in forbidden_account_ids {forbidden:?}; \
             refusing to operate against this account"
        ));
    }
    if !allowed.is_empty() && !allowed.iter().any(|a| a == account_id) {
        return Err(format!(
            "AWS account ID {account_id} is not in allowed_account_ids {allowed:?}; \
             refusing to operate against this account"
        ));
    }
    Ok(())
}

/// Extract a `Value::List` of strings from a provider config attribute.
///
/// Non-string entries are dropped silently — type-level validation is
/// the host's responsibility via
/// `ProviderFactory::provider_config_attribute_types`; by the time we
/// see the value here the host has already enforced
/// `list(string)`. Missing keys yield an empty `Vec`, which the policy
/// treats as "list not configured".
pub fn extract_string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Concrete(ConcreteValue::List(items))) => items
            .iter()
            .filter_map(|v| match v {
                Value::Concrete(ConcreteValue::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

impl AwsProvider {
    /// Run the configured account guard. No-op when both
    /// `allowed_account_ids` and `forbidden_account_ids` are empty.
    ///
    /// Otherwise calls STS `GetCallerIdentity` and validates the
    /// returned account against the configured lists, returning an
    /// error string suitable for surfacing through
    /// `CarinaProvider::initialize`.
    pub async fn verify_account_id(&self) -> Result<(), String> {
        if self.allowed_account_ids.is_empty() && self.forbidden_account_ids.is_empty() {
            return Ok(());
        }

        let response = self
            .sts_client
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| {
                sdk_error_message("Failed to get STS caller identity for account guard", &e)
            })?;

        let account_id = response.account().ok_or_else(|| {
            "STS GetCallerIdentity returned no account ID; cannot enforce account guard".to_string()
        })?;

        check_account_id(
            account_id,
            &self.allowed_account_ids,
            &self.forbidden_account_ids,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_lists_empty_passes() {
        assert!(check_account_id("123456789012", &[], &[]).is_ok());
    }

    #[test]
    fn allowed_match_passes() {
        let allowed = vec!["123456789012".to_string()];
        assert!(check_account_id("123456789012", &allowed, &[]).is_ok());
    }

    #[test]
    fn allowed_miss_fails_and_names_both_lists_and_account() {
        let allowed = vec!["111111111111".to_string(), "222222222222".to_string()];
        let err = check_account_id("999999999999", &allowed, &[])
            .expect_err("should fail when caller is not in allowed list");
        assert!(
            err.contains("999999999999"),
            "error must name actual account: {err}"
        );
        assert!(
            err.contains("111111111111"),
            "error must name expected list: {err}"
        );
        assert!(
            err.contains("222222222222"),
            "error must name full expected list: {err}"
        );
        assert!(
            err.contains("allowed_account_ids"),
            "error must name the rule: {err}"
        );
    }

    #[test]
    fn forbidden_match_fails_and_names_both_lists_and_account() {
        let forbidden = vec!["412038850359".to_string()];
        let err = check_account_id("412038850359", &[], &forbidden)
            .expect_err("should fail when caller is in forbidden list");
        assert!(
            err.contains("412038850359"),
            "error must name actual account: {err}"
        );
        assert!(
            err.contains("forbidden_account_ids"),
            "error must name the rule: {err}"
        );
    }

    #[test]
    fn forbidden_takes_precedence_over_allowed() {
        // If for some reason the same account appears in both lists,
        // forbid wins — the safer outcome.
        let allowed = vec!["123456789012".to_string()];
        let forbidden = vec!["123456789012".to_string()];
        let err = check_account_id("123456789012", &allowed, &forbidden)
            .expect_err("forbidden must win when caller is in both lists");
        assert!(
            err.contains("forbidden_account_ids"),
            "must surface the forbidden rule first: {err}"
        );
    }

    #[test]
    fn allowed_and_forbidden_both_set_caller_in_allowed_only_passes() {
        let allowed = vec!["123456789012".to_string()];
        let forbidden = vec!["999999999999".to_string()];
        assert!(check_account_id("123456789012", &allowed, &forbidden).is_ok());
    }

    #[test]
    fn extract_string_list_handles_missing() {
        assert!(extract_string_list(None).is_empty());
    }

    #[test]
    fn extract_string_list_handles_non_list() {
        assert!(
            extract_string_list(Some(&Value::Concrete(ConcreteValue::String("oops".into()))))
                .is_empty()
        );
    }

    #[test]
    fn extract_string_list_extracts_strings_in_order() {
        let v = Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("aaa".into())),
            Value::Concrete(ConcreteValue::String("bbb".into())),
        ]));
        assert_eq!(
            extract_string_list(Some(&v)),
            vec!["aaa".to_string(), "bbb".to_string()]
        );
    }

    #[test]
    fn extract_string_list_drops_non_string_entries() {
        // Type validation happens host-side via
        // `provider_config_attribute_types`; this is just the safety net.
        let v = Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("aaa".into())),
            Value::Concrete(ConcreteValue::Int(42)),
            Value::Concrete(ConcreteValue::String("bbb".into())),
        ]));
        assert_eq!(
            extract_string_list(Some(&v)),
            vec!["aaa".to_string(), "bbb".to_string()]
        );
    }
}
