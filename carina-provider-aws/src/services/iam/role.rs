use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};

impl AwsProvider {
    /// Read an IAM Role
    pub(crate) async fn read_iam_role(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .iam_client
            .get_role()
            .role_name(identifier)
            .send()
            .await;

        match result {
            Ok(output) => {
                if let Some(role) = output.role() {
                    let mut attributes = HashMap::new();

                    let identifier_value = Self::extract_iam_role_attributes(role, &mut attributes);

                    // Extract tags
                    let tags = role.tags();
                    if !tags.is_empty() {
                        let mut tag_map = IndexMap::new();
                        for tag in tags {
                            let key = tag.key();
                            let val = tag.value();
                            tag_map.insert(key.to_string(), Value::String(val.to_string()));
                        }
                        if !tag_map.is_empty() {
                            attributes.insert("tags".to_string(), Value::Map(tag_map));
                        }
                    }

                    let state = State::existing(id.clone(), attributes);
                    Ok(if let Some(id_val) = identifier_value {
                        state.with_identifier(id_val)
                    } else {
                        state
                    })
                } else {
                    Ok(State::not_found(id.clone()))
                }
            }
            Err(e) => {
                // Check if it's a NoSuchEntity error
                if let Some(service_err) = e.as_service_error()
                    && service_err.is_no_such_entity_exception()
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::new(sdk_error_message("Failed to get IAM role", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    /// Create an IAM Role
    pub(crate) async fn create_iam_role(&self, resource: Resource) -> ProviderResult<State> {
        let role_name = require_string_attr(&resource, "role_name")?;

        let assume_role_policy_document =
            resolve_iam_policy_attr(&resource, "assume_role_policy_document")?;

        let mut req = self
            .iam_client
            .create_role()
            .role_name(&role_name)
            .assume_role_policy_document(&assume_role_policy_document);

        if let Some(Value::String(desc)) = resource.get_attr("description") {
            req = req.description(desc);
        }

        if let Some(Value::String(path)) = resource.get_attr("path") {
            req = req.path(path);
        }

        if let Some(Value::Int(duration)) = resource.get_attr("max_session_duration") {
            req = req.max_session_duration(*duration as i32);
        }

        // Apply tags at creation time
        if let Some(Value::Map(tag_map)) = resource.get_attr("tags") {
            for (key, value) in tag_map {
                if let Value::String(val) = value {
                    let tag = aws_sdk_iam::types::Tag::builder()
                        .key(key)
                        .value(val)
                        .build()
                        .map_err(|e| {
                            ProviderError::new(sdk_error_message("Failed to build tag", &e))
                                .for_resource(resource.id.clone())
                        })?;
                    req = req.tags(tag);
                }
            }
        }

        req.send().await.map_err(|e| {
            ProviderError::new(sdk_error_message("Failed to create IAM role", &e))
                .for_resource(resource.id.clone())
        })?;

        self.read_iam_role(&resource.id, Some(&role_name)).await
    }

    /// Update an IAM Role
    pub(crate) async fn update_iam_role(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        // Update assume role policy document
        if to.get_attr("assume_role_policy_document").is_some() {
            let policy_doc = resolve_iam_policy_attr(&to, "assume_role_policy_document")?;
            self.iam_client
                .update_assume_role_policy()
                .role_name(identifier)
                .policy_document(&policy_doc)
                .send()
                .await
                .map_err(|e| {
                    ProviderError::new(sdk_error_message("Failed to update assume role policy", &e))
                        .for_resource(id.clone())
                })?;
        }

        // Update description and max_session_duration via update_role
        let mut needs_update = false;
        let mut req = self.iam_client.update_role().role_name(identifier);

        if let Some(Value::String(desc)) = to.get_attr("description") {
            req = req.description(desc);
            needs_update = true;
        }

        if let Some(Value::Int(duration)) = to.get_attr("max_session_duration") {
            req = req.max_session_duration(*duration as i32);
            needs_update = true;
        }

        if needs_update {
            req.send().await.map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to update IAM role", &e))
                    .for_resource(id.clone())
            })?;
        }

        // Update tags
        self.apply_iam_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        self.read_iam_role(&id, Some(identifier)).await
    }

    /// Delete an IAM Role
    pub(crate) async fn delete_iam_role(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.iam_client
            .delete_role()
            .role_name(identifier)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to delete IAM role", &e))
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Apply IAM tags (create/delete tag differences)
    async fn apply_iam_tags(
        &self,
        id: &ResourceId,
        role_name: &str,
        desired: &HashMap<String, Value>,
        current: Option<&HashMap<String, Value>>,
    ) -> ProviderResult<()> {
        let desired_tags = match desired.get("tags") {
            Some(Value::Map(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        let current_tags = match current.and_then(|c| c.get("tags")) {
            Some(Value::Map(m)) => m.clone(),
            _ => IndexMap::new(),
        };

        // Tags to remove
        let keys_to_remove: Vec<String> = current_tags
            .keys()
            .filter(|k| !desired_tags.contains_key(*k))
            .cloned()
            .collect();

        if !keys_to_remove.is_empty() {
            let mut req = self.iam_client.untag_role().role_name(role_name);
            for key in &keys_to_remove {
                req = req.tag_keys(key);
            }
            req.send().await.map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to untag IAM role", &e))
                    .for_resource(id.clone())
            })?;
        }

        // Tags to add/update
        let mut tags_to_add = Vec::new();
        for (key, value) in &desired_tags {
            if let Value::String(val) = value {
                let should_add = match current_tags.get(key) {
                    Some(Value::String(current_val)) => current_val != val,
                    _ => true,
                };
                if should_add {
                    let tag = aws_sdk_iam::types::Tag::builder()
                        .key(key)
                        .value(val)
                        .build()
                        .map_err(|e| {
                            ProviderError::new(sdk_error_message("Failed to build tag", &e))
                                .for_resource(id.clone())
                        })?;
                    tags_to_add.push(tag);
                }
            }
        }

        if !tags_to_add.is_empty() {
            let mut req = self.iam_client.tag_role().role_name(role_name);
            for tag in tags_to_add {
                req = req.tags(tag);
            }
            req.send().await.map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to tag IAM role", &e))
                    .for_resource(id.clone())
            })?;
        }

        Ok(())
    }

    /// Extract iam.Role attributes from the SDK response.
    ///
    /// Lives here (not in `provider_generated.rs`) because
    /// `assume_role_policy_document` needs URL-decoding and a JSON →
    /// `Value::Map` parse step that the codegen template can't express;
    /// `scan_manual_methods` picks it up by name and the codegen skips
    /// emitting a duplicate stub. Returns the role name as the
    /// state identifier.
    pub(crate) fn extract_iam_role_attributes(
        obj: &aws_sdk_iam::types::Role,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        // arn, path, role_id, role_name return &str (always present per
        // Smithy `@required`); skip the empty-string sentinel the SDK
        // sometimes uses when the field is absent on the wire.
        let arn = obj.arn();
        if !arn.is_empty() {
            attributes.insert("arn".to_string(), Value::String(arn.to_string()));
        }
        if let Some(v) = obj.assume_role_policy_document() {
            // The SDK URL-encodes the policy document.
            let decoded = urlencoding::decode(v).unwrap_or_else(|_| v.into());
            // Convert JSON string to Value::Map with snake_case keys for
            // struct comparison; fall back to the raw string if the
            // policy is malformed.
            let policy_value = iam_policy_json_to_value(&decoded)
                .unwrap_or_else(|_| Value::String(decoded.into_owned()));
            attributes.insert("assume_role_policy_document".to_string(), policy_value);
        }
        if let Some(v) = obj.description() {
            attributes.insert("description".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = obj.max_session_duration() {
            attributes.insert("max_session_duration".to_string(), Value::Int(v as i64));
        }
        let path = obj.path();
        if !path.is_empty() {
            attributes.insert("path".to_string(), Value::String(path.to_string()));
        }
        let role_id = obj.role_id();
        if !role_id.is_empty() {
            attributes.insert("role_id".to_string(), Value::String(role_id.to_string()));
        }
        let role_name = obj.role_name();
        if !role_name.is_empty() {
            attributes.insert(
                "role_name".to_string(),
                Value::String(role_name.to_string()),
            );
            Some(role_name.to_string())
        } else {
            None
        }
    }
}

/// Convert a Carina Value (Map with snake_case keys) to a JSON string
/// with PascalCase keys suitable for the IAM API.
///
/// The conversion is *position-aware*: only IAM standard fields (e.g.
/// `version` → `Version`, `statement` → `Statement`) and condition
/// operators (e.g. `bool` → `Bool`, `string_equals` → `StringEquals`)
/// are case-converted. Condition variable keys (`aws:SecureTransport`,
/// `s3:prefix`, ...), ARN values, and Action / Resource literals are
/// passed through verbatim. A blanket `snake_to_pascal` would mangle
/// them (e.g. `aws:SecureTransport` → `Aws:SecureTransport`), which
/// AWS treats as an unknown — and therefore unenforced — condition.
pub fn value_to_iam_policy_json(value: &Value) -> Result<String, String> {
    let json_value = policy_doc_to_json(value);
    serde_json::to_string(&json_value).map_err(|e| format!("JSON serialization failed: {}", e))
}

/// Resolve an IAM-style policy attribute (e.g. `assume_role_policy_document`,
/// `policy`) on a `Resource` into a JSON string suitable for the AWS API.
///
/// Accepts either a pre-serialized JSON string or a Carina `Value::Map`
/// (block-syntax DSL form). Errors when the attribute is missing or has an
/// unsupported type.
pub fn resolve_iam_policy_attr(resource: &Resource, attr_name: &str) -> ProviderResult<String> {
    match resource.get_attr(attr_name) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(value @ Value::Map(_)) => value_to_iam_policy_json(value).map_err(|e| {
            ProviderError::new(format!("Failed to convert {}: {}", attr_name, e))
                .for_resource(resource.id.clone())
        }),
        _ => Err(ProviderError::new(format!(
            "{} is required (must be a JSON string or block)",
            attr_name
        ))
        .for_resource(resource.id.clone())),
    }
}

/// IAM policy top-level standard fields (snake → PascalCase).
const POLICY_TOP_FIELDS: &[(&str, &str)] = &[
    ("version", "Version"),
    ("id", "Id"),
    ("statement", "Statement"),
];

/// IAM policy Statement fields (snake → PascalCase).
const STATEMENT_FIELDS: &[(&str, &str)] = &[
    ("sid", "Sid"),
    ("effect", "Effect"),
    ("principal", "Principal"),
    ("not_principal", "NotPrincipal"),
    ("action", "Action"),
    ("not_action", "NotAction"),
    ("resource", "Resource"),
    ("not_resource", "NotResource"),
    ("condition", "Condition"),
];

/// IAM Principal map field names (snake → PascalCase).
const PRINCIPAL_FIELDS: &[(&str, &str)] = &[
    ("aws", "AWS"),
    ("federated", "Federated"),
    ("service", "Service"),
    ("canonical_user", "CanonicalUser"),
];

fn lookup_pascal(
    table: &'static [(&'static str, &'static str)],
    snake: &str,
) -> Option<&'static str> {
    table.iter().find_map(|(s, p)| (*s == snake).then_some(*p))
}

fn lookup_snake(
    table: &'static [(&'static str, &'static str)],
    pascal: &str,
) -> Option<&'static str> {
    table.iter().find_map(|(s, p)| (*p == pascal).then_some(*s))
}

/// Convert a Carina policy document Value to JSON. Top-level fields are
/// case-mapped via `POLICY_TOP_FIELDS`; the value of `Statement` is
/// further converted by `policy_statement_to_json`.
fn policy_doc_to_json(value: &Value) -> serde_json::Value {
    let Value::Map(map) = value else {
        return scalar_to_json(value);
    };
    let mut obj = serde_json::Map::new();
    for (k, v) in map {
        let pascal_key = lookup_pascal(POLICY_TOP_FIELDS, k).unwrap_or(k.as_str());
        let json_value = if k == "statement" {
            match v {
                Value::List(items) => {
                    serde_json::Value::Array(items.iter().map(policy_statement_to_json).collect())
                }
                Value::Map(_) => policy_statement_to_json(v),
                _ => scalar_to_json(v),
            }
        } else {
            scalar_or_passthrough_to_json(v)
        };
        obj.insert(pascal_key.to_string(), json_value);
    }
    serde_json::Value::Object(obj)
}

fn policy_statement_to_json(value: &Value) -> serde_json::Value {
    let Value::Map(map) = value else {
        return scalar_to_json(value);
    };
    let mut obj = serde_json::Map::new();
    for (k, v) in map {
        let pascal_key = lookup_pascal(STATEMENT_FIELDS, k).unwrap_or(k.as_str());
        let json_value = match k.as_str() {
            "principal" | "not_principal" => principal_to_json(v),
            "condition" => condition_to_json(v),
            _ => scalar_or_passthrough_to_json(v),
        };
        obj.insert(pascal_key.to_string(), json_value);
    }
    serde_json::Value::Object(obj)
}

fn principal_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = lookup_pascal(PRINCIPAL_FIELDS, k).unwrap_or(k.as_str());
                obj.insert(key.to_string(), scalar_or_passthrough_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => scalar_to_json(value),
    }
}

fn condition_to_json(value: &Value) -> serde_json::Value {
    let Value::Map(operators) = value else {
        return scalar_to_json(value);
    };
    let mut obj = serde_json::Map::new();
    for (op_key, kv_value) in operators {
        let op_pascal =
            carina_aws_types::condition_operator_to_aws(op_key).unwrap_or_else(|| op_key.clone());
        let kv_json = match kv_value {
            // Inner map keys are condition variables (e.g. "aws:SecureTransport")
            // and must pass through verbatim — no case conversion.
            Value::Map(inner) => {
                let mut m = serde_json::Map::new();
                for (var, val) in inner {
                    m.insert(var.clone(), scalar_or_passthrough_to_json(val));
                }
                serde_json::Value::Object(m)
            }
            _ => scalar_to_json(kv_value),
        };
        obj.insert(op_pascal, kv_json);
    }
    serde_json::Value::Object(obj)
}

/// Scalar-or-list-of-scalars passthrough: used for Action / Resource /
/// Sid / Effect / condition variable values. No key conversion.
fn scalar_or_passthrough_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(scalar_or_passthrough_to_json).collect())
        }
        Value::Map(map) => {
            // Generic map (e.g. nested condition value map): keys verbatim.
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), scalar_or_passthrough_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => scalar_to_json(value),
    }
}

fn scalar_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Int(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::json!(*f),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::List(_) | Value::Map(_) => scalar_or_passthrough_to_json(value),
        _ => serde_json::Value::Null,
    }
}

/// Convert an IAM policy JSON string (PascalCase keys) to a Carina Value::Map (snake_case keys).
///
/// Mirrors `value_to_iam_policy_json`'s position-aware case conversion:
/// only IAM standard fields and condition operators are mapped to
/// snake_case; condition variable keys, ARN values, and Action / Resource
/// literals are preserved verbatim.
pub fn iam_policy_json_to_value(json_str: &str) -> Result<Value, String> {
    let json: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse failed: {}", e))?;
    Ok(json_to_policy_doc(&json))
}

fn json_to_policy_doc(json: &serde_json::Value) -> Value {
    let serde_json::Value::Object(obj) = json else {
        return json_scalar_to_value(json);
    };
    let mut map = IndexMap::new();
    for (k, v) in obj {
        if v.is_null() {
            continue;
        }
        let snake_key = lookup_snake(POLICY_TOP_FIELDS, k).unwrap_or(k.as_str());
        let value = if snake_key == "statement" {
            match v {
                serde_json::Value::Array(items) => {
                    Value::List(items.iter().map(json_to_policy_statement).collect())
                }
                serde_json::Value::Object(_) => json_to_policy_statement(v),
                _ => json_scalar_to_value(v),
            }
        } else {
            json_scalar_or_passthrough_to_value(v)
        };
        map.insert(snake_key.to_string(), value);
    }
    Value::Map(map)
}

fn json_to_policy_statement(json: &serde_json::Value) -> Value {
    let serde_json::Value::Object(obj) = json else {
        return json_scalar_to_value(json);
    };
    let mut map = IndexMap::new();
    for (k, v) in obj {
        if v.is_null() {
            continue;
        }
        let snake_key = lookup_snake(STATEMENT_FIELDS, k).unwrap_or(k.as_str());
        let value = match snake_key {
            "principal" | "not_principal" => json_to_principal(v),
            "condition" => json_to_condition(v),
            _ => json_scalar_or_passthrough_to_value(v),
        };
        map.insert(snake_key.to_string(), value);
    }
    Value::Map(map)
}

fn json_to_principal(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Object(obj) => {
            let mut map = IndexMap::new();
            for (k, v) in obj {
                if v.is_null() {
                    continue;
                }
                let key = lookup_snake(PRINCIPAL_FIELDS, k).unwrap_or(k.as_str());
                map.insert(key.to_string(), json_scalar_or_passthrough_to_value(v));
            }
            Value::Map(map)
        }
        _ => json_scalar_to_value(json),
    }
}

fn json_to_condition(json: &serde_json::Value) -> Value {
    let serde_json::Value::Object(obj) = json else {
        return json_scalar_to_value(json);
    };
    let mut map = IndexMap::new();
    for (op_key, kv_value) in obj {
        if kv_value.is_null() {
            continue;
        }
        let op_snake =
            carina_aws_types::condition_operator_to_snake(op_key).unwrap_or_else(|| op_key.clone());
        let kv = match kv_value {
            // Condition variable keys (e.g. "aws:SecureTransport") pass through.
            serde_json::Value::Object(inner) => {
                let mut m = IndexMap::new();
                for (var, val) in inner {
                    if val.is_null() {
                        continue;
                    }
                    m.insert(var.clone(), json_scalar_or_passthrough_to_value(val));
                }
                Value::Map(m)
            }
            _ => json_scalar_to_value(kv_value),
        };
        map.insert(op_snake, kv);
    }
    Value::Map(map)
}

fn json_scalar_or_passthrough_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Array(items) => Value::List(
            items
                .iter()
                .map(json_scalar_or_passthrough_to_value)
                .collect(),
        ),
        serde_json::Value::Object(obj) => {
            let mut map = IndexMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_scalar_or_passthrough_to_value(v));
            }
            Value::Map(map)
        }
        _ => json_scalar_to_value(json),
    }
}

fn json_scalar_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => Value::Bool(*b),
        // JSON null in policy documents is uncommon and has no faithful
        // Carina counterpart (Value has no Null variant). Map to empty
        // string only as a last resort; callers should normally treat
        // null fields as absent.
        serde_json::Value::Null => Value::String(String::new()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            json_scalar_or_passthrough_to_value(json)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_resource(policy: Option<Value>) -> Resource {
        let mut r = Resource::new("test.Type", "test");
        if let Some(v) = policy {
            r.set_attr("policy", v);
        }
        r
    }

    #[test]
    fn resolve_iam_policy_attr_accepts_json_string_passthrough() {
        let json = r#"{"Version":"2012-10-17","Statement":[]}"#;
        let r = make_resource(Some(Value::String(json.to_string())));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("string passthrough");
        assert_eq!(resolved, json);
    }

    #[test]
    fn resolve_iam_policy_attr_serializes_map_to_pascal_case_json() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        let mut stmt = IndexMap::new();
        stmt.insert("effect".to_string(), Value::String("Allow".to_string()));
        stmt.insert(
            "action".to_string(),
            Value::String("s3:GetObject".to_string()),
        );
        policy.insert("statement".to_string(), Value::List(vec![Value::Map(stmt)]));

        let r = make_resource(Some(Value::Map(policy)));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("map → JSON");

        assert!(resolved.contains("\"Version\""));
        assert!(resolved.contains("\"Statement\""));
        assert!(resolved.contains("\"Effect\""));
        assert!(resolved.contains("\"Action\""));
        assert!(!resolved.contains("\"version\""));
    }

    #[test]
    fn resolve_iam_policy_attr_errors_when_missing() {
        let r = make_resource(None);
        let err = resolve_iam_policy_attr(&r, "policy").expect_err("missing");
        assert!(format!("{}", err).contains("policy is required"));
    }

    #[test]
    fn resolve_iam_policy_attr_errors_on_non_string_non_map() {
        let r = make_resource(Some(Value::Bool(true)));
        let err = resolve_iam_policy_attr(&r, "policy").expect_err("invalid type");
        assert!(format!("{}", err).contains("policy is required"));
    }

    /// A full deny-insecure-transport policy mirroring the acceptance fixture.
    /// Exercises principal map, list-valued action/resource, and a
    /// `bool` condition with the `aws:SecureTransport` value key.
    fn full_deny_policy_value() -> Value {
        let mut principal = IndexMap::new();
        principal.insert("aws".to_string(), Value::String("*".to_string()));

        let mut bool_inner = IndexMap::new();
        bool_inner.insert(
            "aws:SecureTransport".to_string(),
            Value::String("false".to_string()),
        );
        let mut condition = IndexMap::new();
        condition.insert("bool".to_string(), Value::Map(bool_inner));

        let mut stmt = IndexMap::new();
        stmt.insert(
            "sid".to_string(),
            Value::String("DenyInsecureTransport".to_string()),
        );
        stmt.insert("effect".to_string(), Value::String("Deny".to_string()));
        stmt.insert("principal".to_string(), Value::Map(principal));
        stmt.insert("action".to_string(), Value::String("s3:*".to_string()));
        stmt.insert(
            "resource".to_string(),
            Value::List(vec![
                Value::String("arn:aws:s3:::my-bucket".to_string()),
                Value::String("arn:aws:s3:::my-bucket/*".to_string()),
            ]),
        );
        stmt.insert("condition".to_string(), Value::Map(condition));

        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        doc.insert("statement".to_string(), Value::List(vec![Value::Map(stmt)]));
        Value::Map(doc)
    }

    #[test]
    fn value_to_iam_policy_json_preserves_condition_variable_keys() {
        let json = value_to_iam_policy_json(&full_deny_policy_value()).expect("ok");

        // Top-level + statement fields PascalCase.
        assert!(json.contains("\"Version\""));
        assert!(json.contains("\"Statement\""));
        assert!(json.contains("\"Effect\""));
        assert!(json.contains("\"Sid\""));
        assert!(json.contains("\"Principal\""));
        assert!(json.contains("\"Condition\""));

        // Principal "aws" is mapped to PascalCase "AWS" (IAM-specific).
        assert!(json.contains("\"AWS\""));

        // Condition operator "bool" is mapped to "Bool".
        assert!(json.contains("\"Bool\""));

        // CRITICAL: condition variable keys with ":" must NOT be case-converted.
        // A blanket snake_to_pascal would produce "Aws:SecureTransport", which
        // AWS treats as an unknown — and therefore unenforced — key.
        assert!(
            json.contains("\"aws:SecureTransport\""),
            "expected literal 'aws:SecureTransport' in {}",
            json
        );
        assert!(!json.contains("\"Aws:SecureTransport\""));
    }

    #[test]
    fn value_to_iam_policy_json_preserves_action_and_resource_literals() {
        let json = value_to_iam_policy_json(&full_deny_policy_value()).expect("ok");
        assert!(json.contains("\"s3:*\""));
        assert!(json.contains("\"arn:aws:s3:::my-bucket\""));
        assert!(json.contains("\"arn:aws:s3:::my-bucket/*\""));
    }

    #[test]
    fn iam_policy_json_to_value_inverts_value_to_iam_policy_json() {
        let original = full_deny_policy_value();
        let json = value_to_iam_policy_json(&original).expect("to json");
        let roundtripped = iam_policy_json_to_value(&json).expect("from json");
        assert_eq!(original, roundtripped, "round-trip must be lossless");
    }

    #[test]
    fn iam_policy_json_to_value_handles_pascal_input() {
        // Simulates AWS's GetBucketPolicy response shape.
        let aws_json = r#"{
            "Version": "2012-10-17",
            "Statement": [{
                "Sid": "DenyInsecureTransport",
                "Effect": "Deny",
                "Principal": {"AWS": "*"},
                "Action": "s3:*",
                "Resource": ["arn:aws:s3:::my-bucket", "arn:aws:s3:::my-bucket/*"],
                "Condition": {"Bool": {"aws:SecureTransport": "false"}}
            }]
        }"#;
        let value = iam_policy_json_to_value(aws_json).expect("ok");
        let Value::Map(doc) = &value else {
            panic!("expected map");
        };
        // Top-level snake_case.
        assert!(doc.contains_key("version"));
        assert!(doc.contains_key("statement"));

        // Drill into statement[0].condition.bool.
        let Value::List(stmts) = doc.get("statement").unwrap() else {
            panic!();
        };
        let Value::Map(stmt) = &stmts[0] else {
            panic!()
        };
        let Value::Map(cond) = stmt.get("condition").unwrap() else {
            panic!()
        };
        let Value::Map(bool_inner) = cond.get("bool").unwrap() else {
            panic!()
        };
        // CRITICAL: condition variable key preserved verbatim.
        assert!(bool_inner.contains_key("aws:SecureTransport"));
        assert!(!bool_inner.contains_key("aws:secure_transport"));

        // Principal {"AWS"} → snake "aws".
        let Value::Map(principal) = stmt.get("principal").unwrap() else {
            panic!()
        };
        assert!(principal.contains_key("aws"));
        assert_eq!(
            principal.get("aws"),
            Some(&Value::String("*".to_string())),
            "principal value should round-trip verbatim"
        );
    }

    #[test]
    fn id_field_round_trips() {
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        doc.insert("id".to_string(), Value::String("MyPolicyId".to_string()));
        doc.insert("statement".to_string(), Value::List(vec![]));

        let original = Value::Map(doc);
        let json = value_to_iam_policy_json(&original).expect("ok");
        assert!(json.contains("\"Id\""));
        assert!(json.contains("\"MyPolicyId\""));

        let roundtripped = iam_policy_json_to_value(&json).expect("ok");
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn statement_as_single_object_form_is_handled() {
        // Some tools emit Statement as a single object rather than a list.
        let aws_json = r#"{
            "Version": "2012-10-17",
            "Statement": {
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "Resource": "*"
            }
        }"#;
        let value = iam_policy_json_to_value(aws_json).expect("ok");
        let Value::Map(doc) = &value else { panic!() };
        let Value::Map(stmt) = doc.get("statement").unwrap() else {
            panic!("statement should be a Map for single-object form")
        };
        assert_eq!(
            stmt.get("effect"),
            Some(&Value::String("Allow".to_string()))
        );
    }

    #[test]
    fn principal_string_form_is_handled() {
        // Principal: "*" — wildcard as a bare string, not a map.
        let aws_json = r#"{
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Deny",
                "Principal": "*",
                "Action": "s3:*",
                "Resource": "*"
            }]
        }"#;
        let value = iam_policy_json_to_value(aws_json).expect("ok");
        let Value::Map(doc) = &value else { panic!() };
        let Value::List(stmts) = doc.get("statement").unwrap() else {
            panic!()
        };
        let Value::Map(stmt) = &stmts[0] else {
            panic!()
        };
        assert_eq!(
            stmt.get("principal"),
            Some(&Value::String("*".to_string())),
            "principal string form should pass through verbatim"
        );
    }

    #[test]
    fn condition_qualifier_round_trips() {
        // ForAllValues:StringEquals + IfExists are handled by
        // condition_operator_to_aws/snake; verify they round-trip when used
        // through the policy converters.
        let mut inner = IndexMap::new();
        inner.insert(
            "aws:TagKeys".to_string(),
            Value::List(vec![Value::String("Environment".to_string())]),
        );
        let mut condition = IndexMap::new();
        condition.insert(
            "for_all_values_string_equals_if_exists".to_string(),
            Value::Map(inner),
        );
        let mut stmt = IndexMap::new();
        stmt.insert("effect".to_string(), Value::String("Allow".to_string()));
        stmt.insert("action".to_string(), Value::String("s3:*".to_string()));
        stmt.insert("condition".to_string(), Value::Map(condition));
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        doc.insert("statement".to_string(), Value::List(vec![Value::Map(stmt)]));

        let original = Value::Map(doc);
        let json = value_to_iam_policy_json(&original).expect("ok");
        assert!(
            json.contains("\"ForAllValues:StringEqualsIfExists\""),
            "expected qualified op in {}",
            json
        );
        assert!(json.contains("\"aws:TagKeys\""));

        let roundtripped = iam_policy_json_to_value(&json).expect("ok");
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn condition_operator_is_translated_round_trip() {
        // string_equals ↔ StringEquals via condition_operator_to_aws/snake.
        let mut inner = IndexMap::new();
        inner.insert(
            "aws:PrincipalOrgID".to_string(),
            Value::String("o-1234567890".to_string()),
        );
        let mut condition = IndexMap::new();
        condition.insert("string_equals".to_string(), Value::Map(inner));
        let mut stmt = IndexMap::new();
        stmt.insert("effect".to_string(), Value::String("Allow".to_string()));
        stmt.insert(
            "action".to_string(),
            Value::String("s3:GetObject".to_string()),
        );
        stmt.insert("condition".to_string(), Value::Map(condition));
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        doc.insert("statement".to_string(), Value::List(vec![Value::Map(stmt)]));

        let original = Value::Map(doc);
        let json = value_to_iam_policy_json(&original).expect("ok");
        assert!(json.contains("\"StringEquals\""));
        assert!(json.contains("\"aws:PrincipalOrgID\""));

        let roundtripped = iam_policy_json_to_value(&json).expect("ok");
        assert_eq!(original, roundtripped);
    }
}
