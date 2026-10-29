use carina_core::schema::{AttributeType, StructField};

/// Validate an IdentityStore id (`d-<10-hex>` or a 36-char UUID).
pub fn validate_identity_store_id(id: &str) -> Result<(), String> {
    let looks_like_d = id
        .strip_prefix("d-")
        .is_some_and(|rest| rest.len() == 10 && rest.chars().all(|c| c.is_ascii_hexdigit()));
    let looks_like_uuid = id.len() == 36
        && id.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if looks_like_d || looks_like_uuid {
        Ok(())
    } else {
        Err("must be d-<10 hex> or a 36-char UUID".to_string())
    }
}

/// Structured name returned by Identity Store's DescribeUser operation.
pub fn identitystore_user_name() -> AttributeType {
    AttributeType::struct_(
        "IdentityStoreUserName".to_string(),
        [
            "formatted",
            "family_name",
            "given_name",
            "middle_name",
            "honorific_prefix",
            "honorific_suffix",
        ]
        .into_iter()
        .map(|name| StructField::optional(name, AttributeType::string()).build())
        .collect(),
    )
}
