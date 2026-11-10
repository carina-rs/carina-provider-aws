use carina_core::schema::AttributeType;
use carina_core::schema::{PatternMatch, PatternSpec};

fn validate_iam_name(name: &str, maximum: usize) -> Result<(), String> {
    if name.is_empty() || name.len() > maximum {
        return Err(format!(
            "IAM name must be between 1 and {maximum} ASCII characters"
        ));
    }
    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"_+=,.@-".contains(&c))
    {
        return Err("IAM name allows only ASCII alphanumeric characters and _+=,.@-".to_string());
    }
    Ok(())
}

/// Validate Smithy roleNameType: 1..=64 ASCII letters, digits, or _+=,.@-.
pub fn validate_iam_role_name(name: &str) -> Result<(), String> {
    validate_iam_name(name, 64)
}

/// Validate Smithy policyNameType: 1..=128 ASCII letters, digits, or _+=,.@-.
pub fn validate_iam_policy_name(name: &str) -> Result<(), String> {
    validate_iam_name(name, 128)
}

/// Established whole-value IAM path check, including the root path '/'.
/// Smithy's pathType has a looser alternation (`^(/)|(/.../)$`); member
/// derivation preserves that search and applies this check as an input rule.
pub const IAM_PATH_PATTERN: &str = r"^(?:/|/[\x21-\x7E]+/)\z";

/// Shared standalone anonymous IAM path, with length 1..512.
pub fn iam_path() -> AttributeType {
    AttributeType::refined_string(
        None,
        Some(PatternSpec::new(IAM_PATH_PATTERN, PatternMatch::Full)),
        Some((Some(1), Some(512))),
        None,
    )
}

/// Shared anonymous IAM pathPrefixType; a closing slash is optional.
/// Source: carina-provider-aws/tests/fixtures/smithy/iam.json, pathPrefixType:
/// length 1..512; `^\u002F[\u0021-\u007F]*$` (includes DEL, unlike pathType).
pub fn iam_path_prefix() -> AttributeType {
    AttributeType::refined_string(
        None,
        Some(PatternSpec::new(r"^/[\x21-\x7F]*\z", PatternMatch::Full)),
        Some((Some(1), Some(512))),
        None,
    )
}

/// Validate IAM Role ID format: starts with "AROA" followed by alphanumeric characters.
pub fn validate_iam_role_id(id: &str) -> Result<(), String> {
    let Some(rest) = id.strip_prefix("AROA") else {
        return Err("must start with 'AROA'".to_string());
    };
    if rest.is_empty() {
        return Err("must have characters after 'AROA' prefix".to_string());
    }
    if !rest.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("characters after prefix must be alphanumeric".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::{ConcreteValue, Value};
    use carina_core::schema::RawShape;

    #[test]
    fn iam_paths_are_anonymous_and_preserve_distinct_smithy_grammars() {
        for (attr, expected) in [
            (iam_path(), r"^(?:/|/[\x21-\x7E]+/)\z"),
            (iam_path_prefix(), r"^/[\x21-\x7F]*\z"),
        ] {
            let RawShape::String {
                identity_layer:
                    carina_core::schema::StringIdentityParts {
                        identity,
                        pattern,
                        length,
                        validate,
                        ..
                    },
                ..
            } = attr.raw_shape()
            else {
                panic!("IAM path must be a String");
            };
            assert!(identity.is_none() && validate.is_none());
            assert_eq!(pattern.map(PatternSpec::as_str), Some(expected));
            assert_eq!(length, Some((Some(1), Some(512))));
            let admitted = crate::tests::rootless(attr);
            for valid in [
                "/".to_string(),
                "/carina-588/probe/".to_string(),
                "/aws-reserved/sso.amazonaws.com/".to_string(),
                "/aws-service-role/elasticloadbalancing.amazonaws.com/".to_string(),
                format!("/{}/", "a".repeat(510)),
            ] {
                let value = Value::Concrete(ConcreteValue::String(valid.clone().into()));
                assert!(admitted.validate_collect(&value).is_empty(), "{valid:?}");
            }
            for invalid in [
                "".to_string(),
                "carina".to_string(),
                "/bad path/".to_string(),
                "/valid/\n".to_string(),
                "/é/".to_string(),
                format!("/{}/", "a".repeat(511)),
            ] {
                let value = Value::Concrete(ConcreteValue::String(invalid.clone().into()));
                assert!(!admitted.validate_collect(&value).is_empty(), "{invalid:?}");
            }
        }
        let path = crate::tests::rootless(iam_path());
        let prefix = crate::tests::rootless(iam_path_prefix());
        for value in [
            "/no-trailing".to_string(),
            "/\u{7f}/".to_string(),
            format!("/{}", "a".repeat(511)),
        ] {
            let value = Value::Concrete(ConcreteValue::String(value.into()));
            assert!(!path.validate_collect(&value).is_empty());
            assert!(prefix.validate_collect(&value).is_empty());
        }
    }
}
