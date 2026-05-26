use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
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
                            tag_map.insert(
                                key.to_string(),
                                Value::Concrete(ConcreteValue::String(val.to_string())),
                            );
                        }
                        if !tag_map.is_empty() {
                            attributes.insert(
                                "tags".to_string(),
                                Value::Concrete(ConcreteValue::Map(tag_map)),
                            );
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
                    api_error_with_meta("Failed to get IAM role", "iam.GetRole", e)
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

        if let Some(Value::Concrete(ConcreteValue::String(desc))) = resource.get_attr("description")
        {
            req = req.description(desc);
        }

        if let Some(Value::Concrete(ConcreteValue::String(path))) = resource.get_attr("path") {
            req = req.path(path);
        }

        if let Some(Value::Concrete(ConcreteValue::Int(duration))) =
            resource.get_attr("max_session_duration")
        {
            req = req.max_session_duration(*duration as i32);
        }

        // Apply tags at creation time
        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") {
            for (key, value) in tag_map {
                if let Value::Concrete(ConcreteValue::String(val)) = value {
                    let tag = aws_sdk_iam::types::Tag::builder()
                        .key(key)
                        .value(val)
                        .build()
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message("Failed to build tag", &e))
                                .for_resource(resource.id.clone())
                        })?;
                    req = req.tags(tag);
                }
            }
        }

        req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create IAM role", "iam.CreateRole", e)
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
                    api_error_with_meta(
                        "Failed to update assume role policy",
                        "iam.UpdateAssumeRolePolicy",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        // Update description and max_session_duration via update_role
        let mut needs_update = false;
        let mut req = self.iam_client.update_role().role_name(identifier);

        if let Some(Value::Concrete(ConcreteValue::String(desc))) = to.get_attr("description") {
            req = req.description(desc);
            needs_update = true;
        }

        if let Some(Value::Concrete(ConcreteValue::Int(duration))) =
            to.get_attr("max_session_duration")
        {
            req = req.max_session_duration(*duration as i32);
            needs_update = true;
        }

        if needs_update {
            req.send().await.map_err(|e| {
                api_error_with_meta("Failed to update IAM role", "iam.UpdateRole", e)
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
                api_error_with_meta("Failed to delete IAM role", "iam.DeleteRole", e)
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
            Some(Value::Concrete(ConcreteValue::Map(m))) => m.clone(),
            _ => IndexMap::new(),
        };
        let current_tags = match current.and_then(|c| c.get("tags")) {
            Some(Value::Concrete(ConcreteValue::Map(m))) => m.clone(),
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
                api_error_with_meta("Failed to untag IAM role", "iam.UntagRole", e)
                    .for_resource(id.clone())
            })?;
        }

        // Tags to add/update
        let mut tags_to_add = Vec::new();
        for (key, value) in &desired_tags {
            if let Value::Concrete(ConcreteValue::String(val)) = value {
                let should_add = match current_tags.get(key) {
                    Some(Value::Concrete(ConcreteValue::String(current_val))) => current_val != val,
                    _ => true,
                };
                if should_add {
                    let tag = aws_sdk_iam::types::Tag::builder()
                        .key(key)
                        .value(val)
                        .build()
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message("Failed to build tag", &e))
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
                api_error_with_meta("Failed to tag IAM role", "iam.TagRole", e)
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
            attributes.insert(
                "arn".to_string(),
                Value::Concrete(ConcreteValue::String(arn.to_string())),
            );
        }
        if let Some(v) = obj.assume_role_policy_document() {
            // The SDK URL-encodes the policy document.
            let decoded = urlencoding::decode(v).unwrap_or_else(|_| v.into());
            // Convert JSON string to Value::Map with snake_case keys for
            // struct comparison; fall back to the raw string if the
            // policy is malformed.
            let policy_value = iam_policy_json_to_value(&decoded)
                .unwrap_or_else(|_| Value::Concrete(ConcreteValue::String(decoded.into_owned())));
            attributes.insert("assume_role_policy_document".to_string(), policy_value);
        }
        if let Some(v) = obj.description() {
            attributes.insert(
                "description".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.max_session_duration() {
            attributes.insert(
                "max_session_duration".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        let path = obj.path();
        if !path.is_empty() {
            attributes.insert(
                "path".to_string(),
                Value::Concrete(ConcreteValue::String(path.to_string())),
            );
        }
        let role_id = obj.role_id();
        if !role_id.is_empty() {
            attributes.insert(
                "role_id".to_string(),
                Value::Concrete(ConcreteValue::String(role_id.to_string())),
            );
        }
        let role_name = obj.role_name();
        if !role_name.is_empty() {
            attributes.insert(
                "role_name".to_string(),
                Value::Concrete(ConcreteValue::String(role_name.to_string())),
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
    let json_value =
        dsl_value_to_iam_json(value, &carina_aws_types::iam_policy_document(), "policy");
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
        Some(Value::Concrete(ConcreteValue::String(s))) => Ok(s.clone()),
        Some(value @ Value::Concrete(ConcreteValue::Map(_))) => value_to_iam_policy_json(value)
            .map_err(|e| {
                ProviderError::invalid_input(format!("Failed to convert {}: {}", attr_name, e))
                    .for_resource(resource.id.clone())
            }),
        _ => Err(ProviderError::invalid_input(format!(
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

fn lookup_snake(
    table: &'static [(&'static str, &'static str)],
    pascal: &str,
) -> Option<&'static str> {
    table.iter().find_map(|(s, p)| (*p == pascal).then_some(*s))
}

/// Convert a Carina IAM policy document `Value` to JSON, driven by the
/// `iam_policy_document()` schema.
///
/// This is the apply-path serializer (reached via `resolve_iam_policy_attr`
/// from `s3.BucketPolicy` / `iam.Role` / `sqs.Queue`). Being schema-typed
/// is the root fix for aws#315: `StringEnum` leaves (`version`, `effect`)
/// are canonicalized to the AWS wire form here via `DslMap::api_for`,
/// regardless of whether the upstream plan-time normalizer ran. The
/// previous hand-written serializer was schema-blind and depended on the
/// normalizer having canonicalized enums first — which the apply path
/// does not do (carina#3060), so the DSL spelling reached AWS and was
/// rejected with `MalformedPolicy`.
///
/// Mirrors awscc's `dsl_value_to_aws`. Field key casing comes from the
/// schema's `with_provider_name(...)`; condition operator keys are still
/// translated positionally (`attr_name == "condition"`) to match awscc.
pub(crate) fn policy_doc_to_json(value: &Value) -> serde_json::Value {
    dsl_value_to_iam_json(value, &carina_aws_types::iam_policy_document(), "policy")
}

/// Schema-type-driven Value → IAM-policy JSON walk. `attr_name` carries
/// the current DSL field name so the condition-operator key translation
/// (which is positional, not schema-expressible without a key-typed map
/// rewrite) can fire on `condition`, matching awscc's behavior.
fn dsl_value_to_iam_json(
    value: &Value,
    attr_type: &carina_core::schema::AttributeType,
    attr_name: &str,
) -> serde_json::Value {
    use carina_core::schema::AttributeType;

    // StringEnum leaf: canonicalize the DSL spelling to the AWS wire
    // form. Accepts `String` and `EnumIdentifier` (same text payload;
    // the WIT boundary collapses `EnumIdentifier` to `String` anyway).
    if let Some((_, values, _, dsl_map)) = attr_type.string_enum_parts() {
        match value {
            Value::Concrete(ConcreteValue::String(s))
            | Value::Concrete(ConcreteValue::EnumIdentifier(s)) => {
                let valid: Vec<&str> = values.iter().map(String::as_str).collect();
                let trailing = carina_core::utils::extract_enum_value_with_values(s, &valid);
                return serde_json::Value::String(dsl_map.api_for(trailing));
            }
            _ => return scalar_to_json(value),
        }
    }

    match attr_type {
        AttributeType::Union(members) => {
            // Try each member; first whose *input* value-shape matches
            // wins. Gating on the input shape (not the output JSON)
            // avoids an empty Struct member ({} from a Map whose keys
            // are all unknown) shadowing the String member. Struct must
            // precede String in the schema (see
            // `string_or_principal_struct`) so a Map principal is not
            // matched against the String arm.
            for member in members {
                let value_matches_member = match member {
                    AttributeType::Struct { .. } => {
                        matches!(value, Value::Concrete(ConcreteValue::Map(_)))
                    }
                    AttributeType::List { .. } => {
                        matches!(value, Value::Concrete(ConcreteValue::List(_)))
                    }
                    _ => true,
                };
                if value_matches_member {
                    return dsl_value_to_iam_json(value, member, attr_name);
                }
            }
            scalar_to_json(value)
        }
        AttributeType::List { inner, .. } => match value {
            Value::Concrete(ConcreteValue::List(items)) => serde_json::Value::Array(
                items
                    .iter()
                    .map(|it| dsl_value_to_iam_json(it, inner, attr_name))
                    .collect(),
            ),
            // A scalar where the schema declares a list: emit the scalar
            // (AWS accepts a bare string for single-element Action etc.).
            _ => dsl_value_to_iam_json(value, inner, attr_name),
        },
        AttributeType::Struct { fields, .. } => {
            // Block syntax materializes a single-element List<Map>.
            let map = match value {
                Value::Concrete(ConcreteValue::Map(m)) => m,
                Value::Concrete(ConcreteValue::List(items)) if items.len() == 1 => {
                    match &items[0] {
                        Value::Concrete(ConcreteValue::Map(m)) => m,
                        _ => return scalar_to_json(value),
                    }
                }
                _ => return scalar_to_json(value),
            };
            // The schema is authoritative for IAM policy documents: the
            // grammar has no extension keys, so a map key not modeled in
            // `iam_policy_document()` is a user typo (`statment`) that
            // AWS would reject anyway. Emitting only schema fields (and
            // dropping unmodeled keys) matches awscc's `dsl_value_to_aws`
            // and is the deliberate behavior — the prior hand-written
            // serializer's verbatim passthrough would instead forward
            // the malformed key to AWS.
            let mut obj = serde_json::Map::new();
            for field in fields {
                let Some(fv) = map.get(&field.name) else {
                    continue;
                };
                let key = field
                    .provider_name
                    .as_deref()
                    .unwrap_or(&field.name)
                    .to_string();
                obj.insert(
                    key,
                    dsl_value_to_iam_json(fv, &field.field_type, &field.name),
                );
            }
            serde_json::Value::Object(obj)
        }
        AttributeType::Map { value: inner, .. } => {
            let Value::Concrete(ConcreteValue::Map(map)) = value else {
                return scalar_to_json(value);
            };
            // IAM condition is `Map<Op, Map<Var, StringOrList>>`, so this
            // arm fires twice with `attr_name` still "condition": once
            // for operator keys, once for the inner variable keys. The
            // operator table (`condition_operator_to_aws`) PascalCases
            // operators; for variable keys (`aws:SecureTransport`) the
            // table lookup misses and `unwrap_or_else` keeps them
            // verbatim — a condition variable can never alias an
            // operator name (operators are bare lowercase, never contain
            // `:`), so the inner pass is a safe no-op. The schema types
            // the operator key as a StringEnum, but the Map walk does
            // not consult the key type, so the positional
            // `attr_name == "condition"` switch mirrors awscc's
            // `dsl_value_to_aws`.
            let is_condition = attr_name == "condition";
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = if is_condition {
                    carina_aws_types::condition_operator_to_aws(k).unwrap_or_else(|| k.clone())
                } else {
                    k.clone()
                };
                obj.insert(key, dsl_value_to_iam_json(v, inner, attr_name));
            }
            serde_json::Value::Object(obj)
        }
        _ => scalar_to_json(value),
    }
}

/// Terminal scalar conversion. No enum handling here — `StringEnum`
/// leaves are canonicalized in `dsl_value_to_iam_json` before reaching
/// this point. An `EnumIdentifier` only lands here for a non-schema
/// position (none exist in `iam_policy_document()` today); strip its
/// namespace so a stray value still serializes as a plain string.
fn scalar_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Concrete(ConcreteValue::String(s)) => serde_json::Value::String(s.clone()),
        Value::Concrete(ConcreteValue::Int(n)) => serde_json::Value::Number((*n).into()),
        Value::Concrete(ConcreteValue::Float(f)) => serde_json::json!(*f),
        Value::Concrete(ConcreteValue::Bool(b)) => serde_json::Value::Bool(*b),
        Value::Concrete(ConcreteValue::EnumIdentifier(id)) => {
            serde_json::Value::String(id.rsplit('.').next().unwrap_or(id.as_str()).to_string())
        }
        Value::Concrete(ConcreteValue::List(items)) => {
            serde_json::Value::Array(items.iter().map(scalar_to_json).collect())
        }
        Value::Concrete(ConcreteValue::Map(map)) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), scalar_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
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

pub(crate) fn json_to_policy_doc(json: &serde_json::Value) -> Value {
    let serde_json::Value::Object(obj) = json else {
        return json_scalar_to_value(json);
    };
    let mut map = IndexMap::new();
    for (k, v) in obj {
        if v.is_null() {
            continue;
        }
        let snake_key = lookup_snake(POLICY_TOP_FIELDS, k).unwrap_or(k.as_str());
        let value = match snake_key {
            "statement" => match v {
                serde_json::Value::Array(items) => Value::Concrete(ConcreteValue::List(
                    items.iter().map(json_to_policy_statement).collect(),
                )),
                serde_json::Value::Object(_) => json_to_policy_statement(v),
                _ => json_scalar_to_value(v),
            },
            // `version` is a StringEnum but needs no special arm: emit
            // the raw AWS-canonical value (`"2012-10-17"`) directly as
            // a plain `String` via the catch-all. The alias↔canonical
            // reconciliation against the parsed-desired side is owned
            // by carina-core (the saved-state `lift_string_enum_leaves`
            // lift + the differ's spelling-agnostic `StringEnum` arm),
            // not by this read path emitting a pre-down-converted alias
            // (aws#326).
            _ => json_scalar_or_passthrough_to_value(v),
        };
        map.insert(snake_key.to_string(), value);
    }
    Value::Concrete(ConcreteValue::Map(map))
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
            // `effect` is a StringEnum. Emit the raw AWS-canonical
            // value (`"Allow"`) directly as a plain `String` via the
            // catch-all; carina-core's saved-state lift + the differ's
            // spelling-agnostic `StringEnum` arm reconcile it against
            // the parsed-desired alias side (aws#326). No special arm
            // needed.
            // `Action` / `Resource` are `Map<String, StringOrList>`
            // — AWS may return a scalar (`"sts:AssumeRole"`) or a list.
            // The DSL schema accepts both, but to avoid post-apply
            // plan-verify churn we normalize scalars to a single-element
            // list (the typical DSL spelling).
            "action" | "not_action" | "resource" | "not_resource" => {
                json_scalar_to_singleton_list(v)
            }
            _ => json_scalar_or_passthrough_to_value(v),
        };
        map.insert(snake_key.to_string(), value);
    }
    Value::Concrete(ConcreteValue::Map(map))
}

/// AWS returns `Principal: {"AWS": "*"}` (scalar) for single-element
/// principals; the DSL schema declares `aws: List<String>`. Wrap the
/// scalar into a single-element list so post-apply plan-verify does
/// not see a shape mismatch.
fn json_scalar_to_singleton_list(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Array(items) => Value::Concrete(ConcreteValue::List(
            items.iter().map(json_scalar_to_value).collect(),
        )),
        _ => Value::Concrete(ConcreteValue::List(vec![json_scalar_to_value(v)])),
    }
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
                // All Principal sub-fields (`aws`, `service`,
                // `federated`, `canonical_user`) are declared as
                // `List<String>` in the schema; normalize scalar JSON
                // (which AWS returns when there is only one entry)
                // into a singleton list.
                map.insert(key.to_string(), json_scalar_to_singleton_list(v));
            }
            Value::Concrete(ConcreteValue::Map(map))
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
                Value::Concrete(ConcreteValue::Map(m))
            }
            _ => json_scalar_to_value(kv_value),
        };
        map.insert(op_snake, kv);
    }
    Value::Concrete(ConcreteValue::Map(map))
}

fn json_scalar_or_passthrough_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Array(items) => Value::Concrete(ConcreteValue::List(
            items
                .iter()
                .map(json_scalar_or_passthrough_to_value)
                .collect(),
        )),
        serde_json::Value::Object(obj) => {
            let mut map = IndexMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_scalar_or_passthrough_to_value(v));
            }
            Value::Concrete(ConcreteValue::Map(map))
        }
        _ => json_scalar_to_value(json),
    }
}

fn json_scalar_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::String(s) => Value::Concrete(ConcreteValue::String(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Concrete(ConcreteValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Value::Concrete(ConcreteValue::Float(f))
            } else {
                Value::Concrete(ConcreteValue::String(n.to_string()))
            }
        }
        serde_json::Value::Bool(b) => Value::Concrete(ConcreteValue::Bool(*b)),
        // JSON null in policy documents is uncommon and has no faithful
        // Carina counterpart (Value has no Null variant). Map to empty
        // string only as a last resort; callers should normally treat
        // null fields as absent.
        serde_json::Value::Null => Value::Concrete(ConcreteValue::String(String::new())),
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
        let r = make_resource(Some(Value::Concrete(ConcreteValue::String(
            json.to_string(),
        ))));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("string passthrough");
        assert_eq!(resolved, json);
    }

    #[test]
    fn resolve_iam_policy_attr_serializes_map_to_pascal_case_json() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
        );
        let mut stmt = IndexMap::new();
        // `effect` is a StringEnum. The read path now emits raw
        // AWS-canonical `String` (aws#326), but the *serializer* must
        // still accept `EnumIdentifier` because carina-core's
        // saved-state lift produces `EnumIdentifier("allow")` for the
        // desired-equivalent state; `scalar_to_json` maps it back to
        // the PascalCase wire form (`"Allow"`). This test pins that
        // continued acceptance.
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("Allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::String("s3:GetObject".to_string())),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let r = make_resource(Some(Value::Concrete(ConcreteValue::Map(policy))));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("map → JSON");

        assert!(resolved.contains("\"Version\""));
        assert!(resolved.contains("\"Statement\""));
        assert!(resolved.contains("\"Effect\""));
        assert!(resolved.contains("\"Action\""));
        assert!(!resolved.contains("\"version\""));
    }

    /// aws#315 defensive fallback: the schema-typed AWS normalizer
    /// (`api_canonicalize_recursive`) is what actually canonicalizes
    /// `version` / `effect` before this serializer runs — see the
    /// end-to-end regression in `normalizer.rs`. But if an
    /// `EnumIdentifier` reaches the serializer un-canonicalized (a future
    /// enum field not yet covered by the normalizer pass), the
    /// `scalar_to_json` fallback must still emit the AWS wire form so AWS
    /// does not reject the PUT with `MalformedPolicy`. Both the namespaced
    /// and the bare-alias `EnumIdentifier` shapes are covered here.
    #[test]
    fn resolve_iam_policy_attr_enum_identifier_fallback_is_aws_canonical() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier(
                "aws.iam.PolicyDocument.Version.2012_10_17".to_string(),
            )),
        );
        let mut stmt = IndexMap::new();
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::String("s3:GetObject".to_string())),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let r = make_resource(Some(Value::Concrete(ConcreteValue::Map(policy))));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("map → JSON");

        assert!(
            resolved.contains(r#""Version":"2012-10-17""#),
            "version must be AWS-canonical, got: {resolved}"
        );
        assert!(
            resolved.contains(r#""Effect":"Allow""#),
            "effect must be AWS-canonical, got: {resolved}"
        );
        assert!(
            !resolved.contains("2012_10_17") && !resolved.contains(r#""allow""#),
            "DSL spelling must not reach AWS, got: {resolved}"
        );
    }

    /// Real aws#315 reproduction (root cause: carina#3060). At apply
    /// time `resolve_resource_with_source` re-resolves the policy from
    /// the *un-normalized* source and the WIT boundary collapses
    /// `EnumIdentifier` to a plain `String`. So the serializer — the
    /// path that actually reaches AWS — receives `version` /
    /// `effect` as a `String` carrying the namespaced DSL spelling
    /// (`aws.iam.PolicyDocument.Version.2012_10_17`) or the bare DSL
    /// alias (`allow`), NOT an already-canonical value and NOT an
    /// `EnumIdentifier`. The schema-type-driven serializer must
    /// canonicalize these to the AWS wire form regardless of the
    /// upstream normalizer, so `aws.s3.BucketPolicy` apply stops
    /// failing with `MalformedPolicy`.
    #[test]
    fn resolve_iam_policy_attr_canonicalizes_unnormalized_dsl_string() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.iam.PolicyDocument.Version.2012_10_17".to_string(),
            )),
        );
        let mut stmt = IndexMap::new();
        stmt.insert(
            "sid".to_string(),
            Value::Concrete(ConcreteValue::String("AllowRead".to_string())),
        );
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::String("s3:GetObject".to_string())),
        );
        let mut principal = IndexMap::new();
        principal.insert(
            "canonical_user".to_string(),
            Value::Concrete(ConcreteValue::String("CANON123".to_string())),
        );
        stmt.insert(
            "principal".to_string(),
            Value::Concrete(ConcreteValue::Map(principal)),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let r = make_resource(Some(Value::Concrete(ConcreteValue::Map(policy))));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("map → JSON");

        assert!(
            resolved.contains(r#""Version":"2012-10-17""#),
            "version must be AWS-canonical, got: {resolved}"
        );
        assert!(
            resolved.contains(r#""Effect":"Allow""#),
            "effect must be AWS-canonical, got: {resolved}"
        );
        assert!(
            !resolved.contains("2012_10_17") && !resolved.contains(r#""allow""#),
            "DSL spelling must not reach AWS, got: {resolved}"
        );
        // Constraint A regression: canonical_user Principal must not be
        // silently dropped by the schema-driven walk.
        assert!(
            resolved.contains(r#""CanonicalUser":"CANON123""#),
            "canonical_user Principal must round-trip, got: {resolved}"
        );
    }

    /// Pins the deliberate schema-authoritative behavior (aws#317): a
    /// map key not modeled in `iam_policy_document()` is dropped, not
    /// passed through. The IAM policy grammar has no extension keys, so
    /// an unmodeled key is a user typo AWS would reject anyway; this
    /// matches awscc's `dsl_value_to_aws`. Every *modeled* key must
    /// still round-trip.
    #[test]
    fn resolve_iam_policy_attr_drops_unmodeled_keys_keeps_modeled() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String("2012-10-17".to_string())),
        );
        // Typo / unmodeled top-level key — must NOT reach AWS.
        policy.insert(
            "statment".to_string(),
            Value::Concrete(ConcreteValue::String("oops".to_string())),
        );
        let mut stmt = IndexMap::new();
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("Allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::String("s3:GetObject".to_string())),
        );
        stmt.insert(
            "resource".to_string(),
            Value::Concrete(ConcreteValue::String("arn:aws:s3:::b/*".to_string())),
        );
        // Unmodeled statement key — dropped.
        stmt.insert(
            "bogus_field".to_string(),
            Value::Concrete(ConcreteValue::String("x".to_string())),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let r = make_resource(Some(Value::Concrete(ConcreteValue::Map(policy))));
        let resolved = resolve_iam_policy_attr(&r, "policy").expect("map → JSON");

        // Modeled keys preserved.
        assert!(resolved.contains(r#""Version":"2012-10-17""#));
        assert!(resolved.contains(r#""Effect":"Allow""#));
        assert!(resolved.contains(r#""Action":"s3:GetObject""#));
        assert!(resolved.contains(r#""Resource":"arn:aws:s3:::b/*""#));
        // Unmodeled keys dropped (schema is authoritative).
        assert!(
            !resolved.contains("statment") && !resolved.contains("bogus_field"),
            "unmodeled keys must not reach AWS, got: {resolved}"
        );
    }

    #[test]
    fn resolve_iam_policy_attr_errors_when_missing() {
        let r = make_resource(None);
        let err = resolve_iam_policy_attr(&r, "policy").expect_err("missing");
        assert!(format!("{}", err).contains("policy is required"));
    }

    #[test]
    fn resolve_iam_policy_attr_errors_on_non_string_non_map() {
        let r = make_resource(Some(Value::Concrete(ConcreteValue::Bool(true))));
        let err = resolve_iam_policy_attr(&r, "policy").expect_err("invalid type");
        assert!(format!("{}", err).contains("policy is required"));
    }

    /// A full deny-insecure-transport policy mirroring the acceptance fixture.
    /// Exercises principal map, list-valued action/resource, and a
    /// `bool` condition with the `aws:SecureTransport` value key.
    fn full_deny_policy_value() -> Value {
        // Canonical post-read shape (aws#326):
        //   - `effect` / `version`: raw AWS-canonical `String`
        //     (`"Deny"` / `"2012-10-17"`) — the read path no longer
        //     down-converts to a DSL-alias `EnumIdentifier`
        //   - `principal.aws`: singleton List (schema declares List<String>)
        //   - `action`: singleton List (schema declares List<String>)
        // value_to_iam_policy_json serializes both List-of-one and bare
        // scalars to the same JSON, and json_to_policy_doc normalizes
        // scalars to singleton lists, so this is the round-trip fixed
        // point.
        let mut principal = IndexMap::new();
        principal.insert(
            "aws".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("*".to_string()),
            )])),
        );

        let mut bool_inner = IndexMap::new();
        bool_inner.insert(
            "aws:SecureTransport".to_string(),
            Value::Concrete(ConcreteValue::String("false".to_string())),
        );
        let mut condition = IndexMap::new();
        condition.insert(
            "bool".to_string(),
            Value::Concrete(ConcreteValue::Map(bool_inner)),
        );

        let mut stmt = IndexMap::new();
        stmt.insert(
            "sid".to_string(),
            Value::Concrete(ConcreteValue::String("DenyInsecureTransport".to_string())),
        );
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("Deny".to_string())),
        );
        stmt.insert(
            "principal".to_string(),
            Value::Concrete(ConcreteValue::Map(principal)),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("s3:*".to_string()),
            )])),
        );
        stmt.insert(
            "resource".to_string(),
            Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("arn:aws:s3:::my-bucket".to_string())),
                Value::Concrete(ConcreteValue::String(
                    "arn:aws:s3:::my-bucket/*".to_string(),
                )),
            ])),
        );
        stmt.insert(
            "condition".to_string(),
            Value::Concrete(ConcreteValue::Map(condition)),
        );

        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String("2012-10-17".to_string())),
        );
        doc.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );
        Value::Concrete(ConcreteValue::Map(doc))
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
        let Value::Concrete(ConcreteValue::Map(doc)) = &value else {
            panic!("expected map");
        };
        // Top-level snake_case.
        assert!(doc.contains_key("version"));
        assert!(doc.contains_key("statement"));

        // Drill into statement[0].condition.bool.
        let Value::Concrete(ConcreteValue::List(stmts)) = doc.get("statement").unwrap() else {
            panic!();
        };
        let Value::Concrete(ConcreteValue::Map(stmt)) = &stmts[0] else {
            panic!()
        };
        let Value::Concrete(ConcreteValue::Map(cond)) = stmt.get("condition").unwrap() else {
            panic!()
        };
        let Value::Concrete(ConcreteValue::Map(bool_inner)) = cond.get("bool").unwrap() else {
            panic!()
        };
        // CRITICAL: condition variable key preserved verbatim.
        assert!(bool_inner.contains_key("aws:SecureTransport"));
        assert!(!bool_inner.contains_key("aws:secure_transport"));

        // Principal {"AWS"} → snake "aws".
        let Value::Concrete(ConcreteValue::Map(principal)) = stmt.get("principal").unwrap() else {
            panic!()
        };
        assert!(principal.contains_key("aws"));
        // Schema declares principal sub-fields as List<String>; scalar
        // JSON (`"AWS": "*"`) is normalized to a singleton list on read
        // so post-apply plan-verify does not see a shape mismatch.
        assert_eq!(
            principal.get("aws"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("*".to_string()))
            ])))
        );

        // Effect is the raw AWS-canonical String on read (aws#326).
        assert_eq!(
            stmt.get("effect"),
            Some(&Value::Concrete(ConcreteValue::String("Deny".to_string())))
        );
        // Action scalar is wrapped to singleton list.
        assert_eq!(
            stmt.get("action"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("s3:*".to_string()))
            ])))
        );
    }

    #[test]
    fn id_field_round_trips() {
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String("2012-10-17".to_string())),
        );
        doc.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String("MyPolicyId".to_string())),
        );
        doc.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![])),
        );

        let original = Value::Concrete(ConcreteValue::Map(doc));
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
        let Value::Concrete(ConcreteValue::Map(doc)) = &value else {
            panic!()
        };
        let Value::Concrete(ConcreteValue::Map(stmt)) = doc.get("statement").unwrap() else {
            panic!("statement should be a Map for single-object form")
        };
        // Effect is the raw AWS-canonical String on read (aws#326).
        assert_eq!(
            stmt.get("effect"),
            Some(&Value::Concrete(ConcreteValue::String("Allow".to_string())))
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
        let Value::Concrete(ConcreteValue::Map(doc)) = &value else {
            panic!()
        };
        let Value::Concrete(ConcreteValue::List(stmts)) = doc.get("statement").unwrap() else {
            panic!()
        };
        let Value::Concrete(ConcreteValue::Map(stmt)) = &stmts[0] else {
            panic!()
        };
        assert_eq!(
            stmt.get("principal"),
            Some(&Value::Concrete(ConcreteValue::String("*".to_string()))),
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
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("Environment".to_string()),
            )])),
        );
        let mut condition = IndexMap::new();
        condition.insert(
            "for_all_values_string_equals_if_exists".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
        let mut stmt = IndexMap::new();
        // Canonical post-read form: effect raw AWS-canonical String,
        // action singleton List (aws#326).
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("Allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("s3:*".to_string()),
            )])),
        );
        stmt.insert(
            "condition".to_string(),
            Value::Concrete(ConcreteValue::Map(condition)),
        );
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String("2012-10-17".to_string())),
        );
        doc.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let original = Value::Concrete(ConcreteValue::Map(doc));
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
            Value::Concrete(ConcreteValue::String("o-1234567890".to_string())),
        );
        let mut condition = IndexMap::new();
        condition.insert(
            "string_equals".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
        let mut stmt = IndexMap::new();
        // Canonical post-read form: effect raw AWS-canonical String,
        // action singleton List (aws#326).
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::String("Allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("s3:GetObject".to_string()),
            )])),
        );
        stmt.insert(
            "condition".to_string(),
            Value::Concrete(ConcreteValue::Map(condition)),
        );
        let mut doc = IndexMap::new();
        doc.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::String("2012-10-17".to_string())),
        );
        doc.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let original = Value::Concrete(ConcreteValue::Map(doc));
        let json = value_to_iam_policy_json(&original).expect("ok");
        assert!(json.contains("\"StringEquals\""));
        assert!(json.contains("\"aws:PrincipalOrgID\""));

        let roundtripped = iam_policy_json_to_value(&json).expect("ok");
        assert_eq!(original, roundtripped);
    }

    /// aws#326 contract regression: the read path emits the raw
    /// AWS-canonical value as a plain `String` (no down-conversion to a
    /// DSL-alias `EnumIdentifier`). Pins (1) the new read shape and
    /// (2) that this `String` form serializes straight back to the
    /// identical AWS wire form — so the read→serialize round-trip needs
    /// no `json_*_to_enum_identifier` step. Reconciliation against the
    /// parsed-desired alias side is carina-core's job (saved-state
    /// `lift_string_enum_leaves` + the differ's spelling-agnostic
    /// `StringEnum` arm), not this read path's.
    #[test]
    fn read_path_emits_raw_canonical_string_and_round_trips() {
        let aws_json = r#"{
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Deny",
                "Action": "s3:*",
                "Resource": "arn:aws:s3:::b/*"
            }]
        }"#;
        let value = iam_policy_json_to_value(aws_json).expect("read ok");
        let Value::Concrete(ConcreteValue::Map(doc)) = &value else {
            panic!("expected policy map");
        };
        // (1) New read contract: raw AWS-canonical `String`, not a
        // down-converted `EnumIdentifier` alias.
        assert_eq!(
            doc.get("version"),
            Some(&Value::Concrete(ConcreteValue::String(
                "2012-10-17".to_string()
            ))),
            "version must be raw AWS-canonical String (aws#326)"
        );
        let Value::Concrete(ConcreteValue::List(stmts)) = doc.get("statement").unwrap() else {
            panic!("expected statement list");
        };
        let Value::Concrete(ConcreteValue::Map(s0)) = &stmts[0] else {
            panic!("expected statement[0] map");
        };
        assert_eq!(
            s0.get("effect"),
            Some(&Value::Concrete(ConcreteValue::String("Deny".to_string()))),
            "effect must be raw AWS-canonical String (aws#326)"
        );

        // (2) That String form serializes straight back to the AWS
        // wire form with no down-conversion step in between.
        let back = value_to_iam_policy_json(&value).expect("serialize ok");
        let back_json: serde_json::Value = serde_json::from_str(&back).expect("parse");
        assert_eq!(back_json["Version"], "2012-10-17");
        assert_eq!(back_json["Statement"][0]["Effect"], "Deny");
    }
}
