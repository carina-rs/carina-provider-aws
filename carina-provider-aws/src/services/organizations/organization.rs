use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;

impl AwsProvider {
    /// Extract attributes from an Organizations Organization object
    pub(crate) fn extract_organizations_organization_attributes(
        org: &aws_sdk_organizations::types::Organization,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        let mut identifier = None;

        if let Some(id) = org.id() {
            let id_string = id.to_string();
            identifier = Some(id_string.clone());
            attributes.insert(
                "id".to_string(),
                Value::Concrete(ConcreteValue::String(id_string)),
            );
        }
        if let Some(arn) = org.arn() {
            attributes.insert(
                "arn".to_string(),
                Value::Concrete(ConcreteValue::String(arn.to_string())),
            );
        }
        if let Some(feature_set) = org.feature_set() {
            attributes.insert(
                "feature_set".to_string(),
                Value::Concrete(ConcreteValue::String(feature_set.as_str().to_string())),
            );
        }
        if let Some(master_account_id) = org.master_account_id() {
            attributes.insert(
                "master_account_id".to_string(),
                Value::Concrete(ConcreteValue::String(master_account_id.to_string())),
            );
        }
        if let Some(master_account_arn) = org.master_account_arn() {
            attributes.insert(
                "master_account_arn".to_string(),
                Value::Concrete(ConcreteValue::String(master_account_arn.to_string())),
            );
        }
        if let Some(master_account_email) = org.master_account_email() {
            attributes.insert(
                "master_account_email".to_string(),
                Value::Concrete(ConcreteValue::String(master_account_email.to_string())),
            );
        }

        identifier
    }

    /// Read an Organizations organization
    pub(crate) async fn read_organizations_organization(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(_identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        match self
            .organizations_client
            .describe_organization()
            .send()
            .await
        {
            Ok(response) => {
                if let Some(org) = response.organization() {
                    let mut attributes = HashMap::new();
                    let org_id =
                        Self::extract_organizations_organization_attributes(org, &mut attributes);
                    let state = State::existing(id.clone(), attributes);
                    Ok(if let Some(org_id) = org_id {
                        state.with_identifier(org_id)
                    } else {
                        state
                    })
                } else {
                    Ok(State::not_found(id.clone()))
                }
            }
            Err(e) => {
                if let Some(service_err) = e.as_service_error()
                    && service_err.is_aws_organizations_not_in_use_exception()
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to describe organization",
                    "organizations.DescribeOrganization",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    /// Create an Organizations organization
    pub(crate) async fn create_organizations_organization(
        &self,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let mut req = self.organizations_client.create_organization();

        if let Some(Value::Concrete(ConcreteValue::String(feature_set))) =
            resource.get_attr("feature_set")
        {
            let fs =
                aws_sdk_organizations::types::OrganizationFeatureSet::from(feature_set.as_str());
            req = req.feature_set(fs);
        }

        let response = req.send().await.map_err(|e| {
            api_error_with_meta(
                "Failed to create organization",
                "organizations.CreateOrganization",
                e,
            )
            .for_resource(resource.id.clone())
        })?;

        if let Some(org) = response.organization() {
            let mut attributes = HashMap::new();
            let org_id = Self::extract_organizations_organization_attributes(org, &mut attributes);
            let state = State::existing(resource.id.clone(), attributes);
            Ok(if let Some(org_id) = org_id {
                state.with_identifier(org_id)
            } else {
                state
            })
        } else {
            Err(
                ProviderError::api_error("CreateOrganization returned no organization")
                    .for_resource(resource.id.clone()),
            )
        }
    }

    /// Delete an Organizations organization
    pub(crate) async fn delete_organizations_organization(
        &self,
        id: ResourceId,
        _identifier: &str,
    ) -> ProviderResult<()> {
        self.organizations_client
            .delete_organization()
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to delete organization",
                    "organizations.DeleteOrganization",
                    e,
                )
                .for_resource(id.clone())
            })?;
        Ok(())
    }
}
