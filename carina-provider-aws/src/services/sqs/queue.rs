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

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::SqsQueueReadModel;
use crate::send_models_generated::{SqsQueueCreateInput, SqsQueueUpdateInput};
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
        (AttrKind::String, Value::Concrete(ConcreteValue::String(s))) => {
            Some(s.expose_plaintext().to_string())
        }
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

fn optional_bool_value(value: Option<bool>) -> Option<Value> {
    value.map(|value| Value::Concrete(ConcreteValue::Bool(value)))
}

fn optional_int_value(value: Option<i64>) -> Option<Value> {
    value.map(|value| Value::Concrete(ConcreteValue::Int(value)))
}

fn optional_string_value(value: Option<String>) -> Option<Value> {
    value.map(|value| Value::Concrete(ConcreteValue::String(value.into())))
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
        AttrKind::String => Some(Value::Concrete(ConcreteValue::String(
            raw.to_string().into(),
        ))),
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

fn queue_attribute(
    attributes: Option<&HashMap<QueueAttributeName, String>>,
    key: &QueueAttributeName,
    kind: AttrKind,
) -> Option<Value> {
    attributes
        .and_then(|attributes| attributes.get(key))
        .and_then(|raw| unpack_attr(raw, kind))
}

fn required_queue_attribute(
    id: &ResourceId,
    attributes: Option<&HashMap<QueueAttributeName, String>>,
    dsl_name: &str,
    key: &QueueAttributeName,
    kind: AttrKind,
) -> ProviderResult<Value> {
    queue_attribute(attributes, key, kind).ok_or_else(|| {
        ProviderError::api_error(format!(
            "GetQueueAttributes(All) omitted required SQS attribute {dsl_name}"
        ))
        .for_resource(id.clone())
    })
}

/// Reject cross-field combinations before they reach an AWS API that would
/// reject or silently ignore them.
pub(crate) fn validate_sqs_queue_inputs<S>(resource: &Resource<S>) -> ProviderResult<()> {
    if resource.id.provider != "aws" || resource.id.resource_type != "sqs.Queue" {
        return Ok(());
    }

    if matches!(
        resource.get_attr("kms_data_key_reuse_period_seconds"),
        Some(Value::Concrete(_))
    ) && resource.get_attr("kms_master_key_id").is_none()
    {
        return Err(ProviderError::invalid_input(
            "sqs.Queue attribute kms_data_key_reuse_period_seconds requires kms_master_key_id",
        )
        .for_resource(resource.id.clone()));
    }

    let fifo_is_definitively_standard = matches!(
        resource.get_attr("fifo_queue"),
        None | Some(Value::Concrete(ConcreteValue::Bool(false)))
    );
    if fifo_is_definitively_standard {
        let content_dedup_enabled = matches!(
            resource.get_attr("content_based_deduplication"),
            Some(Value::Concrete(ConcreteValue::Bool(true)))
        );
        let unsupported = if content_dedup_enabled {
            Some("content_based_deduplication")
        } else if matches!(
            resource.get_attr("deduplication_scope"),
            Some(Value::Concrete(_))
        ) {
            Some("deduplication_scope")
        } else if matches!(
            resource.get_attr("fifo_throughput_limit"),
            Some(Value::Concrete(_))
        ) {
            Some("fifo_throughput_limit")
        } else {
            None
        };
        if let Some(attribute) = unsupported {
            return Err(ProviderError::invalid_input(format!(
                "sqs.Queue attribute {attribute} requires fifo_queue = true"
            ))
            .for_resource(resource.id.clone()));
        }
    }

    Ok(())
}

/// Serialise a `Value::Map { dead_letter_target_arn, max_receive_count }`
/// to the `{"deadLetterTargetArn": "...", "maxReceiveCount": N}` JSON
/// string SQS expects.
fn redrive_policy_to_json_string(value: &Value) -> Option<String> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return None;
    };
    let arn = match map.get("dead_letter_target_arn") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
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
    let count = match obj.get("maxReceiveCount")? {
        serde_json::Value::Number(number) => number.as_i64()?,
        serde_json::Value::String(value) => value.parse::<i64>().ok()?,
        _ => return None,
    };
    let mut out = IndexMap::new();
    out.insert(
        "dead_letter_target_arn".to_string(),
        Value::Concrete(ConcreteValue::String(arn.to_string().into())),
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
        Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
        Some(Value::Concrete(ConcreteValue::CanonicalEnum(c))) => c.api_value().to_string(),
        Some(Value::Concrete(ConcreteValue::EnumIdentifier(_))) => unreachable!(
            "unresolved enum identifier must not reach SQS redrive policy serialization"
        ),
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
                    Some(serde_json::Value::String(s.expose_plaintext().to_string()))
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
        Value::Concrete(ConcreteValue::String(permission_raw.to_string().into())),
    );
    if let Some(arns_json) = obj.get("sourceQueueArns")
        && let Some(arr) = arns_json.as_array()
    {
        let arns: Vec<Value> = arr
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
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
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let queue_url = identifier.as_str();

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
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                return Err(api_error_with_meta("Failed to get queue attributes", err)
                    .for_resource(id.clone()));
            }
        };

        let name = queue_url
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ProviderError::internal(format!("invalid SQS queue URL identifier: {queue_url}"))
                    .for_resource(id.clone())
            })?;
        let attributes = result.attributes();

        // Tags come from a separate API. Empty tags are canonical state; a
        // failed side read must not erase configured tags.
        let tags_output = self
            .sqs_client
            .list_queue_tags()
            .queue_url(queue_url)
            .send()
            .await
            .map_err(|error| {
                api_error_with_meta(
                    "ListQueueTags failed; ensure sqs:ListQueueTags is allowed",
                    error,
                )
                .for_resource(id.clone())
            })?;
        let mut tags = IndexMap::new();
        if let Some(returned_tags) = tags_output.tags() {
            for (key, value) in returned_tags {
                tags.insert(
                    key.to_string(),
                    Value::Concrete(ConcreteValue::String(value.to_string().into())),
                );
            }
        }

        // Measured with `aws sqs get-queue-attributes --queue-url <standard-queue-url>
        // --attribute-names All --region ap-northeast-1` on 2026-10-29:
        // standard queues omit both
        // FIFO booleans, so their canonical read value is false. The same
        // measurement omits KmsDataKeyReusePeriodSeconds when no KMS key is set.
        let model = SqsQueueReadModel {
            queue_name: Value::Concrete(ConcreteValue::String(name.to_string().into())),
            policy: queue_attribute(attributes, &QueueAttributeName::Policy, AttrKind::IamPolicy),
            visibility_timeout: required_queue_attribute(
                id,
                attributes,
                "visibility_timeout",
                &QueueAttributeName::VisibilityTimeout,
                AttrKind::Int,
            )?,
            message_retention_period: required_queue_attribute(
                id,
                attributes,
                "message_retention_period",
                &QueueAttributeName::MessageRetentionPeriod,
                AttrKind::Int,
            )?,
            delay_seconds: required_queue_attribute(
                id,
                attributes,
                "delay_seconds",
                &QueueAttributeName::DelaySeconds,
                AttrKind::Int,
            )?,
            maximum_message_size: required_queue_attribute(
                id,
                attributes,
                "maximum_message_size",
                &QueueAttributeName::MaximumMessageSize,
                AttrKind::Int,
            )?,
            receive_message_wait_time_seconds: required_queue_attribute(
                id,
                attributes,
                "receive_message_wait_time_seconds",
                &QueueAttributeName::ReceiveMessageWaitTimeSeconds,
                AttrKind::Int,
            )?,
            redrive_policy: queue_attribute(
                attributes,
                &QueueAttributeName::RedrivePolicy,
                AttrKind::RedrivePolicyStruct,
            ),
            redrive_allow_policy: queue_attribute(
                attributes,
                &QueueAttributeName::RedriveAllowPolicy,
                AttrKind::RedriveAllowPolicyStruct,
            ),
            fifo_queue: queue_attribute(attributes, &QueueAttributeName::FifoQueue, AttrKind::Bool)
                .unwrap_or_else(|| Value::Concrete(ConcreteValue::Bool(false))),
            content_based_deduplication: queue_attribute(
                attributes,
                &QueueAttributeName::ContentBasedDeduplication,
                AttrKind::Bool,
            )
            .unwrap_or_else(|| Value::Concrete(ConcreteValue::Bool(false))),
            deduplication_scope: queue_attribute(
                attributes,
                &QueueAttributeName::DeduplicationScope,
                AttrKind::String,
            ),
            fifo_throughput_limit: queue_attribute(
                attributes,
                &QueueAttributeName::FifoThroughputLimit,
                AttrKind::String,
            ),
            kms_master_key_id: queue_attribute(
                attributes,
                &QueueAttributeName::KmsMasterKeyId,
                AttrKind::String,
            ),
            kms_data_key_reuse_period_seconds: queue_attribute(
                attributes,
                &QueueAttributeName::KmsDataKeyReusePeriodSeconds,
                AttrKind::Int,
            ),
            sqs_managed_sse_enabled: queue_attribute(
                attributes,
                &QueueAttributeName::SqsManagedSseEnabled,
                AttrKind::Bool,
            ),
            queue_arn: required_queue_attribute(
                id,
                attributes,
                "queue_arn",
                &QueueAttributeName::QueueArn,
                AttrKind::String,
            )?,
            approximate_number_of_messages: queue_attribute(
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessages,
                AttrKind::Int,
            ),
            approximate_number_of_messages_delayed: queue_attribute(
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessagesDelayed,
                AttrKind::Int,
            ),
            approximate_number_of_messages_not_visible: queue_attribute(
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessagesNotVisible,
                AttrKind::Int,
            ),
            created_timestamp: required_queue_attribute(
                id,
                attributes,
                "created_timestamp",
                &QueueAttributeName::CreatedTimestamp,
                AttrKind::Int,
            )?,
            last_modified_timestamp: required_queue_attribute(
                id,
                attributes,
                "last_modified_timestamp",
                &QueueAttributeName::LastModifiedTimestamp,
                AttrKind::Int,
            )?,
            tags: Value::Concrete(ConcreteValue::Map(tags)),
        };

        found_managed_read(id, applied_identifier, model)
    }

    /// Create an SQS Queue and return its post-create state.
    pub(crate) async fn create_sqs_queue(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        validate_sqs_queue_inputs(resource)?;
        let input = SqsQueueCreateInput::decode(resource)?;
        let sent = input.sent();
        let (
            content_based_deduplication,
            deduplication_scope,
            delay_seconds,
            fifo_queue,
            fifo_throughput_limit,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            queue_name,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        ) = input.into_parts();
        let fifo = fifo_queue.unwrap_or(false);
        let desired = HashMap::from([
            (
                "content_based_deduplication",
                optional_bool_value(content_based_deduplication),
            ),
            (
                "deduplication_scope",
                optional_string_value(deduplication_scope),
            ),
            ("delay_seconds", optional_int_value(delay_seconds)),
            ("fifo_queue", optional_bool_value(fifo_queue)),
            (
                "fifo_throughput_limit",
                optional_string_value(fifo_throughput_limit),
            ),
            (
                "kms_data_key_reuse_period_seconds",
                optional_int_value(kms_data_key_reuse_period_seconds),
            ),
            (
                "kms_master_key_id",
                optional_string_value(kms_master_key_id),
            ),
            (
                "maximum_message_size",
                optional_int_value(maximum_message_size),
            ),
            (
                "message_retention_period",
                optional_int_value(message_retention_period),
            ),
            ("policy", policy),
            (
                "receive_message_wait_time_seconds",
                optional_int_value(receive_message_wait_time_seconds),
            ),
            ("redrive_allow_policy", redrive_allow_policy),
            ("redrive_policy", redrive_policy),
            (
                "sqs_managed_sse_enabled",
                optional_bool_value(sqs_managed_sse_enabled),
            ),
            ("visibility_timeout", optional_int_value(visibility_timeout)),
        ]);

        // Pack every writable attribute the user actually set.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key, kind, writable) in QUEUE_ATTRS {
            if !writable {
                continue;
            }
            // Measured with `aws sqs create-queue --queue-name <name>
            // --attributes FifoQueue=false --region ap-northeast-1` and the
            // same command with `ContentBasedDeduplication=false` on
            // 2026-10-29: standard queues reject both attribute names.
            if !fifo
                && matches!(
                    sdk_key,
                    QueueAttributeName::FifoQueue
                        | QueueAttributeName::ContentBasedDeduplication
                        | QueueAttributeName::DeduplicationScope
                        | QueueAttributeName::FifoThroughputLimit
                )
            {
                continue;
            }
            if let Some(Some(value)) = desired.get(attr_name)
                && let Some(serialized) = pack_attr(value, *kind)
            {
                attr_map.insert(sdk_key.clone(), serialized);
            }
        }

        let tags = tags_map(tags.as_ref());

        let mut req = self.sqs_client.create_queue().queue_name(&queue_name);
        if !attr_map.is_empty() {
            req = req.set_attributes(Some(attr_map));
        }
        if let Some(tags) = tags {
            req = req.set_tags(Some(tags));
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create SQS queue", e).for_resource(resource.id.clone())
        })?;

        let queue_url = result.queue_url().ok_or_else(|| {
            ProviderError::api_error("CreateQueue returned no QueueUrl")
                .for_resource(resource.id.clone())
        })?;

        let identifier =
            crate::helpers::applied_identifier(&resource.id, "sqs.CreateQueue", queue_url)?;
        let outcome = self.read_sqs_queue(&resource.id, &identifier).await;
        Ok((
            sent,
            self.create_outcome_from_read(
                &resource.id,
                identifier,
                resource.resolved_attributes(),
                outcome,
            ),
        ))
    }

    /// Update an existing SQS Queue.
    pub(crate) async fn update_sqs_queue(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        validate_sqs_queue_inputs(&to)?;
        let input = SqsQueueUpdateInput::decode(&to)?;
        let (
            content_based_deduplication,
            delay_seconds,
            kms_data_key_reuse_period_seconds,
            kms_master_key_id,
            maximum_message_size,
            message_retention_period,
            policy,
            receive_message_wait_time_seconds,
            redrive_allow_policy,
            redrive_policy,
            sqs_managed_sse_enabled,
            tags,
            visibility_timeout,
        ) = input.into_parts();
        let desired = HashMap::from([
            (
                "content_based_deduplication",
                optional_bool_value(content_based_deduplication),
            ),
            ("delay_seconds", optional_int_value(delay_seconds)),
            (
                "kms_data_key_reuse_period_seconds",
                optional_int_value(kms_data_key_reuse_period_seconds),
            ),
            (
                "kms_master_key_id",
                optional_string_value(kms_master_key_id),
            ),
            (
                "maximum_message_size",
                optional_int_value(maximum_message_size),
            ),
            (
                "message_retention_period",
                optional_int_value(message_retention_period),
            ),
            ("policy", policy),
            (
                "receive_message_wait_time_seconds",
                optional_int_value(receive_message_wait_time_seconds),
            ),
            ("redrive_allow_policy", redrive_allow_policy),
            ("redrive_policy", redrive_policy),
            (
                "sqs_managed_sse_enabled",
                optional_bool_value(sqs_managed_sse_enabled),
            ),
            ("visibility_timeout", optional_int_value(visibility_timeout)),
        ]);
        // Build a partial map of attributes whose value changed.
        // Create-only attributes (FifoQueue / FifoThroughputLimit /
        // DeduplicationScope) are dropped — schema marks them
        // create_only, but defend in depth.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key, kind, writable) in QUEUE_ATTRS {
            if !writable || is_create_only(sdk_key) {
                continue;
            }
            let desired = desired.get(attr_name).and_then(Option::as_ref);
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
                .queue_url(identifier.as_str())
                .set_attributes(Some(attr_map))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to set queue attributes", e)
                        .for_resource(id.clone())
                })?;
        }

        self.apply_sqs_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;

        Ok(self.read_sqs_queue(&id, identifier).await)
    }

    /// Delete an SQS Queue.
    pub(crate) async fn delete_sqs_queue(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.sqs_client
            .delete_queue()
            .queue_url(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete SQS queue", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Apply tag diff between desired and current state.
    async fn apply_sqs_tags(
        &self,
        id: &ResourceId,
        queue_url: &str,
        desired: Option<&Value>,
        current: Option<&HashMap<String, Value>>,
    ) -> ProviderResult<()> {
        let desired_tags = match desired {
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
                    api_error_with_meta("Failed to untag SQS queue", e).for_resource(id.clone())
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
                    tags_to_add.insert(key.clone(), val.expose_plaintext().to_string());
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
                    api_error_with_meta("Failed to tag SQS queue", e).for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}

/// Extract a `tags = { ... }` block from the resource, dropping any
/// non-string values. Returns `None` when no tags are set.
fn tags_map(tags: Option<&Value>) -> Option<HashMap<String, String>> {
    let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = tags else {
        return None;
    };
    let mut out = HashMap::new();
    for (key, value) in tag_map {
        if let Value::Concrete(ConcreteValue::String(val)) = value {
            out.insert(key.clone(), val.expose_plaintext().to_string());
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::{DeferredValue, UnknownReason};

    fn queue_resource() -> Resource {
        Resource::with_provider("aws", "sqs.Queue", "test", None)
    }

    fn unknown() -> Value {
        Value::Deferred(DeferredValue::Unknown(UnknownReason::ForValue))
    }

    #[test]
    fn unresolved_fifo_queue_defers_fifo_only_validation() {
        let mut resource = queue_resource();
        resource.set_attr("fifo_queue", unknown());
        resource.set_attr(
            "deduplication_scope",
            Value::Concrete(ConcreteValue::String("messageGroup".into())),
        );

        validate_sqs_queue_inputs(&resource)
            .expect("an unresolved fifo_queue value must not be treated as false");
    }

    #[test]
    fn unresolved_fifo_only_value_is_not_rejected_for_a_standard_queue() {
        let mut resource = queue_resource();
        resource.set_attr("fifo_queue", Value::Concrete(ConcreteValue::Bool(false)));
        resource.set_attr(
            "deduplication_scope",
            Value::resource_ref("upstream", "deduplication_scope", vec![]),
        );

        validate_sqs_queue_inputs(&resource)
            .expect("a deferred FIFO-only value must wait for resolution");
    }

    #[test]
    fn unresolved_kms_key_is_not_treated_as_absent() {
        let mut resource = queue_resource();
        resource.set_attr(
            "kms_data_key_reuse_period_seconds",
            Value::Concrete(ConcreteValue::Int(300)),
        );
        resource.set_attr("kms_master_key_id", unknown());

        validate_sqs_queue_inputs(&resource)
            .expect("an unresolved KMS key is present but not yet decidable");
    }

    #[test]
    fn unresolved_kms_reuse_period_defers_missing_key_validation() {
        let mut resource = queue_resource();
        resource.set_attr("kms_data_key_reuse_period_seconds", unknown());

        validate_sqs_queue_inputs(&resource)
            .expect("an unresolved reuse period must wait for resolution");
    }

    #[test]
    #[should_panic(
        expected = "unresolved enum identifier must not reach SQS redrive policy serialization"
    )]
    fn redrive_allow_policy_rejects_unresolved_enum_identifier() {
        let mut map = IndexMap::new();
        map.insert(
            "redrive_permission".to_string(),
            Value::Concrete(ConcreteValue::enum_identifier("allow_all")),
        );

        redrive_allow_policy_to_json_string(&Value::Concrete(ConcreteValue::Map(map)));
    }

    #[test]
    fn redrive_allow_policy_serializes_canonical_enum_api_value() {
        let mut map = IndexMap::new();
        map.insert(
            "redrive_permission".to_string(),
            Value::Concrete(ConcreteValue::String("allowAll".into())),
        );
        let schema = crate::test_rootless_type(carina_aws_types::sqs_redrive_allow_policy());
        let canonical = schema.canonicalize_state(Value::Concrete(ConcreteValue::Map(map)));

        let json = redrive_allow_policy_to_json_string(&canonical).expect("redrive allow JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["redrivePermission"].as_str(), Some("allowAll"));
    }
}
