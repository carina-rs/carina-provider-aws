use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ProviderResult, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{optional_enum_attr, require_string_attr};
use crate::provider::AwsCreateAttempt;
use crate::read_model::found_managed_read;
use crate::read_models_generated::LogsLogGroupReadModelSupplement;

impl AwsProvider {
    /// Read a CloudWatch Logs Log Group
    pub(crate) async fn read_logs_log_group(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let result = self
            .logs_client
            .describe_log_groups()
            .log_group_name_prefix(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe log groups", e).for_resource(id.clone())
            })?;

        // Find exact match (prefix search can return multiple results)
        if let Some(lg) = result
            .log_groups()
            .iter()
            .find(|lg| lg.log_group_name() == Some(identifier))
        {
            let fields = Self::extract_logs_log_group_attributes(lg);

            // Read tags separately. Empty tags are canonical state, and a
            // failed side read must not erase configured tags.
            #[allow(deprecated)]
            let tags_output = self
                .logs_client
                .list_tags_log_group()
                .log_group_name(identifier)
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta(
                        "ListTagsLogGroup failed; ensure logs:ListTagsLogGroup is allowed",
                        error,
                    )
                    .for_resource(id.clone())
                })?;
            let mut tag_map = IndexMap::new();
            if let Some(tags) = tags_output.tags() {
                for (key, val) in tags {
                    tag_map.insert(
                        key.to_string(),
                        Value::Concrete(ConcreteValue::String(val.to_string().into())),
                    );
                }
            }

            // Measured with `aws logs describe-log-groups --log-group-name-prefix <name>
            // --region ap-northeast-1` on 2026-10-29: the response omits
            // deletionProtectionEnabled when it is false.
            let fields = fields.map_deletion_protection_enabled(|value| {
                Some(value.unwrap_or_else(|| Value::Concrete(ConcreteValue::Bool(false))))
            });
            let (_, model) = fields.into_model(
                id,
                LogsLogGroupReadModelSupplement {
                    tags: Value::Concrete(ConcreteValue::Map(tag_map)),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create a CloudWatch Logs Log Group
    pub(crate) async fn create_logs_log_group<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let log_group_name = match require_string_attr(resource, "log_group_name") {
            Ok(log_group_name) => log_group_name,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let identifier = match crate::helpers::applied_identifier(
            &resource.id,
            "logs.CreateLogGroup",
            log_group_name.clone(),
        ) {
            Ok(identifier) => identifier,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let mut req = self
            .logs_client
            .create_log_group()
            .log_group_name(&log_group_name);

        if let Some(Value::Concrete(ConcreteValue::Bool(enabled))) =
            resource.get_attr("deletion_protection_enabled")
        {
            req = req.deletion_protection_enabled(*enabled);
        }

        if let Some(Value::Concrete(ConcreteValue::String(kms_key))) =
            resource.get_attr("kms_key_id")
        {
            req = req.kms_key_id(kms_key.expose_plaintext());
        }

        let log_group_class = match optional_enum_attr(resource, schema, "log_group_class") {
            Ok(log_group_class) => log_group_class,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(class) = log_group_class {
            use aws_sdk_cloudwatchlogs::types::LogGroupClass;
            req = req.log_group_class(LogGroupClass::from(class));
        }

        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") {
            let mut tags = HashMap::new();
            for (key, value) in tag_map {
                if let Value::Concrete(ConcreteValue::String(val)) = value {
                    tags.insert(key.clone(), val.expose_plaintext().to_string());
                }
            }
            req = req.set_tags(Some(tags));
        }

        if let Err(error) = req.send().await {
            return Err(attempt.untracked(
                api_error_with_meta("Failed to create log group", error)
                    .for_resource(resource.id.clone()),
            ));
        }
        let issued = attempt.issued(
            identifier.clone(),
            self.sent_only(
                resource,
                &[
                    "log_group_name",
                    "kms_key_id",
                    "log_group_class",
                    "deletion_protection_enabled",
                    "tags",
                ],
            ),
        );

        // Set retention if specified
        if let Some(Value::Concrete(ConcreteValue::Int(days))) =
            resource.get_attr("retention_in_days")
        {
            let result = self
                .logs_client
                .put_retention_policy()
                .log_group_name(&log_group_name)
                .retention_in_days(*days as i32)
                .send()
                .await;
            if let Err(error) = result {
                let error = api_error_with_meta("Failed to set retention policy", error)
                    .for_resource(resource.id.clone());
                return Err(self.failed_issued(&resource.id, issued, error).await);
            }
        }

        let outcome = self.read_logs_log_group(&resource.id, &identifier).await;
        let completion = self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        );
        self.finish_issued(issued, completion)
    }

    /// Update a CloudWatch Logs Log Group
    pub(crate) async fn update_logs_log_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // Update retention
        match to.get_attr("retention_in_days") {
            Some(Value::Concrete(ConcreteValue::Int(days))) => {
                self.logs_client
                    .put_retention_policy()
                    .log_group_name(identifier.as_str())
                    .retention_in_days(*days as i32)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta("Failed to set retention policy", e)
                            .for_resource(id.clone())
                    })?;
            }
            None if from.attributes.contains_key("retention_in_days") => {
                // If retention was previously set but now removed, delete the policy
                self.logs_client
                    .delete_retention_policy()
                    .log_group_name(identifier.as_str())
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta("Failed to delete retention policy", e)
                            .for_resource(id.clone())
                    })?;
            }
            _ => {}
        }

        // Update KMS key
        if let Some(Value::Concrete(ConcreteValue::String(kms_key))) = to.get_attr("kms_key_id") {
            self.logs_client
                .associate_kms_key()
                .log_group_name(identifier.as_str())
                .kms_key_id(kms_key.expose_plaintext())
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to associate KMS key", e).for_resource(id.clone())
                })?;
        } else if from.attributes.contains_key("kms_key_id") {
            self.logs_client
                .disassociate_kms_key()
                .log_group_name(identifier.as_str())
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to disassociate KMS key", e)
                        .for_resource(id.clone())
                })?;
        }

        let current_deletion_protection = match from.attributes.get("deletion_protection_enabled") {
            Some(Value::Concrete(ConcreteValue::Bool(enabled))) => Some(*enabled),
            _ => None,
        };
        let desired_deletion_protection = match to.get_attr("deletion_protection_enabled") {
            Some(Value::Concrete(ConcreteValue::Bool(enabled))) => Some(*enabled),
            _ => None,
        };
        if let Some(enabled) = desired_deletion_protection
            && desired_deletion_protection != current_deletion_protection
        {
            self.logs_client
                .put_log_group_deletion_protection()
                .log_group_identifier(identifier.as_str())
                .deletion_protection_enabled(enabled)
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta("Failed to update log group deletion protection", error)
                        .for_resource(id.clone())
                })?;
        }

        // Update tags
        self.apply_logs_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        Ok(self.read_logs_log_group(&id, identifier).await)
    }

    /// Delete a CloudWatch Logs Log Group
    pub(crate) async fn delete_logs_log_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.logs_client
            .delete_log_group()
            .log_group_name(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete log group", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Apply CloudWatch Logs tags
    async fn apply_logs_tags(
        &self,
        id: &ResourceId,
        log_group_name: &str,
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
            #[allow(deprecated)]
            let mut req = self
                .logs_client
                .untag_log_group()
                .log_group_name(log_group_name);
            for key in &keys_to_remove {
                req = req.tags(key);
            }
            req.send().await.map_err(|e| {
                api_error_with_meta("Failed to untag log group", e).for_resource(id.clone())
            })?;
        }

        // Tags to add/update
        let mut tags_to_add = HashMap::new();
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
            #[allow(deprecated)]
            self.logs_client
                .tag_log_group()
                .log_group_name(log_group_name)
                .set_tags(Some(tags_to_add))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to tag log group", e).for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}
