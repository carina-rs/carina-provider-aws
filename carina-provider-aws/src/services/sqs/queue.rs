//! Hand-written service implementation for `aws.sqs.Queue`.
//!
//! SQS exposes every per-queue knob through
//! `Attributes: Map<QueueAttributeName, String>` on `CreateQueue` /
//! `GetQueueAttributes` / `SetQueueAttributes`. Schema-side we project
//! every `QueueAttributeName` variant as a flat top-level attribute
//! via `extra_attributes` (see `carina-codegen-aws/src/resource_defs.rs::
//! sqs_resources`); the map↔flat packing lives here.

use carina_aws_types::sdk::sqs::types::QueueAttributeName;
use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, SnapshotState,
    SnapshotValue, Value,
};
use carina_core::schema::CompiledResourceSchema;
use carina_plugin_sdk::{DesiredInputs, DesiredScalar, DesiredValue, NativeDesiredNormalization};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::SqsQueueReadModel;
use crate::send_models_generated::{SqsQueueCreateInput, SqsQueueUpdateInput};
// IAM policy doc Struct↔JSON converters are borrowed via pub(crate)
// from services/iam/role.rs until aws#286 extracts them into a shared
// module. SQS's Policy and RedriveAllowPolicy are IAM-shaped.
use crate::services::iam::role::{
    IamPolicyDecodeError, iam_policy_json_to_value, policy_doc_to_json,
};

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
fn unpack_attr(raw: &str, kind: AttrKind) -> Result<Option<Value>, SqsAttributeValueError> {
    Ok(match kind {
        AttrKind::Int => Some(Value::Concrete(ConcreteValue::Int(
            raw.parse().map_err(SqsAttributeValueError::Int)?,
        ))),
        AttrKind::Bool => Some(Value::Concrete(ConcreteValue::Bool(
            raw.parse().map_err(SqsAttributeValueError::Bool)?,
        ))),
        AttrKind::String => Some(Value::Concrete(ConcreteValue::String(
            raw.to_string().into(),
        ))),
        AttrKind::IamPolicy if raw.is_empty() => None,
        AttrKind::IamPolicy => {
            Some(iam_policy_json_to_value(raw).map_err(SqsAttributeValueError::Policy)?)
        }
        AttrKind::RedrivePolicyStruct if raw.is_empty() => None,
        AttrKind::RedrivePolicyStruct => {
            let json = serde_json::from_str(raw).map_err(SqsAttributeValueError::RedriveJson)?;
            Some(json_to_redrive_policy(&json).map_err(SqsAttributeValueError::RedriveShape)?)
        }
        AttrKind::RedriveAllowPolicyStruct if raw.is_empty() => None,
        AttrKind::RedriveAllowPolicyStruct => {
            let json = serde_json::from_str(raw).map_err(SqsAttributeValueError::RedriveJson)?;
            Some(
                json_to_redrive_allow_policy(&json)
                    .map_err(SqsAttributeValueError::RedriveShape)?,
            )
        }
    })
}

#[derive(Debug, thiserror::Error)]
enum SqsAttributeValueError {
    #[error("invalid integer")]
    Int(#[source] std::num::ParseIntError),
    #[error("invalid boolean")]
    Bool(#[source] std::str::ParseBoolError),
    #[error("invalid IAM policy")]
    Policy(#[source] IamPolicyDecodeError),
    #[error("invalid redrive JSON")]
    RedriveJson(#[source] serde_json::Error),
    #[error("invalid redrive policy shape")]
    RedriveShape(#[source] SqsRedriveShapeError),
}

#[derive(Debug, thiserror::Error)]
enum SqsRedriveShapeError {
    #[error("{0}")]
    Expected(&'static str),
    #[error("invalid maxReceiveCount")]
    MaxReceiveCount(#[source] std::num::ParseIntError),
}

#[derive(Debug, thiserror::Error)]
#[error("failed to decode SQS queue attribute {attribute}", attribute = .attribute.as_str())]
struct SqsAttributeDecodeError {
    attribute: QueueAttributeName,
    #[source]
    source: SqsAttributeValueError,
}

fn queue_attribute(
    id: &ResourceId,
    attributes: Option<&HashMap<QueueAttributeName, String>>,
    key: &QueueAttributeName,
    kind: AttrKind,
) -> ProviderResult<Option<Value>> {
    attributes
        .and_then(|attributes| attributes.get(key))
        .map(|raw| unpack_attr(raw, kind))
        .transpose()
        .map(Option::flatten)
        .map_err(|source| {
            ProviderError::api_error_caused_by(
                "Failed to decode SQS queue attribute",
                SqsAttributeDecodeError {
                    attribute: key.clone(),
                    source,
                },
            )
            .for_resource(id.clone())
        })
}

fn required_queue_attribute(
    id: &ResourceId,
    attributes: Option<&HashMap<QueueAttributeName, String>>,
    dsl_name: &str,
    key: &QueueAttributeName,
    kind: AttrKind,
) -> ProviderResult<Value> {
    queue_attribute(id, attributes, key, kind)?.ok_or_else(|| {
        ProviderError::api_error(format!(
            "GetQueueAttributes(All) omitted required SQS attribute {dsl_name}"
        ))
        .for_resource(id.clone())
    })
}

/// Reject cross-field combinations before they reach an AWS API that would
/// reject or silently ignore them.
pub(crate) fn validate_sqs_queue_inputs(inputs: &DesiredInputs<'_>) -> ProviderResult<()> {
    if !inputs.is_resource("aws", "sqs.Queue") {
        return Ok(());
    }

    validate_sqs_queue_attributes(inputs.id(), |name, check| {
        Ok(check(queue_validation_attribute(inputs, name)))
    })
}

/// The distinctions cross-field validation needs, without owning input values.
enum QueueValidationValue<'a> {
    Bool(bool),
    String(&'a str),
    ListLen(usize),
    Scalar,
    Other,
}

impl<'a> From<&'a DesiredValue> for QueueValidationValue<'a> {
    fn from(value: &'a DesiredValue) -> Self {
        match value {
            DesiredValue::Known(DesiredScalar::Bool(value)) => Self::Bool(*value),
            DesiredValue::Known(DesiredScalar::String(value)) => {
                Self::String(value.expose_plaintext())
            }
            DesiredValue::Known(_) => Self::Scalar,
            DesiredValue::List(values) => Self::ListLen(values.iter().count()),
            _ => Self::Other,
        }
    }
}

fn queue_validation_attribute<'a>(
    inputs: &'a DesiredInputs<'_>,
    path: &str,
) -> Option<QueueValidationValue<'a>> {
    let mut parts = path.split('.');
    let mut value = inputs.attribute(parts.next()?);
    for part in parts {
        value = match value {
            Some(DesiredValue::Map(fields)) => fields.get(part),
            // A deferred parent does not make its children definitively absent.
            Some(_) => return Some(QueueValidationValue::Other),
            None => return None,
        };
    }
    value.map(QueueValidationValue::from)
}

// Evaluate each predicate while its borrowed projection is alive. In particular,
// update validation must not retain strings from a temporary normalization view.
type QueueValidationCheck = fn(Option<QueueValidationValue<'_>>) -> bool;

fn validate_sqs_queue_update_inputs(to: &crate::helpers::UpdateResource) -> ProviderResult<()> {
    validate_sqs_queue_attributes(&to.id, |path, check| {
        let name = path
            .split('.')
            .next()
            .expect("validation paths have a root");
        let mut resource = to.materialize(&[name])?;
        let desired = NativeDesiredNormalization::from_core(&mut resource)?;
        Ok(check(queue_validation_attribute(&desired.inputs(), path)))
    })
}

fn defer_unavailable_queue_check(result: ProviderResult<bool>) -> ProviderResult<bool> {
    match result {
        // New create-only checks may need an untouched historical secret whose
        // plaintext is unavailable. Leave its hash historical and the check
        // undecided; these inputs are not sent by SetQueueAttributes.
        Err(error)
            if error.detail().cause().is_some_and(|cause| {
                cause.is::<carina_core::resource::UnavailableSecretError>()
            }) =>
        {
            Ok(false)
        }
        result => result,
    }
}

fn validate_sqs_queue_attributes(
    id: &ResourceId,
    mut attribute: impl FnMut(&str, QueueValidationCheck) -> ProviderResult<bool>,
) -> ProviderResult<()> {
    if attribute("kms_data_key_reuse_period_seconds", |value| {
        matches!(
            value,
            Some(
                QueueValidationValue::Bool(_)
                    | QueueValidationValue::String(_)
                    | QueueValidationValue::Scalar
            )
        )
    })? && attribute("kms_master_key_id", |value| value.is_none())?
    {
        return Err(ProviderError::invalid_input(
            "sqs.Queue attribute kms_data_key_reuse_period_seconds requires kms_master_key_id",
        )
        .for_resource(id.clone()));
    }

    let fifo_is_definitively_standard = attribute("fifo_queue", |value| {
        matches!(value, None | Some(QueueValidationValue::Bool(false)))
    })?;
    if fifo_is_definitively_standard {
        let content_dedup_enabled = attribute("content_based_deduplication", |value| {
            matches!(value, Some(QueueValidationValue::Bool(true)))
        })?;
        let unsupported = if content_dedup_enabled {
            Some("content_based_deduplication")
        } else if attribute("deduplication_scope", |value| {
            matches!(
                value,
                Some(
                    QueueValidationValue::Bool(_)
                        | QueueValidationValue::String(_)
                        | QueueValidationValue::Scalar
                )
            )
        })? {
            Some("deduplication_scope")
        } else if attribute("fifo_throughput_limit", |value| {
            matches!(
                value,
                Some(
                    QueueValidationValue::Bool(_)
                        | QueueValidationValue::String(_)
                        | QueueValidationValue::Scalar
                )
            )
        })? {
            Some("fifo_throughput_limit")
        } else {
            None
        };
        if let Some(attribute) = unsupported {
            return Err(ProviderError::invalid_input(format!(
                "sqs.Queue attribute {attribute} requires fifo_queue = true"
            ))
            .for_resource(id.clone()));
        }
    }

    // FIFO names require the suffix, and standard names cannot use it:
    // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
    // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-fifo-queue-message-identifiers.html
    if fifo_is_definitively_standard {
        if defer_unavailable_queue_check(attribute("queue_name", |value| {
            matches!(
                value, Some(QueueValidationValue::String(name)) if name.ends_with(".fifo")
            )
        }))? {
            return Err(ProviderError::invalid_input(
                "sqs.Queue attribute queue_name must not end with .fifo when fifo_queue is false or absent",
            ).for_resource(id.clone()));
        }
    } else if attribute("fifo_queue", |value| {
        matches!(value, Some(QueueValidationValue::Bool(true)))
    })? && defer_unavailable_queue_check(attribute("queue_name", |value| {
        matches!(
            value, Some(QueueValidationValue::String(name)) if !name.ends_with(".fifo")
        )
    }))? {
        return Err(ProviderError::invalid_input(
            "sqs.Queue attribute queue_name must end with .fifo when fifo_queue = true",
        )
        .for_resource(id.clone()));
    }

    // CreateQueue permits perMessageGroupId only with messageGroup scope.
    if defer_unavailable_queue_check(attribute("fifo_throughput_limit", |value| {
        matches!(
            value,
            Some(QueueValidationValue::String("perMessageGroupId"))
        )
    }))? && defer_unavailable_queue_check(attribute(
        "deduplication_scope",
        |value| match value {
            None => true,
            Some(QueueValidationValue::String(scope)) => scope != "messageGroup",
            _ => false,
        },
    ))? {
        return Err(ProviderError::invalid_input(
            "sqs.Queue attribute fifo_throughput_limit requires deduplication_scope = messageGroup when set to perMessageGroupId",
        ).for_resource(id.clone()));
    }

    // CreateQueue limits sourceQueueArns to byQueue and at most ten entries.
    // Real AWS SetQueueAttributes also rejects an empty list for byQueue;
    // require a non-empty list while leaving deferred lists undecided.
    // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-configure-queue-parameters.html
    let by_queue = attribute("redrive_allow_policy.redrive_permission", |value| {
        matches!(value, Some(QueueValidationValue::String("byQueue")))
    })?;
    if by_queue {
        if attribute("redrive_allow_policy.source_queue_arns", |value| {
            matches!(value, None | Some(QueueValidationValue::ListLen(0)))
        })? {
            return Err(ProviderError::invalid_input(
                "sqs.Queue attribute redrive_allow_policy.redrive_permission requires redrive_allow_policy.source_queue_arns to be non-empty when set to byQueue",
            ).for_resource(id.clone()));
        }
        if attribute("redrive_allow_policy.source_queue_arns", |value| {
            matches!(
                value, Some(QueueValidationValue::ListLen(count)) if count > 10
            )
        })? {
            return Err(ProviderError::invalid_input(
                "sqs.Queue attribute redrive_allow_policy.source_queue_arns must contain at most 10 items",
            ).for_resource(id.clone()));
        }
    } else if attribute("redrive_allow_policy.redrive_permission", |value| {
        matches!(value, None | Some(QueueValidationValue::String(_)))
    })? && attribute("redrive_allow_policy.source_queue_arns", |value| {
        matches!(value, Some(QueueValidationValue::ListLen(_)))
    })? {
        return Err(ProviderError::invalid_input(
            "sqs.Queue attribute redrive_allow_policy.source_queue_arns may only be set when redrive_allow_policy.redrive_permission = byQueue",
        ).for_resource(id.clone()));
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
fn json_to_redrive_policy(json: &serde_json::Value) -> Result<Value, SqsRedriveShapeError> {
    let obj = json
        .as_object()
        .ok_or(SqsRedriveShapeError::Expected("expected a JSON object"))?;
    let arn = obj
        .get("deadLetterTargetArn")
        .and_then(serde_json::Value::as_str)
        .ok_or(SqsRedriveShapeError::Expected(
            "deadLetterTargetArn must be a string",
        ))?;
    // AWS uses both integer strings and JSON integers for maxReceiveCount.
    let count = match obj.get("maxReceiveCount") {
        Some(serde_json::Value::Number(number)) => number.as_i64().ok_or(
            SqsRedriveShapeError::Expected("maxReceiveCount must be an i64 integer"),
        )?,
        Some(serde_json::Value::String(value)) => value
            .parse::<i64>()
            .map_err(SqsRedriveShapeError::MaxReceiveCount)?,
        _ => {
            return Err(SqsRedriveShapeError::Expected(
                "maxReceiveCount must be an integer or integer string",
            ));
        }
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
    Ok(Value::Concrete(ConcreteValue::Map(out)))
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
fn json_to_redrive_allow_policy(json: &serde_json::Value) -> Result<Value, SqsRedriveShapeError> {
    let obj = json
        .as_object()
        .ok_or(SqsRedriveShapeError::Expected("expected a JSON object"))?;
    let permission_raw = obj
        .get("redrivePermission")
        .and_then(serde_json::Value::as_str)
        .ok_or(SqsRedriveShapeError::Expected(
            "redrivePermission must be a string",
        ))?;
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
    // sourceQueueArns is optional (allowAll / denyAll), but must decode if present.
    if let Some(arns_json) = obj.get("sourceQueueArns") {
        let arr = arns_json.as_array().ok_or(SqsRedriveShapeError::Expected(
            "sourceQueueArns must be an array of strings",
        ))?;
        let arns: Vec<Value> = arr
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                    .ok_or(SqsRedriveShapeError::Expected(
                        "sourceQueueArns must contain only strings",
                    ))
            })
            .collect::<Result<_, _>>()?;
        out.insert(
            "source_queue_arns".to_string(),
            Value::Concrete(ConcreteValue::List(arns)),
        );
    }
    Ok(Value::Concrete(ConcreteValue::Map(out)))
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
            policy: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::Policy,
                AttrKind::IamPolicy,
            )?,
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
                id,
                attributes,
                &QueueAttributeName::RedrivePolicy,
                AttrKind::RedrivePolicyStruct,
            )?,
            redrive_allow_policy: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::RedriveAllowPolicy,
                AttrKind::RedriveAllowPolicyStruct,
            )?,
            fifo_queue: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::FifoQueue,
                AttrKind::Bool,
            )?
            .unwrap_or_else(|| Value::Concrete(ConcreteValue::Bool(false))),
            content_based_deduplication: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::ContentBasedDeduplication,
                AttrKind::Bool,
            )?
            .unwrap_or_else(|| Value::Concrete(ConcreteValue::Bool(false))),
            deduplication_scope: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::DeduplicationScope,
                AttrKind::String,
            )?,
            fifo_throughput_limit: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::FifoThroughputLimit,
                AttrKind::String,
            )?,
            kms_master_key_id: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::KmsMasterKeyId,
                AttrKind::String,
            )?,
            kms_data_key_reuse_period_seconds: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::KmsDataKeyReusePeriodSeconds,
                AttrKind::Int,
            )?,
            sqs_managed_sse_enabled: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::SqsManagedSseEnabled,
                AttrKind::Bool,
            )?,
            queue_arn: required_queue_attribute(
                id,
                attributes,
                "queue_arn",
                &QueueAttributeName::QueueArn,
                AttrKind::String,
            )?,
            approximate_number_of_messages: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessages,
                AttrKind::Int,
            )?,
            approximate_number_of_messages_delayed: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessagesDelayed,
                AttrKind::Int,
            )?,
            approximate_number_of_messages_not_visible: queue_attribute(
                id,
                attributes,
                &QueueAttributeName::ApproximateNumberOfMessagesNotVisible,
                AttrKind::Int,
            )?,
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
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
        schema: &CompiledResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = SqsQueueCreateInput::decode(resource, schema)?;
        let sent = input.sent();
        let crate::send_models_generated::SqsQueueCreateParts {
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
        } = input.into_parts();
        let content_based_deduplication = content_based_deduplication.into_inner();
        let deduplication_scope = deduplication_scope.into_inner();
        let delay_seconds = delay_seconds.into_inner();
        let fifo_queue = fifo_queue.into_inner();
        let fifo_throughput_limit = fifo_throughput_limit.into_inner();
        let kms_data_key_reuse_period_seconds = kms_data_key_reuse_period_seconds.into_inner();
        let kms_master_key_id = kms_master_key_id.into_inner();
        let maximum_message_size = maximum_message_size.into_inner();
        let message_retention_period = message_retention_period.into_inner();
        let policy = policy.into_inner();
        let queue_name = queue_name.into_inner();
        let receive_message_wait_time_seconds = receive_message_wait_time_seconds.into_inner();
        let redrive_allow_policy = redrive_allow_policy.into_inner();
        let redrive_policy = redrive_policy.into_inner();
        let sqs_managed_sse_enabled = sqs_managed_sse_enabled.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let visibility_timeout = visibility_timeout.into_inner();
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
        from: &SnapshotState,
        to: &crate::helpers::UpdateResource,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // Recheck decisions that were deferred until apply-time resolution.
        validate_sqs_queue_update_inputs(to)?;
        let input = SqsQueueUpdateInput::decode(to)?;
        let crate::send_models_generated::SqsQueueUpdateParts {
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
        } = input.into_parts();
        let content_based_deduplication = content_based_deduplication.into_inner();
        let delay_seconds = delay_seconds.into_inner();
        let kms_data_key_reuse_period_seconds = kms_data_key_reuse_period_seconds.into_inner();
        let kms_master_key_id = kms_master_key_id.into_inner();
        let maximum_message_size = maximum_message_size.into_inner();
        let message_retention_period = message_retention_period.into_inner();
        let policy = policy.into_inner();
        let receive_message_wait_time_seconds = receive_message_wait_time_seconds.into_inner();
        let redrive_allow_policy = redrive_allow_policy.into_inner();
        let redrive_policy = redrive_policy.into_inner();
        let sqs_managed_sse_enabled = sqs_managed_sse_enabled.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let visibility_timeout = visibility_timeout.into_inner();
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
            if desired.map(Value::to_snapshot).as_ref() == current {
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
        current: Option<&HashMap<String, SnapshotValue>>,
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
    use carina_core::provider::{ProviderErrorKind, UpdatePatch};
    use carina_core::resource::DeferredValue;
    use std::error::Error;

    fn assert_attribute_decode_failure(
        raw: &str,
        key: QueueAttributeName,
        kind: AttrKind,
        check: impl FnOnce(&SqsAttributeValueError),
    ) {
        let id = queue_resource().id;
        let attributes = HashMap::from([(key.clone(), raw.to_string())]);
        let error = queue_attribute(&id, Some(&attributes), &key, kind)
            .expect_err("a malformed attribute must fail the read");
        assert_eq!(error.kind(), ProviderErrorKind::ApiError);
        let decode = error
            .detail()
            .cause()
            .unwrap()
            .downcast_ref::<SqsAttributeDecodeError>()
            .expect("typed SQS attribute decode error");
        assert_eq!(decode.attribute, key);
        check(
            decode
                .source()
                .unwrap()
                .downcast_ref::<SqsAttributeValueError>()
                .expect("typed SQS attribute value error"),
        );
    }

    #[test]
    fn sqs_integer_decode_failure_keeps_its_typed_cause() {
        for raw in [
            "",
            "not-an-int",
            "9223372036854775808",
            "-9223372036854775809",
        ] {
            assert_attribute_decode_failure(
                raw,
                QueueAttributeName::VisibilityTimeout,
                AttrKind::Int,
                |error| {
                    assert!(matches!(error, SqsAttributeValueError::Int(_)));
                    assert!(error.source().unwrap().is::<std::num::ParseIntError>());
                },
            );
        }
    }

    #[test]
    fn sqs_boolean_decode_failure_keeps_its_typed_cause() {
        for raw in ["", "TRUE", "1"] {
            assert_attribute_decode_failure(
                raw,
                QueueAttributeName::FifoQueue,
                AttrKind::Bool,
                |error| {
                    assert!(matches!(error, SqsAttributeValueError::Bool(_)));
                    assert!(error.source().unwrap().is::<std::str::ParseBoolError>());
                },
            );
        }
    }

    #[test]
    fn sqs_redrive_json_failure_keeps_its_typed_cause() {
        for (key, kind) in [
            (
                QueueAttributeName::RedrivePolicy,
                AttrKind::RedrivePolicyStruct,
            ),
            (
                QueueAttributeName::RedriveAllowPolicy,
                AttrKind::RedriveAllowPolicyStruct,
            ),
        ] {
            for raw in ["{", "1e999"] {
                assert_attribute_decode_failure(raw, key.clone(), kind, |error| {
                    assert!(matches!(error, SqsAttributeValueError::RedriveJson(_)));
                    assert!(error.source().unwrap().is::<serde_json::Error>());
                });
            }
        }
    }

    #[test]
    fn sqs_redrive_shape_failure_keeps_its_typed_cause() {
        for (key, kind, inputs) in [
            (
                QueueAttributeName::RedrivePolicy,
                AttrKind::RedrivePolicyStruct,
                &[
                    "null",
                    "[]",
                    "{}",
                    r#"{"deadLetterTargetArn":"arn"}"#,
                    r#"{"deadLetterTargetArn":null,"maxReceiveCount":5}"#,
                    r#"{"deadLetterTargetArn":"arn","maxReceiveCount":null}"#,
                    r#"{"deadLetterTargetArn":"arn","maxReceiveCount":1.5}"#,
                    r#"{"deadLetterTargetArn":"arn","maxReceiveCount":9223372036854775808}"#,
                ][..],
            ),
            (
                QueueAttributeName::RedriveAllowPolicy,
                AttrKind::RedriveAllowPolicyStruct,
                &[
                    "null",
                    "[]",
                    "{}",
                    r#"{"redrivePermission":false}"#,
                    r#"{"redrivePermission":"byQueue","sourceQueueArns":null}"#,
                    r#"{"redrivePermission":"byQueue","sourceQueueArns":"arn"}"#,
                    r#"{"redrivePermission":"byQueue","sourceQueueArns":["arn",1]}"#,
                ][..],
            ),
        ] {
            for raw in inputs {
                assert_attribute_decode_failure(raw, key.clone(), kind, |error| {
                    assert!(matches!(error, SqsAttributeValueError::RedriveShape(_)));
                    assert!(error.source().unwrap().is::<SqsRedriveShapeError>());
                });
            }
        }
        for count in ["not-an-int", "9223372036854775808"] {
            let raw = format!(r#"{{"deadLetterTargetArn":"arn","maxReceiveCount":"{count}"}}"#);
            assert_attribute_decode_failure(
                &raw,
                QueueAttributeName::RedrivePolicy,
                AttrKind::RedrivePolicyStruct,
                |error| {
                    let SqsAttributeValueError::RedriveShape(shape) = error else {
                        panic!("expected a redrive shape error");
                    };
                    assert!(matches!(shape, SqsRedriveShapeError::MaxReceiveCount(_)));
                    assert!(shape.source().unwrap().is::<std::num::ParseIntError>());
                },
            );
        }
    }

    #[test]
    fn sqs_missing_attributes_stay_absent() {
        let id = queue_resource().id;
        let empty_attributes = HashMap::new();
        for (_, key, kind, _) in QUEUE_ATTRS {
            for attributes in [None, Some(&empty_attributes)] {
                assert_eq!(queue_attribute(&id, attributes, key, *kind).unwrap(), None);
            }
        }
    }

    #[test]
    fn sqs_redrive_policy_accepts_string_and_integer_counts() {
        let arn = "arn:aws:sqs:us-east-1:123456789012:dead-letter";
        for count in ["5", r#""5""#] {
            let raw = format!(r#"{{"deadLetterTargetArn":"{arn}","maxReceiveCount":{count}}}"#);
            assert_eq!(
                unpack_attr(&raw, AttrKind::RedrivePolicyStruct).unwrap(),
                Some(map([
                    ("dead_letter_target_arn", string(arn)),
                    ("max_receive_count", Value::Concrete(ConcreteValue::Int(5))),
                ])),
            );
        }
    }

    #[test]
    fn sqs_redrive_allow_policy_preserves_optional_arns() {
        for permission in ["allowAll", "denyAll"] {
            let raw = format!(r#"{{"redrivePermission":"{permission}"}}"#);
            assert_eq!(
                unpack_attr(&raw, AttrKind::RedriveAllowPolicyStruct).unwrap(),
                Some(map([("redrive_permission", string(permission))])),
            );
        }
        for arns in [
            vec![],
            vec![
                "arn:aws:sqs:us-east-1:123456789012:source-0",
                "arn:aws:sqs:us-east-1:123456789012:source-1",
            ],
        ] {
            let expected = map([
                ("redrive_permission", string("byQueue")),
                (
                    "source_queue_arns",
                    Value::Concrete(ConcreteValue::List(
                        arns.iter().map(|arn| string(arn)).collect(),
                    )),
                ),
            ]);
            let raw = serde_json::json!({"redrivePermission": "byQueue", "sourceQueueArns": arns});
            assert_eq!(
                unpack_attr(&raw.to_string(), AttrKind::RedriveAllowPolicyStruct).unwrap(),
                Some(expected),
            );
        }
    }

    #[test]
    fn sqs_empty_policy_is_absent() {
        for kind in [
            AttrKind::IamPolicy,
            AttrKind::RedrivePolicyStruct,
            AttrKind::RedriveAllowPolicyStruct,
        ] {
            assert_eq!(
                unpack_attr("", kind).expect("an empty policy must be treated as absent"),
                None,
            );
        }
    }

    #[test]
    fn sqs_policy_decode_failure_keeps_its_typed_cause() {
        let id = queue_resource().id;
        for raw in [
            " ",
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":1e999}}}]}"#,
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":-1e999}}}]}"#,
        ] {
            let attributes = HashMap::from([(QueueAttributeName::Policy, raw.to_string())]);
            let error = queue_attribute(
                &id,
                Some(&attributes),
                &QueueAttributeName::Policy,
                AttrKind::IamPolicy,
            )
            .expect_err("an invalid policy must fail the read");
            assert_eq!(
                error.detail().context(),
                "Failed to decode SQS queue attribute"
            );
            let cause = error.detail().cause().expect("policy decode cause");
            let decode = cause
                .downcast_ref::<SqsAttributeDecodeError>()
                .expect("typed SQS attribute decode error");
            assert_eq!(decode.attribute, QueueAttributeName::Policy);
            assert_eq!(
                decode.to_string(),
                "failed to decode SQS queue attribute Policy"
            );
            let value_decode = decode
                .source()
                .expect("attribute value decode source")
                .downcast_ref::<SqsAttributeValueError>()
                .expect("typed attribute value decode error");
            assert!(matches!(value_decode, SqsAttributeValueError::Policy(_)));
            let policy_decode = value_decode
                .source()
                .expect("policy decode source")
                .downcast_ref::<IamPolicyDecodeError>()
                .expect("typed policy decode error");
            assert!(matches!(policy_decode, IamPolicyDecodeError::Json(_)));
            assert!(policy_decode.source().unwrap().is::<serde_json::Error>());
        }
    }

    fn queue_resource() -> Resource {
        Resource::with_provider("aws", "sqs.Queue", "test", None)
    }

    fn plan_time_unknown() -> Value {
        Value::resource_ref("upstream", "fifo_queue", vec![]).resolve_root_for_test()
    }

    fn secret_snapshot() -> SnapshotValue {
        carina_core::value::redact_secrets_in_value(&Value::Deferred(DeferredValue::Secret(
            Box::new(Value::Concrete(ConcreteValue::String("historical".into()))).into(),
        )))
        .expect("secret fixture should redact")
    }

    fn string(value: &str) -> Value {
        Value::Concrete(ConcreteValue::String(value.into()))
    }

    fn map(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Concrete(ConcreteValue::Map(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        ))
    }

    fn source_queue_arns(count: usize) -> Value {
        Value::Concrete(ConcreteValue::List(
            (0..count)
                .map(|i| string(&format!("arn:aws:sqs:us-east-1:123456789012:source-{i}")))
                .collect(),
        ))
    }

    fn invalid_cross_field_inputs() -> Vec<(Vec<(&'static str, Value)>, &'static str)> {
        vec![
            (
                vec![
                    ("queue_name", string("private-name")),
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                ],
                "queue_name must end with .fifo",
            ),
            (
                vec![
                    ("queue_name", string("private-name.fifo")),
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(false))),
                ],
                "queue_name must not end with .fifo",
            ),
            (
                vec![("queue_name", string("private-name.fifo"))],
                "queue_name must not end with .fifo",
            ),
            (
                vec![
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                    ("fifo_throughput_limit", string("perMessageGroupId")),
                    ("deduplication_scope", string("queue")),
                ],
                "fifo_throughput_limit requires deduplication_scope = messageGroup",
            ),
            (
                vec![
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                    ("fifo_throughput_limit", string("perMessageGroupId")),
                ],
                "fifo_throughput_limit requires deduplication_scope = messageGroup",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([
                        ("redrive_permission", string("allowAll")),
                        ("source_queue_arns", source_queue_arns(1)),
                    ]),
                )],
                "redrive_allow_policy.source_queue_arns may only be set",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([
                        ("redrive_permission", string("denyAll")),
                        ("source_queue_arns", source_queue_arns(0)),
                    ]),
                )],
                "redrive_allow_policy.source_queue_arns may only be set",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([("source_queue_arns", source_queue_arns(1))]),
                )],
                "redrive_allow_policy.source_queue_arns may only be set",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([
                        ("redrive_permission", string("byQueue")),
                        ("source_queue_arns", source_queue_arns(0)),
                    ]),
                )],
                "redrive_allow_policy.redrive_permission requires redrive_allow_policy.source_queue_arns",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([("redrive_permission", string("byQueue"))]),
                )],
                "redrive_allow_policy.redrive_permission requires redrive_allow_policy.source_queue_arns",
            ),
            (
                vec![(
                    "redrive_allow_policy",
                    map([
                        ("redrive_permission", string("byQueue")),
                        ("source_queue_arns", source_queue_arns(11)),
                    ]),
                )],
                "redrive_allow_policy.source_queue_arns must contain at most 10 items",
            ),
        ]
    }

    #[test]
    fn sqs_cross_field_validation_rejects_known_conflicts() {
        for (attributes, message) in invalid_cross_field_inputs() {
            let mut resource = queue_resource();
            for (name, value) in attributes {
                resource.set_attr(name, value);
            }
            let desired = NativeDesiredNormalization::from_core(&mut resource).unwrap();
            let error = validate_sqs_queue_inputs(&desired.inputs())
                .expect_err("known SQS conflicts must fail before send");
            assert!(matches!(error, ProviderError::InvalidInput(_)));
            assert!(error.to_string().contains(message), "{error:?}");
            assert!(!error.to_string().contains("private-name"));
            assert!(!error.to_string().contains("arn:aws:sqs"));
        }
    }

    #[test]
    fn sqs_cross_field_update_validation_rejects_reconstructed_conflicts() {
        use carina_core::provider::PatchOp;

        for (mut attributes, message) in invalid_cross_field_inputs() {
            let (name, value) = attributes.pop().unwrap();
            let from = crate::test_existing_state(
                queue_resource().id,
                attributes
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
            )
            .to_snapshot();
            let patch = UpdatePatch {
                ops: vec![PatchOp::Add {
                    key: name.into(),
                    value,
                }],
            };
            let to = crate::helpers::UpdateResource::new(&from, &patch);
            let error = validate_sqs_queue_update_inputs(&to)
                .expect_err("updates must validate both historical and patched SQS inputs");
            assert!(matches!(error, ProviderError::InvalidInput(_)));
            assert!(error.to_string().contains(message), "{error:?}");
            assert!(to.materialized_attributes().contains_key(name));
        }
    }

    fn assert_valid_cross_field_inputs(attributes: Vec<(&str, Value)>) {
        let mut resource = queue_resource();
        for (name, value) in attributes {
            resource.set_attr(name, value);
        }
        let original = resource.attributes.clone();
        let mut desired = NativeDesiredNormalization::from_core(&mut resource).unwrap();
        crate::AwsNormalizer
            .normalize_desired_view(&mut desired.view())
            .expect("valid and undecidable combinations must pass plan-time normalization");
        desired
            .finish()
            .expect("validation must preserve opaque leaves");
        assert_eq!(resource.attributes, original);
        let from = crate::test_existing_state(resource.id, HashMap::new()).to_snapshot();
        let patch = UpdatePatch {
            ops: resource
                .attributes
                .into_iter()
                .map(|(key, value)| carina_core::provider::PatchOp::Add { key, value })
                .collect(),
        };
        let to = crate::helpers::UpdateResource::new(&from, &patch);
        validate_sqs_queue_update_inputs(&to).expect("valid update inputs must pass");
    }

    #[test]
    fn sqs_cross_field_validation_accepts_valid_combinations() {
        for (name, fifo) in [
            ("valid", None),
            ("valid", Some(false)),
            ("valid.fifo", Some(true)),
        ] {
            let mut attributes = vec![("queue_name", string(name))];
            if let Some(fifo) = fifo {
                attributes.push(("fifo_queue", Value::Concrete(ConcreteValue::Bool(fifo))));
            }
            assert_valid_cross_field_inputs(attributes);
        }
        for fifo in [false, true] {
            assert_valid_cross_field_inputs(vec![(
                "fifo_queue",
                Value::Concrete(ConcreteValue::Bool(fifo)),
            )]);
        }
        for (limit, scope) in [
            ("perQueue", None),
            ("perQueue", Some("queue")),
            ("perQueue", Some("messageGroup")),
            ("perMessageGroupId", Some("messageGroup")),
        ] {
            let mut attributes = vec![
                ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                ("fifo_throughput_limit", string(limit)),
            ];
            if let Some(scope) = scope {
                attributes.push(("deduplication_scope", string(scope)));
            }
            assert_valid_cross_field_inputs(attributes);
        }
        for permission in ["allowAll", "denyAll"] {
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([("redrive_permission", string(permission))]),
            )]);
        }
        for count in [1, 10] {
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([
                    ("redrive_permission", string("byQueue")),
                    ("source_queue_arns", source_queue_arns(count)),
                ]),
            )]);
        }
    }

    #[test]
    fn sqs_cross_field_validation_accepts_unknown_and_deferred_values() {
        let unknown = plan_time_unknown();
        let deferred = Value::Deferred(DeferredValue::Secret(Box::new(string("opaque")).into()));
        for opaque in [unknown, deferred] {
            for fifo in [false, true] {
                assert_valid_cross_field_inputs(vec![
                    ("queue_name", opaque.clone()),
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(fifo))),
                ]);
            }
            for name in ["valid", "valid.fifo"] {
                assert_valid_cross_field_inputs(vec![
                    ("queue_name", string(name)),
                    ("fifo_queue", opaque.clone()),
                ]);
            }
            assert_valid_cross_field_inputs(vec![
                ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                ("fifo_throughput_limit", string("perMessageGroupId")),
                ("deduplication_scope", opaque.clone()),
            ]);
            assert_valid_cross_field_inputs(vec![
                ("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
                ("fifo_throughput_limit", opaque.clone()),
                ("deduplication_scope", string("queue")),
            ]);
            assert_valid_cross_field_inputs(vec![("redrive_allow_policy", opaque.clone())]);
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([
                    ("redrive_permission", opaque.clone()),
                    ("source_queue_arns", source_queue_arns(11)),
                ]),
            )]);
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([
                    ("redrive_permission", opaque.clone()),
                    ("source_queue_arns", source_queue_arns(0)),
                ]),
            )]);
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([("redrive_permission", opaque.clone())]),
            )]);
            for permission in ["byQueue", "allowAll", "denyAll"] {
                assert_valid_cross_field_inputs(vec![(
                    "redrive_allow_policy",
                    map([
                        ("redrive_permission", string(permission)),
                        ("source_queue_arns", opaque.clone()),
                    ]),
                )]);
            }
            assert_valid_cross_field_inputs(vec![(
                "redrive_allow_policy",
                map([
                    ("redrive_permission", string("byQueue")),
                    (
                        "source_queue_arns",
                        Value::Concrete(ConcreteValue::List(vec![opaque])),
                    ),
                ]),
            )]);
        }
        assert_valid_cross_field_inputs(vec![(
            "redrive_allow_policy",
            map([
                ("redrive_permission", string("byQueue")),
                (
                    "source_queue_arns",
                    Value::Deferred(DeferredValue::Secret(Box::new(source_queue_arns(0)).into())),
                ),
            ]),
        )]);
    }

    #[test]
    fn sqs_cross_field_validation_reads_canonical_enum_api_values() {
        for (scope, valid) in [("queue", false), ("messageGroup", true)] {
            let scope =
                crate::test_rootless_type(carina_aws_types::sqs_queue_deduplication_scope())
                    .canonicalize_state(string(scope))
                    .into_present()
                    .unwrap();
            let throughput =
                crate::test_rootless_type(carina_aws_types::sqs_queue_fifo_throughput_limit())
                    .canonicalize_state(string("perMessageGroupId"))
                    .into_present()
                    .unwrap();
            let mut resource = queue_resource()
                .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(true)))
                .with_attribute("deduplication_scope", scope)
                .with_attribute("fifo_throughput_limit", throughput);
            let desired = NativeDesiredNormalization::from_core(&mut resource).unwrap();
            assert_eq!(validate_sqs_queue_inputs(&desired.inputs()).is_ok(), valid);
        }
        for (permission, valid) in [("allowAll", false), ("denyAll", false), ("byQueue", true)] {
            let policy = crate::test_rootless_type(carina_aws_types::sqs_redrive_allow_policy())
                .canonicalize_state(map([
                    ("redrive_permission", string(permission)),
                    ("source_queue_arns", source_queue_arns(1)),
                ]))
                .into_present()
                .unwrap();
            let mut resource = queue_resource().with_attribute("redrive_allow_policy", policy);
            let desired = NativeDesiredNormalization::from_core(&mut resource).unwrap();
            assert_eq!(validate_sqs_queue_inputs(&desired.inputs()).is_ok(), valid);
        }
    }

    #[test]
    fn fifo_update_validation_leaves_skipped_inputs_historical() {
        let mut from = crate::test_existing_state(
            queue_resource().id,
            HashMap::from([(
                "fifo_queue".into(),
                Value::Concrete(ConcreteValue::Bool(true)),
            )]),
        )
        .to_snapshot();
        for name in [
            "kms_master_key_id",
            "content_based_deduplication",
            "deduplication_scope",
            "fifo_throughput_limit",
        ] {
            from.attributes.insert(name.into(), secret_snapshot());
        }
        let to = crate::helpers::UpdateResource::new(&from, &UpdatePatch { ops: vec![] });

        validate_sqs_queue_update_inputs(&to).expect("skipped validation must not consume secrets");
        let materialized = to.materialized_attributes();
        assert_eq!(materialized.len(), 1);
        assert!(materialized.contains_key("fifo_queue"));
    }

    #[test]
    fn update_validation_stops_at_the_first_fifo_conflict() {
        for (name, value) in [
            (
                "content_based_deduplication",
                Value::Concrete(ConcreteValue::Bool(true)),
            ),
            (
                "deduplication_scope",
                Value::Concrete(ConcreteValue::String("messageGroup".into())),
            ),
        ] {
            let mut from = crate::test_existing_state(
                queue_resource().id,
                HashMap::from([
                    (
                        "fifo_queue".into(),
                        Value::Concrete(ConcreteValue::Bool(false)),
                    ),
                    (name.into(), value),
                ]),
            )
            .to_snapshot();
            from.attributes
                .entry("deduplication_scope".into())
                .or_insert_with(secret_snapshot);
            from.attributes
                .insert("fifo_throughput_limit".into(), secret_snapshot());
            let to = crate::helpers::UpdateResource::new(&from, &UpdatePatch { ops: vec![] });

            let error = validate_sqs_queue_update_inputs(&to)
                .expect_err("a standard queue must reject the first known FIFO-only input");
            assert!(
                error
                    .to_string()
                    .contains(&format!("{name} requires fifo_queue = true"))
            );
            assert!(error.detail().cause().is_none());
            assert!(
                !to.materialized_attributes()
                    .contains_key("fifo_throughput_limit")
            );
        }
    }

    #[test]
    fn update_validation_keeps_consumed_secret_errors_typed() {
        let mut from = crate::test_existing_state(
            queue_resource().id,
            HashMap::from([(
                "fifo_queue".into(),
                Value::Concrete(ConcreteValue::Bool(false)),
            )]),
        )
        .to_snapshot();
        from.attributes
            .insert("deduplication_scope".into(), secret_snapshot());
        let to = crate::helpers::UpdateResource::new(&from, &UpdatePatch { ops: vec![] });

        let error = validate_sqs_queue_update_inputs(&to)
            .expect_err("validation of a standard queue consumes its deduplication scope");
        let cause = error
            .detail()
            .cause()
            .and_then(|cause| cause.downcast_ref::<carina_core::resource::UnavailableSecretError>())
            .expect("consumed historical secrets must keep their typed error");
        assert_eq!(cause.path(), "aws.sqs.Queue.test.deduplication_scope");
    }

    #[test]
    fn unresolved_fifo_queue_defers_fifo_only_validation() {
        let mut resource = queue_resource();
        resource.set_attr("fifo_queue", plan_time_unknown());
        resource.set_attr(
            "deduplication_scope",
            Value::Concrete(ConcreteValue::String("messageGroup".into())),
        );

        let desired = carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource)
            .expect("normalization keeps unresolved values opaque");
        validate_sqs_queue_inputs(&desired.inputs())
            .expect("an unresolved fifo_queue value must not be treated as false");
    }

    #[test]
    fn unresolved_fifo_only_value_is_not_rejected_for_a_standard_queue() {
        let mut resource = queue_resource();
        resource.set_attr("fifo_queue", Value::Concrete(ConcreteValue::Bool(false)));
        resource.set_attr(
            "deduplication_scope",
            Value::resource_ref("upstream", "deduplication_scope", vec![]).resolve_root_for_test(),
        );

        let desired = carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource)
            .expect("normalization keeps unresolved values opaque");
        validate_sqs_queue_inputs(&desired.inputs())
            .expect("a deferred FIFO-only value must wait for resolution");
    }

    #[test]
    fn unresolved_kms_key_is_not_treated_as_absent() {
        let mut resource = queue_resource();
        resource.set_attr(
            "kms_data_key_reuse_period_seconds",
            Value::Concrete(ConcreteValue::Int(300)),
        );
        resource.set_attr("kms_master_key_id", plan_time_unknown());

        let desired = carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource)
            .expect("normalization keeps unresolved values opaque");
        validate_sqs_queue_inputs(&desired.inputs())
            .expect("an unresolved KMS key is present but not yet decidable");
    }

    #[test]
    fn unresolved_kms_reuse_period_defers_missing_key_validation() {
        let mut resource = queue_resource();
        resource.set_attr("kms_data_key_reuse_period_seconds", plan_time_unknown());

        let desired = carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource)
            .expect("normalization keeps unresolved values opaque");
        validate_sqs_queue_inputs(&desired.inputs())
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
        let canonical = schema
            .canonicalize_state(Value::Concrete(ConcreteValue::Map(map)))
            .into_present()
            .expect("redrive-allow-policy fixture must remain present");

        let json = redrive_allow_policy_to_json_string(&canonical).expect("redrive allow JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["redrivePermission"].as_str(), Some("allowAll"));
    }
}
