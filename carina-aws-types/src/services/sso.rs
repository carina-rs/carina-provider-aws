use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{arn, provider_type, validate_arn};

// ========== SSO / Identity Center helpers ==========

/// Validate an SSO principal ID. Accepts either an IdentityStore user id
/// (`<region>-<uuid>`) or a group id (`<region>-<uuid>`) as defined by the
/// AWS::SSO::Assignment CFN schema (pattern: `^([0-9a-f]{10}-|)[A-Fa-f0-9]{8}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{12}$`).
pub fn validate_sso_principal_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("must not be empty".to_string());
    }
    if id.len() > 64 {
        return Err(format!("must be at most 64 characters, got {}", id.len()));
    }
    Ok(())
}

/// SSO PrincipalId type (user or group id from IdentityStore).
pub fn sso_principal_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("sso", "Principal", "Id")),
        AttributeType::string(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_sso_principal_id(s)
                    .map_err(|reason| format!("Invalid SSO principal ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Validate an SSO Instance ARN
/// (`arn:aws:sso:::instance/ssoins-<hex>`).
pub fn validate_sso_instance_arn(arn: &str) -> Result<(), String> {
    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != "sso" {
        return Err(format!("expected service 'sso', got '{}'", parts[2]));
    }
    if !parts[5].starts_with("instance/") {
        return Err(
            "resource must start with 'instance/' (e.g. 'instance/ssoins-...')".to_string(),
        );
    }
    Ok(())
}

/// SSO Instance ARN type (e.g., "arn:aws:sso:::instance/ssoins-xxxxxxxx").
pub fn sso_instance_arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("sso", "Instance", "Arn")),
        arn(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_sso_instance_arn(s)
                    .map_err(|reason| format!("Invalid SSO instance ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Validate an SSO PermissionSet ARN (`arn:aws:sso:::permissionSet/ssoins-<hex>/ps-<hex>`).
pub fn validate_sso_permission_set_arn(arn: &str) -> Result<(), String> {
    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != "sso" {
        return Err(format!("expected service 'sso', got '{}'", parts[2]));
    }
    if !parts[5].starts_with("permissionSet/") {
        return Err("resource must start with 'permissionSet/'".to_string());
    }
    Ok(())
}

/// SSO PermissionSet ARN type.
pub fn sso_permission_set_arn() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("sso", "PermissionSet", "Arn")),
        arn(),
        None,
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_sso_permission_set_arn(s)
                    .map_err(|reason| format!("Invalid SSO permission set ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
