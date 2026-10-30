use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{CreatePoll, sdk_error_message};
use crate::provider::CreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::OrganizationsAccountReadModelSupplement;
use crate::send_models_generated::{
    OrganizationsAccountCreateInput, OrganizationsAccountUpdateInput,
};

const ACCOUNT_RENAME_SETUP_ERROR: &str = "Failed to rename account: AccessDenied can mean either a missing IAM permission account:PutAccountName or that Account Management trusted access is not enabled. Enable trusted access with `aws organizations enable-aws-service-access --service-principal account.amazonaws.com`";

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
                    let fields = Self::extract_organizations_account_attributes(account);

                    let tags_response = self
                        .organizations_client
                        .list_tags_for_resource()
                        .resource_id(identifier)
                        .send()
                        .await
                        .map_err(|error| {
                            api_error_with_meta("Failed to list account tags", error)
                                .for_resource(id.clone())
                        })?;
                    let mut tag_map = IndexMap::new();
                    for tag in tags_response.tags() {
                        tag_map.insert(
                            tag.key().to_string(),
                            Value::Concrete(ConcreteValue::String(tag.value().to_string().into())),
                        );
                    }

                    let joined_timestamp = account.joined_timestamp().map(|timestamp| {
                        Value::Concrete(ConcreteValue::String(timestamp.to_string().into()))
                    });
                    let (_, model) = fields.into_model(
                        id,
                        OrganizationsAccountReadModelSupplement {
                            joined_timestamp,
                            tags: Value::Concrete(ConcreteValue::Map(tag_map)),
                        },
                    )?;
                    found_managed_read(id, applied_identifier, model)
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
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<(String, carina_core::provider::Sent)> {
        let input = OrganizationsAccountCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::OrganizationsAccountCreateParts {
            account_name: name,
            email,
            iam_user_access_to_billing,
            role_name,
            tags,
        } = input.into_parts();
        let name = name.into_inner();
        let email = email.into_inner();
        let iam_user_access_to_billing = iam_user_access_to_billing.into_inner();
        let role_name = role_name.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);

        let mut req = self
            .organizations_client
            .create_account()
            .account_name(&name)
            .email(&email);

        if let Some(iam_billing) = iam_user_access_to_billing {
            let val =
                aws_sdk_organizations::types::IamUserAccessToBilling::from(iam_billing.as_str());
            req = req.iam_user_access_to_billing(val);
        }

        if let Some(role_name) = role_name {
            req = req.role_name(role_name);
        }

        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = tags.as_ref() {
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

        Ok((request_id.to_string(), sent))
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
        resource: &Resource<impl Sized>,
        account_id: &str,
    ) -> ProviderResult<CreateCompletion> {
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
        let input = OrganizationsAccountUpdateInput::decode(&to)?;
        let crate::send_models_generated::OrganizationsAccountUpdateParts { account_name, tags } =
            input.into_parts();
        let account_name = account_name.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let desired_account_name =
            Value::Concrete(ConcreteValue::String(account_name.clone().into()));
        if from.attributes.get("account_name") != Some(&desired_account_name) {
            let organization = self
                .organizations_client
                .describe_organization()
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta(
                        "Failed to determine the management account before renaming account",
                        error,
                    )
                    .for_resource(id.clone())
                })?;
            let management_account_id = organization
                .organization()
                .and_then(|organization| organization.master_account_id())
                .ok_or_else(|| {
                    ProviderError::api_error(
                        "DescribeOrganization returned no management account ID",
                    )
                    .for_resource(id.clone())
                })?;

            let mut request = self
                .account_client
                .put_account_name()
                .account_name(account_name);
            if identifier.as_str() != management_account_id {
                request = request.account_id(identifier.as_str());
            }
            request.send().await.map_err(|error| {
                let requires_setup = error
                    .as_service_error()
                    .is_some_and(|service_error| service_error.is_access_denied_exception());
                let context = if requires_setup {
                    ACCOUNT_RENAME_SETUP_ERROR
                } else {
                    "Failed to rename account"
                };
                api_error_with_meta(context, error).for_resource(id.clone())
            })?;
        }

        // Handle tag changes
        self.apply_organizations_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
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
