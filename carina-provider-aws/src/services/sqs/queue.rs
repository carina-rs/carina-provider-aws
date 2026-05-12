//! Hand-written service implementation for `aws.sqs.Queue`.
//!
//! SQS exposes its mutable knobs through `Attributes: Map<QueueAttributeName,
//! String>` on `CreateQueue` / `GetQueueAttributes` / `SetQueueAttributes`.
//! Schema-side we project the supported subset (visibility_timeout,
//! message_retention_period, delay_seconds, maximum_message_size, queue_arn)
//! as flat top-level attributes via `extra_attributes` (see
//! `carina-codegen-aws/src/resource_defs.rs::sqs_resources`); the
//! pack/unpack lives here.

use aws_sdk_sqs::types::QueueAttributeName;
use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};

/// Writable numeric attributes mapped 1:1 to SQS `QueueAttributeName`
/// keys. Each entry is (DSL snake-case attribute name, SDK enum key).
const WRITABLE_INT_ATTRS: &[(&str, QueueAttributeName)] = &[
    ("visibility_timeout", QueueAttributeName::VisibilityTimeout),
    (
        "message_retention_period",
        QueueAttributeName::MessageRetentionPeriod,
    ),
    ("delay_seconds", QueueAttributeName::DelaySeconds),
    (
        "maximum_message_size",
        QueueAttributeName::MaximumMessageSize,
    ),
];

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

        // Pull every attribute we surface in one round-trip.
        let mut attr_names: Vec<QueueAttributeName> =
            WRITABLE_INT_ATTRS.iter().map(|(_, n)| n.clone()).collect();
        attr_names.push(QueueAttributeName::QueueArn);

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
                // Service treats a missing queue with a typed error variant.
                if let Some(svc_err) = err.as_service_error()
                    && svc_err.is_queue_does_not_exist()
                {
                    return Ok(State::not_found(id.clone()));
                }
                return Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get queue attributes",
                    &err,
                ))
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
            for (attr_name, sdk_key) in WRITABLE_INT_ATTRS {
                if let Some(raw) = map.get(sdk_key)
                    && let Ok(parsed) = raw.parse::<i64>()
                {
                    attributes.insert(
                        attr_name.to_string(),
                        Value::Concrete(ConcreteValue::Int(parsed)),
                    );
                }
            }
            if let Some(arn) = map.get(&QueueAttributeName::QueueArn) {
                attributes.insert(
                    "queue_arn".to_string(),
                    Value::Concrete(ConcreteValue::String(arn.to_string())),
                );
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
    pub(crate) async fn create_sqs_queue(&self, resource: Resource) -> ProviderResult<State> {
        let queue_name = require_string_attr(&resource, "queue_name")?;

        // Pack writable numeric attributes into the SQS map shape.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key) in WRITABLE_INT_ATTRS {
            if let Some(Value::Concrete(ConcreteValue::Int(n))) = resource.get_attr(attr_name) {
                attr_map.insert(sdk_key.clone(), n.to_string());
            }
        }

        // Tags are passed as a separate parameter.
        let tags = resource_tags_map(&resource);

        let mut req = self.sqs_client.create_queue().queue_name(&queue_name);
        if !attr_map.is_empty() {
            req = req.set_attributes(Some(attr_map));
        }
        if let Some(tags) = tags {
            req = req.set_tags(Some(tags));
        }

        let result = req.send().await.map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to create SQS queue", &e))
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
    ) -> ProviderResult<State> {
        // Build the partial map of attributes that actually changed.
        let mut attr_map: HashMap<QueueAttributeName, String> = HashMap::new();
        for (attr_name, sdk_key) in WRITABLE_INT_ATTRS {
            let desired = to.get_attr(attr_name);
            let current = from.attributes.get(*attr_name);
            if let Some(Value::Concrete(ConcreteValue::Int(n))) = desired
                && current != desired
            {
                attr_map.insert(sdk_key.clone(), n.to_string());
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
                    ProviderError::api_error(sdk_error_message(
                        "Failed to set queue attributes",
                        &e,
                    ))
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
                ProviderError::api_error(sdk_error_message("Failed to delete SQS queue", &e))
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
                    ProviderError::api_error(sdk_error_message("Failed to untag SQS queue", &e))
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
                    ProviderError::api_error(sdk_error_message("Failed to tag SQS queue", &e))
                        .for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}

/// Extract a `tags = { ... }` block from the resource, dropping any
/// non-string values. Returns `None` when no tags are set so callers
/// can leave the SDK builder untouched.
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
