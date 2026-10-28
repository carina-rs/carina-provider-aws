use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::optional_enum_attr;
use crate::provider::CreateCompletion;

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
                Value::Concrete(ConcreteValue::String(id_string.into())),
            );
        }
        if let Some(arn) = org.arn() {
            attributes.insert(
                "arn".to_string(),
                Value::Concrete(ConcreteValue::String(arn.to_string().into())),
            );
        }
        if let Some(feature_set) = org.feature_set() {
            attributes.insert(
                "feature_set".to_string(),
                Value::Concrete(ConcreteValue::String(
                    feature_set.as_str().to_string().into(),
                )),
            );
        }
        if let Some(master_account_id) = org.master_account_id() {
            attributes.insert(
                "master_account_id".to_string(),
                Value::Concrete(ConcreteValue::String(master_account_id.to_string().into())),
            );
        }
        if let Some(master_account_arn) = org.master_account_arn() {
            attributes.insert(
                "master_account_arn".to_string(),
                Value::Concrete(ConcreteValue::String(master_account_arn.to_string().into())),
            );
        }
        if let Some(master_account_email) = org.master_account_email() {
            attributes.insert(
                "master_account_email".to_string(),
                Value::Concrete(ConcreteValue::String(
                    master_account_email.to_string().into(),
                )),
            );
        }

        identifier
    }

    /// Read an Organizations organization
    pub(crate) async fn read_organizations_organization(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let _identifier = identifier.as_str();

        match self
            .organizations_client
            .describe_organization()
            .send()
            .await
        {
            Ok(response) => {
                if let Some(org) = response.organization() {
                    let mut attributes = HashMap::new();
                    let _org_id =
                        Self::extract_organizations_organization_attributes(org, &mut attributes);
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
                    && service_err.is_aws_organizations_not_in_use_exception()
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to describe organization", e)
                    .for_resource(id.clone()))
            }
        }
    }

    /// Create an Organizations organization
    pub(crate) async fn create_organizations_organization(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateCompletion> {
        let mut req = self.organizations_client.create_organization();

        if let Some(feature_set) = optional_enum_attr(resource, schema, "feature_set")? {
            let fs = aws_sdk_organizations::types::OrganizationFeatureSet::from(feature_set);
            req = req.feature_set(fs);
        }

        let response = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create organization", e)
                .for_resource(resource.id.clone())
        })?;

        if let Some(org) = response.organization() {
            let mut attributes = HashMap::new();
            let org_id = Self::extract_organizations_organization_attributes(org, &mut attributes)
                .ok_or_else(|| {
                    ProviderError::api_error(
                        "organizations.CreateOrganization returned an organization with an empty provider identifier",
                    )
                    .for_resource(resource.id.clone())
                    .with_operation("organizations.CreateOrganization")
                })?;
            let identifier = crate::helpers::applied_identifier(
                &resource.id,
                "organizations.CreateOrganization",
                org_id,
            )?;
            let outcome = self
                .read_organizations_organization(&resource.id, &identifier)
                .await;
            Ok(self.create_outcome_from_read(&resource.id, identifier, attributes, outcome))
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
        _identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.organizations_client
            .delete_organization()
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete organization", e).for_resource(id.clone())
            })?;
        Ok(())
    }
}
