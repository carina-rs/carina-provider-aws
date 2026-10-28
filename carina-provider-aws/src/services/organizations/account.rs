use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{CreatePoll, optional_enum_attr, require_string_attr, sdk_error_message};
use crate::provider::CreateCompletion;

fn organizations_account_create_poll_state(
    status: Option<&aws_sdk_organizations::types::CreateAccountStatus>,
) -> CreatePoll<Option<String>> {
    match status.and_then(|status| status.state()) {
        Some(aws_sdk_organizations::types::CreateAccountState::Succeeded) => CreatePoll::Ready(
            status
                .and_then(|status| status.account_id())
                .map(str::to_string),
        ),
        Some(aws_sdk_organizations::types::CreateAccountState::Failed) => CreatePoll::Failed,
        Some(aws_sdk_organizations::types::CreateAccountState::InProgress) | None => {
            CreatePoll::Pending
        }
        // The SDK enum is non-exhaustive. An unrecognized state is not proof
        // that account creation ended, so retain the continuation.
        Some(_unrecognized_state) => CreatePoll::Pending,
    }
}

impl AwsProvider {
    /// Extract attributes from an Organizations Account object
    pub(crate) fn extract_organizations_account_attributes(
        account: &aws_sdk_organizations::types::Account,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        let mut identifier = None;

        if let Some(id) = account.id() {
            let id_string = id.to_string();
            identifier = Some(id_string.clone());
            attributes.insert(
                "id".to_string(),
                Value::Concrete(ConcreteValue::String(id_string.into())),
            );
        }
        if let Some(arn) = account.arn() {
            attributes.insert(
                "arn".to_string(),
                Value::Concrete(ConcreteValue::String(arn.to_string().into())),
            );
        }
        if let Some(name) = account.name() {
            attributes.insert(
                "name".to_string(),
                Value::Concrete(ConcreteValue::String(name.to_string().into())),
            );
        }
        if let Some(email) = account.email() {
            attributes.insert(
                "email".to_string(),
                Value::Concrete(ConcreteValue::String(email.to_string().into())),
            );
        }
        if let Some(status) = account.status() {
            attributes.insert(
                "status".to_string(),
                Value::Concrete(ConcreteValue::String(status.as_str().to_string().into())),
            );
        }
        if let Some(joined_method) = account.joined_method() {
            attributes.insert(
                "joined_method".to_string(),
                Value::Concrete(ConcreteValue::String(
                    joined_method.as_str().to_string().into(),
                )),
            );
        }
        if let Some(joined_timestamp) = account.joined_timestamp() {
            attributes.insert(
                "joined_timestamp".to_string(),
                Value::Concrete(ConcreteValue::String(joined_timestamp.to_string().into())),
            );
        }

        identifier
    }

    /// Read an Organizations account
    pub(crate) async fn read_organizations_account(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // DescribeAccount
        let account_result = self
            .organizations_client
            .describe_account()
            .account_id(identifier)
            .send()
            .await;

        match account_result {
            Ok(response) => {
                if let Some(account) = response.account() {
                    let mut attributes = HashMap::new();
                    let _account_id =
                        Self::extract_organizations_account_attributes(account, &mut attributes);

                    // ListParents for parent_id
                    if let Ok(parents_response) = self
                        .organizations_client
                        .list_parents()
                        .child_id(identifier)
                        .send()
                        .await
                        && let Some(parent) = parents_response.parents().first()
                        && let Some(parent_id) = parent.id()
                    {
                        attributes.insert(
                            "parent_id".to_string(),
                            Value::Concrete(ConcreteValue::String(parent_id.to_string().into())),
                        );
                    }

                    // ListTagsForResource for tags
                    if let Ok(tags_response) = self
                        .organizations_client
                        .list_tags_for_resource()
                        .resource_id(identifier)
                        .send()
                        .await
                    {
                        let tags = tags_response.tags();
                        if !tags.is_empty() {
                            let mut tag_map = IndexMap::new();
                            for tag in tags {
                                tag_map.insert(
                                    tag.key().to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        tag.value().to_string().into(),
                                    )),
                                );
                            }
                            if !tag_map.is_empty() {
                                attributes.insert(
                                    "tags".to_string(),
                                    Value::Concrete(ConcreteValue::Map(tag_map)),
                                );
                            }
                        }
                    }

                    Ok(crate::helpers::found_read(
                        id,
                        applied_identifier,
                        attributes,
                    ))
                } else {
                    Ok(ReadOutcome::not_found(id.clone()))
                }
            }
            Err(e) => {
                if let Some(service_err) = e.as_service_error()
                    && service_err.is_account_not_found_exception()
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to describe account", e).for_resource(id.clone()))
            }
        }
    }

    /// Create an Organizations account
    pub(crate) async fn create_organizations_account_start(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<String> {
        let name = require_string_attr(resource, "account_name")?;
        let email = require_string_attr(resource, "email")?;

        let mut req = self
            .organizations_client
            .create_account()
            .account_name(&name)
            .email(&email);

        if let Some(iam_billing) =
            optional_enum_attr(resource, schema, "iam_user_access_to_billing")?
        {
            let val = aws_sdk_organizations::types::IamUserAccessToBilling::from(iam_billing);
            req = req.iam_user_access_to_billing(val);
        }

        if let Some(Value::Concrete(ConcreteValue::String(role_name))) =
            resource.get_attr("role_name")
        {
            req = req.role_name(role_name.expose_plaintext());
        }

        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") {
            for (key, value) in tag_map {
                if let Value::Concrete(ConcreteValue::String(val)) = value {
                    let tag = aws_sdk_organizations::types::Tag::builder()
                        .key(key)
                        .value(val.expose_plaintext())
                        .build()
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message("Failed to build tag", &e))
                                .for_resource(resource.id.clone())
                        })?;
                    req = req.tags(tag);
                }
            }
        }

        let response = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create account", e).for_resource(resource.id.clone())
        })?;

        let create_status = response.create_account_status().ok_or_else(|| {
            ProviderError::api_error("CreateAccount returned no status")
                .for_resource(resource.id.clone())
        })?;

        let request_id = create_status.id().ok_or_else(|| {
            ProviderError::api_error("CreateAccount returned no request ID")
                .for_resource(resource.id.clone())
        })?;

        Ok(request_id.to_string())
    }

    /// Perform one describe call for a pending Organizations account create.
    pub(crate) async fn poll_organizations_account_create(
        &self,
        id: &ResourceId,
        request_id: &str,
    ) -> ProviderResult<CreatePoll<Option<String>>> {
        let result = self
            .organizations_client
            .describe_create_account_status()
            .create_account_request_id(request_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe create account status", e)
                    .for_resource(id.clone())
            })?;
        Ok(organizations_account_create_poll_state(
            result.create_account_status(),
        ))
    }

    /// Complete post-create handling after the account status becomes ready.
    pub(crate) async fn finish_organizations_account_create(
        &self,
        resource: &Resource,
        account_id: &str,
    ) -> ProviderResult<CreateCompletion> {
        // Move to parent OU if specified
        if let Some(Value::Concrete(ConcreteValue::String(parent_id))) =
            resource.get_attr("parent_id")
        {
            // Get current parent (root)
            let parents_response = self
                .organizations_client
                .list_parents()
                .child_id(account_id)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to list parents for new account", e)
                        .for_resource(resource.id.clone())
                })?;

            if let Some(current_parent) = parents_response.parents().first()
                && let Some(source_parent_id) = current_parent.id()
                && source_parent_id != parent_id
            {
                self.organizations_client
                    .move_account()
                    .account_id(account_id)
                    .source_parent_id(source_parent_id)
                    .destination_parent_id(parent_id.expose_plaintext())
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta("Failed to move account to parent", e)
                            .for_resource(resource.id.clone())
                    })?;
            }
        }

        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "organizations.CreateAccount",
            account_id,
        )?;
        let outcome = self
            .read_organizations_account(&resource.id, &identifier)
            .await;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    /// Update an Organizations account
    pub(crate) async fn update_organizations_account(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // Handle parent_id change via MoveAccount
        let desired_parent = to.get_attr("parent_id").and_then(|v| match v {
            Value::Concrete(ConcreteValue::String(s)) => Some(s.expose_plaintext()),
            _ => None,
        });
        let current_parent = from.attributes.get("parent_id").and_then(|v| match v {
            Value::Concrete(ConcreteValue::String(s)) => Some(s.expose_plaintext()),
            _ => None,
        });

        if desired_parent != current_parent
            && let (Some(dest), Some(src)) = (desired_parent, current_parent)
        {
            self.organizations_client
                .move_account()
                .account_id(identifier.as_str())
                .source_parent_id(src)
                .destination_parent_id(dest)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to move account", e).for_resource(id.clone())
                })?;
        }

        // Handle tag changes
        self.apply_organizations_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        Ok(self.read_organizations_account(&id, identifier).await)
    }

    /// Delete (close) an Organizations account
    pub(crate) async fn delete_organizations_account(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.organizations_client
            .close_account()
            .account_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to close account", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Apply Organizations tags (create/delete tag differences)
    async fn apply_organizations_tags(
        &self,
        id: &ResourceId,
        resource_id: &str,
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
            self.organizations_client
                .untag_resource()
                .resource_id(resource_id)
                .set_tag_keys(Some(keys_to_remove))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to untag account", e).for_resource(id.clone())
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
                    let tag = aws_sdk_organizations::types::Tag::builder()
                        .key(key)
                        .value(val.expose_plaintext())
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
            self.organizations_client
                .tag_resource()
                .resource_id(resource_id)
                .set_tags(Some(tags_to_add))
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to tag account", e).for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;
    use aws_sdk_organizations::types::{CreateAccountState, CreateAccountStatus};

    fn status(state: CreateAccountState, account_id: Option<&str>) -> CreateAccountStatus {
        CreateAccountStatus::builder()
            .state(state)
            .set_account_id(account_id.map(str::to_string))
            .build()
    }

    #[test]
    fn create_poll_maps_pending_ready_and_failed_states() {
        assert_eq!(
            organizations_account_create_poll_state(None),
            CreatePoll::Pending
        );
        assert_eq!(
            organizations_account_create_poll_state(Some(&status(
                CreateAccountState::InProgress,
                None,
            ))),
            CreatePoll::Pending
        );
        assert_eq!(
            organizations_account_create_poll_state(Some(&status(
                CreateAccountState::Succeeded,
                Some("123456789012"),
            ))),
            CreatePoll::Ready(Some("123456789012".to_string()))
        );
        assert_eq!(
            organizations_account_create_poll_state(Some(&status(
                CreateAccountState::Failed,
                None,
            ))),
            CreatePoll::Failed
        );
    }

    #[test]
    fn create_poll_preserves_succeeded_status_without_account_id_for_classification() {
        assert_eq!(
            organizations_account_create_poll_state(Some(&status(
                CreateAccountState::Succeeded,
                None,
            ))),
            CreatePoll::Ready(None)
        );
    }
}
