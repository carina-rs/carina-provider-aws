use crate::validate_arn;

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
