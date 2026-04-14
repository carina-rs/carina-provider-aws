use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};
use carina_core::utils::extract_enum_value;

use crate::AwsProvider;
use crate::helpers::{PollState, require_string_attr, sdk_error_message, wait_for_ec2_state};

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
            attributes.insert("id".to_string(), Value::String(id_string));
        }
        if let Some(arn) = account.arn() {
            attributes.insert("arn".to_string(), Value::String(arn.to_string()));
        }
        if let Some(name) = account.name() {
            attributes.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(email) = account.email() {
            attributes.insert("email".to_string(), Value::String(email.to_string()));
        }
        if let Some(status) = account.status() {
            attributes.insert(
                "status".to_string(),
                Value::String(status.as_str().to_string()),
            );
        }
        if let Some(joined_method) = account.joined_method() {
            attributes.insert(
                "joined_method".to_string(),
                Value::String(joined_method.as_str().to_string()),
            );
        }
        if let Some(joined_timestamp) = account.joined_timestamp() {
            attributes.insert(
                "joined_timestamp".to_string(),
                Value::String(joined_timestamp.to_string()),
            );
        }

        identifier
    }

    /// Read an Organizations account
    pub(crate) async fn read_organizations_account(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

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
                    let account_id =
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
                            Value::String(parent_id.to_string()),
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
                            let mut tag_map = HashMap::new();
                            for tag in tags {
                                tag_map.insert(
                                    tag.key().to_string(),
                                    Value::String(tag.value().to_string()),
                                );
                            }
                            if !tag_map.is_empty() {
                                attributes.insert("tags".to_string(), Value::Map(tag_map));
                            }
                        }
                    }

                    let state = State::existing(id.clone(), attributes);
                    Ok(if let Some(account_id) = account_id {
                        state.with_identifier(account_id)
                    } else {
                        state
                    })
                } else {
                    Ok(State::not_found(id.clone()))
                }
            }
            Err(e) => {
                if let Some(service_err) = e.as_service_error()
                    && service_err.is_account_not_found_exception()
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::new(sdk_error_message("Failed to describe account", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    /// Create an Organizations account
    pub(crate) async fn create_organizations_account(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let name = require_string_attr(&resource, "account_name")?;
        let email = require_string_attr(&resource, "email")?;

        let mut req = self
            .organizations_client
            .create_account()
            .account_name(&name)
            .email(&email);

        if let Some(Value::String(iam_billing)) = resource.get_attr("iam_user_access_to_billing") {
            let val = aws_sdk_organizations::types::IamUserAccessToBilling::from(
                extract_enum_value(iam_billing),
            );
            req = req.iam_user_access_to_billing(val);
        }

        if let Some(Value::String(role_name)) = resource.get_attr("role_name") {
            req = req.role_name(role_name);
        }

        if let Some(Value::Map(tag_map)) = resource.get_attr("tags") {
            for (key, value) in tag_map {
                if let Value::String(val) = value {
                    let tag = aws_sdk_organizations::types::Tag::builder()
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

        let response = req.send().await.map_err(|e| {
            ProviderError::new(sdk_error_message("Failed to create account", &e))
                .for_resource(resource.id.clone())
        })?;

        let create_status = response.create_account_status().ok_or_else(|| {
            ProviderError::new("CreateAccount returned no status").for_resource(resource.id.clone())
        })?;

        let request_id = create_status.id().ok_or_else(|| {
            ProviderError::new("CreateAccount returned no request ID")
                .for_resource(resource.id.clone())
        })?;

        // Poll for completion
        let resource_id = resource.id.clone();
        let request_id_owned = request_id.to_string();
        wait_for_ec2_state(
            &resource_id,
            || async {
                let result = self
                    .organizations_client
                    .describe_create_account_status()
                    .create_account_request_id(&request_id_owned)
                    .send()
                    .await
                    .map_err(|e| {
                        ProviderError::new(sdk_error_message(
                            "Failed to describe create account status",
                            &e,
                        ))
                        .for_resource(resource_id.clone())
                    })?;
                if let Some(status) = result.create_account_status() {
                    match status.state() {
                        Some(aws_sdk_organizations::types::CreateAccountState::Succeeded) => {
                            Ok(PollState::Ready)
                        }
                        Some(aws_sdk_organizations::types::CreateAccountState::Failed) => {
                            Ok(PollState::Failed)
                        }
                        _ => Ok(PollState::Pending),
                    }
                } else {
                    Ok(PollState::Pending)
                }
            },
            120,
            "Timeout waiting for account creation to complete",
            "Account creation failed",
        )
        .await?;

        // Get the account ID from the final status
        let final_status = self
            .organizations_client
            .describe_create_account_status()
            .create_account_request_id(request_id)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message(
                    "Failed to get final create account status",
                    &e,
                ))
                .for_resource(resource.id.clone())
            })?;

        let account_id = final_status
            .create_account_status()
            .and_then(|s| s.account_id())
            .ok_or_else(|| {
                ProviderError::new("CreateAccount succeeded but no account ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Move to parent OU if specified
        if let Some(Value::String(parent_id)) = resource.get_attr("parent_id") {
            // Get current parent (root)
            let parents_response = self
                .organizations_client
                .list_parents()
                .child_id(account_id)
                .send()
                .await
                .map_err(|e| {
                    ProviderError::new(sdk_error_message(
                        "Failed to list parents for new account",
                        &e,
                    ))
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
                    .destination_parent_id(parent_id)
                    .send()
                    .await
                    .map_err(|e| {
                        ProviderError::new(sdk_error_message(
                            "Failed to move account to parent",
                            &e,
                        ))
                        .for_resource(resource.id.clone())
                    })?;
            }
        }

        self.read_organizations_account(&resource.id, Some(account_id))
            .await
    }

    /// Update an Organizations account
    pub(crate) async fn update_organizations_account(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        // Handle parent_id change via MoveAccount
        let desired_parent = to.get_attr("parent_id").and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        });
        let current_parent = from.attributes.get("parent_id").and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        });

        if desired_parent != current_parent
            && let (Some(dest), Some(src)) = (desired_parent, current_parent)
        {
            self.organizations_client
                .move_account()
                .account_id(identifier)
                .source_parent_id(src)
                .destination_parent_id(dest)
                .send()
                .await
                .map_err(|e| {
                    ProviderError::new(sdk_error_message("Failed to move account", &e))
                        .for_resource(id.clone())
                })?;
        }

        // Handle tag changes
        self.apply_organizations_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        self.read_organizations_account(&id, Some(identifier)).await
    }

    /// Delete (close) an Organizations account
    pub(crate) async fn delete_organizations_account(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.organizations_client
            .close_account()
            .account_id(identifier)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to close account", &e))
                    .for_resource(id.clone())
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
            Some(Value::Map(m)) => m.clone(),
            _ => HashMap::new(),
        };
        let current_tags = match current.and_then(|c| c.get("tags")) {
            Some(Value::Map(m)) => m.clone(),
            _ => HashMap::new(),
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
                    ProviderError::new(sdk_error_message("Failed to untag account", &e))
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
                    let tag = aws_sdk_organizations::types::Tag::builder()
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
            self.organizations_client
                .tag_resource()
                .resource_id(resource_id)
                .set_tags(Some(tags_to_add))
                .send()
                .await
                .map_err(|e| {
                    ProviderError::new(sdk_error_message("Failed to tag account", &e))
                        .for_resource(id.clone())
                })?;
        }

        Ok(())
    }
}
