//! Hand-written service implementation for `aws.sqs.Queue`.
//!
//! SQS exposes every per-queue knob through
//! `Attributes: Map<QueueAttributeName, String>` on `CreateQueue` /
//! `GetQueueAttributes` / `SetQueueAttributes`. Schema-side we project
//! every `QueueAttributeName` variant as a flat top-level attribute
//! via `extra_attributes` (see `carina-codegen-aws/src/resource_defs.rs::
//! sqs_resources`); the map↔flat packing lives here.

use aws_sdk_sqs::types::QueueAttributeName;
use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_string_attr;
// IAM policy doc Struct↔JSON converters are borrowed via pub(crate)
// from services/iam/role.rs until aws#286 extracts them into a shared
// module. SQS's Policy and RedriveAllowPolicy are IAM-shaped.
use crate::services::iam::role::{json_to_policy_doc, policy_doc_to_json};

/// Classification of how a `QueueAttributeName` value moves between
/// the DSL `Value` layer and the SQS wire format (strings).
#[derive(Clone, Copy)]
enum AttrKind {
    /// Stored / surfaced as integer; serialized to/from string with
    /// `i64::to_string` / `str::parse::<i64>`.
    Int,
    /// Stored / surfaced as bool; serialized to/from string with
    /// `"true"` / `"false"` (the SQS conventions).
    Bool,
    /// Stored / surfaced as String; serialized verbatim.
    String,
    /// Stored as a Struct shaped like an IAM policy document. Serialized
    /// to / from a JSON string via the IAM policy converters.
    IamPolicy,
    /// Stored as a Struct shaped like `{deadLetterTargetArn,
    /// maxReceiveCount}`. Serialized to / from a JSON string by the
    /// local `redrive_policy_to_json` / `json_to_redrive_policy`
    /// helpers below.
    RedrivePolicyStruct,
    /// Stored as a Struct shaped like `{redrivePermission,
    /// sourceQueueArns?}`. Serialized to / from a JSON string by the
    /// local `redrive_allow_policy_to_json` /
    /// `json_to_redrive_allow_policy` helpers below.
    RedriveAllowPolicyStruct,
}

/// Every queue attribute Carina surfaces. The tuple is
/// (DSL snake-case name, SDK enum key, AttrKind, is_writable).
///
/// `is_writable=false` entries skip pack-on-create / pack-on-update
/// but participate in read-back.
const QUEUE_ATTRS: &[(&str, QueueAttributeName, AttrKind, bool)] = &[
    // Writable: policies and JSON-shaped settings
    (
        "policy",
        QueueAttributeName::Policy,
        AttrKind::IamPolicy,
        true,
    ),
    (
        "redrive_policy",
        QueueAttributeName::RedrivePolicy,
        AttrKind::RedrivePolicyStruct,
        true,
    ),
    (
        "redrive_allow_policy",
        QueueAttributeName::RedriveAllowPolicy,
        AttrKind::RedriveAllowPolicyStruct,
        true,
    ),
    // Writable: numeric knobs
    (
        "visibility_timeout",
        QueueAttributeName::VisibilityTimeout,
        AttrKind::Int,
        true,
    ),
    (
        "message_retention_period",
        QueueAttributeName::MessageRetentionPeriod,
        AttrKind::Int,
        true,
    ),
    (
        "delay_seconds",
        QueueAttributeName::DelaySeconds,
        AttrKind::Int,
        true,
    ),
    (
        "maximum_message_size",
        QueueAttributeName::MaximumMessageSize,
        AttrKind::Int,
        true,
    ),
    (
        "receive_message_wait_time_seconds",
        QueueAttributeName::ReceiveMessageWaitTimeSeconds,
        AttrKind::Int,
        true,
    ),
    (
        "kms_data_key_reuse_period_seconds",
        QueueAttributeName::KmsDataKeyReusePeriodSeconds,
        AttrKind::Int,
        true,
    ),
    // Writable: FIFO + dedup
    (
        "fifo_queue",
        QueueAttributeName::FifoQueue,
        AttrKind::Bool,
        true,
    ),
    (
        "content_based_deduplication",
        QueueAttributeName::ContentBasedDeduplication,
        AttrKind::Bool,
        true,
    ),
    (
        "deduplication_scope",
        QueueAttributeName::DeduplicationScope,
        AttrKind::String,
        true,
    ),
    (
        "fifo_throughput_limit",
        QueueAttributeName::FifoThroughputLimit,
        AttrKind::String,
        true,
    ),
    // Writable: KMS encryption
    (
        "kms_master_key_id",
        QueueAttributeName::KmsMasterKeyId,
        AttrKind::String,
        true,
    ),
    (
        "sqs_managed_sse_enabled",
        QueueAttributeName::SqsManagedSseEnabled,
        AttrKind::Bool,
        true,
    ),
    // Read-only: ARN + runtime metrics + timestamps
    (
        "queue_arn",
        QueueAttributeName::QueueArn,
        AttrKind::String,
        false,
    ),
    (
        "approximate_number_of_messages",
        QueueAttributeName::ApproximateNumberOfMessages,
        AttrKind::Int,
        false,
    ),
    (
        "approximate_number_of_messages_delayed",
        QueueAttributeName::ApproximateNumberOfMessagesDelayed,
        AttrKind::Int,
        false,
    ),
    (
        "approximate_number_of_messages_not_visible",
        QueueAttributeName::ApproximateNumberOfMessagesNotVisible,
        AttrKind::Int,
        false,
    ),
    (
        "created_timestamp",
        QueueAttributeName::CreatedTimestamp,
        AttrKind::Int,
        false,
    ),
    (
        "last_modified_timestamp",
        QueueAttributeName::LastModifiedTimestamp,
        AttrKind::Int,
        false,
    ),
];

/// Attributes that AWS rejects in `SetQueueAttributes` (must be set at
/// CreateQueue time and are immutable afterwards). DSL-side
/// `create_only_overrides` already protects users from emitting an
/// update plan for these; this list mirrors that decision so the
/// update path drops them too.
fn is_create_only(name: &QueueAttributeName) -> bool {
    matches!(
        name,
        QueueAttributeName::FifoQueue
            | QueueAttributeName::FifoThroughputLimit
            | QueueAttributeName::DeduplicationScope
    )
}

/// Read `resource[attr_name]` and serialise it into the SQS wire
/// string for `kind`. Returns `None` when the attribute is not set or
/// the value is the wrong shape.
fn pack_attr(value: &Value, kind: AttrKind) -> Option<String> {
    match (kind, value) {
        (AttrKind::Int, Value::Concrete(ConcreteValue::Int(n))) => Some(n.to_string()),
        (AttrKind::Bool, Value::Concrete(ConcreteValue::Bool(b))) => {
            Some(if *b { "true" } else { "false" }.to_string())
        }
        (AttrKind::String, Value::Concrete(ConcreteValue::String(s))) => Some(s.clone()),
        (AttrKind::IamPolicy, Value::Concrete(ConcreteValue::Map(_))) => {
            let json = policy_doc_to_json(value);
            serde_json::to_string(&json).ok()
        }
        (AttrKind::RedrivePolicyStruct, Value::Concrete(ConcreteValue::Map(_))) => {
            redrive_policy_to_json_string(value)
        }
        (AttrKind::RedriveAllowPolicyStruct, Value::Concrete(ConcreteValue::Map(_))) => {
            redrive_allow_policy_to_json_string(value)
        }
        _ => None,
    }
}

/// Reverse of `pack_attr`: deserialise the SQS wire string into a
/// `Value` of the kind we declared in `QUEUE_ATTRS`.
fn unpack_attr(raw: &str, kind: AttrKind) -> Option<Value> {
    match kind {
        AttrKind::Int => raw
            .parse::<i64>()
            .ok()
            .map(|n| Value::Concrete(ConcreteValue::Int(n))),
        AttrKind::Bool => match raw {
            "true" => Some(Value::Concrete(ConcreteValue::Bool(true))),
            "false" => Some(Value::Concrete(ConcreteValue::Bool(false))),
            _ => None,
        },
        AttrKind::String => Some(Value::Concrete(ConcreteValue::String(raw.to_string()))),
        AttrKind::IamPolicy => {
            let json: serde_json::Value = serde_json::from_str(raw).ok()?;
            Some(json_to_policy_doc(&json))
        }
        AttrKind::RedrivePolicyStruct => {
            let json: serde_json::Value = serde_json::from_str(raw).ok()?;
            json_to_redrive_policy(&json)
        }
        AttrKind::RedriveAllowPolicyStruct => {
            let json: serde_json::Value = serde_json::from_str(raw).ok()?;
            json_to_redrive_allow_policy(&json)
        }
    }
}

/// Serialise a `Value::Map { dead_letter_target_arn, max_receive_count }`
/// to the `{"deadLetterTargetArn": "...", "maxReceiveCount": N}` JSON
/// string SQS expects.
fn redrive_policy_to_json_string(value: &Value) -> Option<String> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return None;
    };
    let arn = match map.get("dead_letter_target_arn") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => return None,
    };
    let count = match map.get("max_receive_count") {
        Some(Value::Concrete(ConcreteValue::Int(n))) => *n,
        _ => return None,
    };
    let json = serde_json::json!({
        "deadLetterTargetArn": arn,
        "maxReceiveCount": count,
    });
    serde_json::to_string(&json).ok()
}

/// Parse the `{deadLetterTargetArn, maxReceiveCount}` JSON document
/// SQS returns into a `Value::Map` with the DSL-side snake_case keys.
fn json_to_redrive_policy(json: &serde_json::Value) -> Option<Value> {
    let obj = json.as_object()?;
    let arn = obj.get("deadLetterTargetArn")?.as_str()?;
    let count = obj.get("maxReceiveCount")?.as_i64()?;
    let mut out = IndexMap::new();
    out.insert(
        "dead_letter_target_arn".to_string(),
        Value::Concrete(ConcreteValue::String(arn.to_string())),
    );
    out.insert(
        "max_receive_count".to_string(),
        Value::Concrete(ConcreteValue::Int(count)),
    );
    Some(Value::Concrete(ConcreteValue::Map(out)))
}

/// Serialise a `Value::Map { redrive_permission, source_queue_arns? }`
/// to the `{"redrivePermission": "...", "sourceQueueArns": [...]}`
/// JSON string SQS expects.
fn redrive_allow_policy_to_json_string(value: &Value) -> Option<String> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return None;
    };
    let permission = match map.get("redrive_permission") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        Some(Value::Concrete(ConcreteValue::EnumIdentifier(s))) => {
            // Raw EnumIdentifier at wire-out means the host did not
            // canonicalize it; pass it through so AWS-side rejection is
            // visible instead of silently mis-splitting.
            s.as_str().to_string()
        }
        Some(Value::Concrete(ConcreteValue::CanonicalEnum(c))) => c.api_value().to_string(),
        _ => return None,
    };
    let mut obj = serde_json::Map::new();
    obj.insert(
        "redrivePermission".to_string(),
        serde_json::Value::String(permission),
    );
    if let Some(Value::Concrete(ConcreteValue::List(items))) = map.get("source_queue_arns") {
        let arns: Vec<serde_json::Value> = items
            .iter()
            .filter_map(|v| match v {
                Value::Concrete(ConcreteValue::String(s)) => {
                    Some(serde_json::Value::String(s.clone()))
                }
                _ => None,
            })
            .collect();
        if !arns.is_empty() {
            obj.insert(
                "sourceQueueArns".to_string(),
                serde_json::Value::Array(arns),
            );
        }
    }
    serde_json::to_string(&serde_json::Value::Object(obj)).ok()
}

/// Parse the `{redrivePermission, sourceQueueArns?}` JSON document
/// SQS returns into a `Value::Map` with the DSL-side snake_case keys.
fn json_to_redrive_allow_policy(json: &serde_json::Value) -> Option<Value> {
    let obj = json.as_object()?;
    let permission_raw = obj.get("redrivePermission")?.as_str()?;
    // Emit the raw AWS-canonical value (`"allowAll"`) directly as a
    // plain `String`. The alias↔canonical reconciliation against the
    // parsed-desired side is owned by carina-core (the saved-state
    // `lift_string_enum_leaves` lift + the differ's spelling-agnostic
    // `StringEnum` arm), not by this read path emitting a
    // pre-down-converted alias (aws#326).
    let mut out = IndexMap::new();
    out.insert(
        "redrive_permission".to_string(),
        Value::Concrete(ConcreteValue::String(permission_raw.to_string())),
    );
    if let Some(arns_json) = obj.get("sourceQueueArns")
        && let Some(arr) = arns_json.as_array()
    {
        let arns: Vec<Value> = arr
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string())))
            })
            .collect();
        out.insert(
            "source_queue_arns".to_string(),
            Value::Concrete(ConcreteValue::List(arns)),
        );
    }
    Some(Value::Concrete(ConcreteValue::Map(out)))
}

impl AwsProvider {
    /// Read an SQS Queue by its URL (the provider identifier).
    pub(crate) async fn read_sqs_queue(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(queue_url) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        // Use the `All` meta-name so SQS returns every queue attribute
        // applicable to this queue (standard queues reject explicit
        // FIFO-only attribute names like `DeduplicationScope`).
        let attr_names: Vec<QueueAttributeName> = vec![QueueAttributeName::All];

        let result = match self
            .sqs_client
            .get_queue_attributes()
            .queue_url(queue_url)
            .set_attribute_names(Some(attr_names))
            .send()
            .await
        {
            Ok(out) => out,
            Err(err) => {
                if let Some(svc_err) = err.as_service_error()
                    && svc_err.is_queue_does_not_exist()
                {
                    return Ok(State::not_found(id.clone()));
                }
                return Err(api_error_with_meta(
                    "Failed to get queue attributes",
                    "sqs.GetQueueAttributes",
                    err,
                )
                .for_resource(id.clone()));
            }
        };

        let mut attributes: HashMap<String, Value> = HashMap::new();

        // Re-derive queue_name from the URL: SQS URLs end with `/{queue_name}`.
        if let Some(name) = queue_url.rsplit('/').next() {
            attributes.insert(
                "queue_name".to_string(),
                Value::Concrete(ConcreteValue::String(name.to_string())),
            );
        }

        if let Some(map) = result.attributes() {
            for (attr_name, sdk_key, kind, _) in QUEUE_ATTRS {
                if let Some(raw) = map.get(sdk_key)
                    && let Some(value) = unpack_attr(raw, *kind)
                {
                    attributes.insert((*attr_name).to_string(), value);
                }
            }
        }

        // Tags come from a separate API.
        match self
            .sqs_client
            .list_queue_tags()
            .queue_url(queue_url)
            .send()
            .await
        {
            Ok(tags_output) => {
                if let Some(tags) = tags_output.tags()
                    && !tags.is_empty()
                {
                    let mut tag_map = IndexMap::new();
                    for (key, val) in tags {
                        tag_map.insert(
                            key.to_string(),
                            Value::Concrete(ConcreteValue::String(val.to_string())),
                        );
                    }
                    attributes.insert(
                        "tags".to_string(),
                        Value::Concrete(ConcreteValue::Map(tag_map)),
                    );
                }
            }
            Err(_) => {
                // List-tags may fail with throttling or transient errors;
                // skip rather than fail the whole read. Same pattern as
                // logs.LogGroup.
            }
        }

        let state = State::existing(id.clone(), attributes);
        Ok(state.with_identifier(queue_url.to_string()))
    }

    /// Create an SQS Queue and return its post-create state.
    pub(crate) async fn create_sqs_queue(
        &self,
        resource: &Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let queue_name = require_string_attr(resource, "queue_name")?;

        // Pack every writable attribute the user actually set.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key, kind, writable) in QUEUE_ATTRS {
            if !writable {
                continue;
            }
            if let Some(value) = resource.get_attr(attr_name)
                && let Some(serialized) = pack_attr(value, *kind)
            {
                attr_map.insert(sdk_key.clone(), serialized);
            }
        }

        let tags = resource_tags_map(resource);

        let mut req = self.sqs_client.create_queue().queue_name(&queue_name);
        if !attr_map.is_empty() {
            req = req.set_attributes(Some(attr_map));
        }
        if let Some(tags) = tags {
            req = req.set_tags(Some(tags));
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create SQS queue", "sqs.CreateQueue", e)
                .for_resource(resource.id.clone())
        })?;

        let queue_url = result.queue_url().ok_or_else(|| {
            ProviderError::api_error("CreateQueue returned no QueueUrl")
                .for_resource(resource.id.clone())
        })?;

        self.read_sqs_queue(&resource.id, Some(queue_url)).await
    }

    /// Update an existing SQS Queue.
    pub(crate) async fn update_sqs_queue(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        // Build a partial map of attributes whose value changed.
        // Create-only attributes (FifoQueue / FifoThroughputLimit /
        // DeduplicationScope) are dropped — schema marks them
        // create_only, but defend in depth.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key, kind, writable) in QUEUE_ATTRS {
            if !writable || is_create_only(sdk_key) {
                continue;
            }
            let desired = to.get_attr(attr_name);
            let current = from.attributes.get(*attr_name);
            if desired == current {
                continue;
            }
            if let Some(value) = desired
                && let Some(serialized) = pack_attr(value, *kind)
            {
                attr_map.insert(sdk_key.clone(), serialized);
            }
        }

        if !attr_map.is_empty() {
            self.sqs_client
                .set_queue_attributes()
                .queue_url(identifier)
                .set_attributes(Some(attr_map))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "Failed to set queue attributes",
                        "sqs.SetQueueAttributes",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }

        self.apply_sqs_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        self.read_sqs_queue(&id, Some(identifier)).await
    }

    /// Delete an SQS Queue.
    pub(crate) async fn delete_sqs_queue(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.sqs_client
            .delete_queue()
            .queue_url(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete SQS queue", "sqs.DeleteQueue", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Apply tag diff between desired and current state.
    async fn apply_sqs_tags(
        &self,
        id: &ResourceId,
        queue_url: &str,
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

        let keys_to_remove: Vec<String> = current_tags
            .keys()
            .filter(|k| !desired_tags.contains_key(*k))
            .cloned()
            .collect();

        if !keys_to_remove.is_empty() {
            self.sqs_client
                .untag_queue()
                .queue_url(queue_url)
                .set_tag_keys(Some(keys_to_remove))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to untag SQS queue", "sqs.UntagQueue", e)
                        .for_resource(id.clone())
                })?;
        }

        let mut tags_to_add: HashMap<String, String> = HashMap::new();
        for (key, value) in &desired_tags {
            if let Value::Concrete(ConcreteValue::String(val)) = value {
                let should_add = match current_tags.get(key) {
                    Some(Value::Concrete(ConcreteValue::String(current_val))) => current_val != val,
                    _ => true,
                };
                if should_add {
                    tags_to_add.insert(key.clone(), val.clone());
                }
            }
        }

        if !tags_to_add.is_empty() {
            self.sqs_client
                .tag_queue()
                .queue_url(queue_url)
                .set_tags(Some(tags_to_add))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to tag SQS queue", "sqs.TagQueue", e)
                        .for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}

/// Extract a `tags = { ... }` block from the resource, dropping any
/// non-string values. Returns `None` when no tags are set.
fn resource_tags_map(resource: &Resource) -> Option<HashMap<String, String>> {
    let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") else {
        return None;
    };
    let mut out = HashMap::new();
    for (key, value) in tag_map {
        if let Value::Concrete(ConcreteValue::String(val)) = value {
            out.insert(key.clone(), val.clone());
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redrive_allow_policy_with_permission(permission: Value) -> Value {
        let mut map = IndexMap::new();
        map.insert("redrive_permission".to_string(), permission);
        Value::Concrete(ConcreteValue::Map(map))
    }

    #[test]
    fn redrive_allow_policy_serializes_canonical_enum_api_value() {
        let mut map = IndexMap::new();
        map.insert(
            "redrive_permission".to_string(),
            Value::Concrete(ConcreteValue::enum_identifier("allowAll")),
        );
        let schema =
            carina_core::schema::Schema::flat(carina_aws_types::sqs_redrive_allow_policy());
        let canonical = schema.canonicalize(Value::Concrete(ConcreteValue::Map(map)));

        let json = redrive_allow_policy_to_json_string(&canonical).expect("redrive allow JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["redrivePermission"].as_str(), Some("allowAll"));
    }

    #[test]
    fn redrive_allow_policy_raw_enum_identifier_passes_through_verbatim() {
        let raw = "aws.sqs.Queue.RedriveAllowPolicy.RedrivePermission.allowAll";
        let value = redrive_allow_policy_with_permission(Value::Concrete(
            ConcreteValue::enum_identifier(raw),
        ));

        let json = redrive_allow_policy_to_json_string(&value).expect("redrive allow JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["redrivePermission"].as_str(), Some(raw));
    }
}
