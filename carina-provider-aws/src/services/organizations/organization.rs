use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::{ManagedReadModel, found_managed_read};
use crate::read_models_generated::OrganizationsOrganizationReadModelSupplement;
use crate::send_models_generated::OrganizationsOrganizationCreateInput;

impl AwsProvider {
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
                    let fields = Self::extract_organizations_organization_attributes(org);
                    let (_, model) =
                        fields.into_model(id, OrganizationsOrganizationReadModelSupplement {})?;
                    found_managed_read(id, applied_identifier, model)
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
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = OrganizationsOrganizationCreateInput::decode(resource)?;
        let sent = input.sent();
        let (feature_set,) = input.into_parts();
        let mut req = self.organizations_client.create_organization();

        if let Some(feature_set) = feature_set {
            let fs =
                aws_sdk_organizations::types::OrganizationFeatureSet::from(feature_set.as_str());
            req = req.feature_set(fs);
        }

        let response = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create organization", e)
                .for_resource(resource.id.clone())
        })?;

        if let Some(org) = response.organization() {
            let fields = Self::extract_organizations_organization_attributes(org);
            let (applied_identifier, model) = fields.into_model(
                &resource.id,
                OrganizationsOrganizationReadModelSupplement {},
            )?;
            let org_id = applied_identifier.ok_or_else(|| {
                    ProviderError::api_error(
                        "organizations.CreateOrganization returned an organization with an empty provider identifier",
                    )
                    .for_resource(resource.id.clone())
                    .with_operation("organizations.CreateOrganization")
                })?;
            let attributes = model.into_attributes()?;
            let identifier = crate::helpers::applied_identifier(
                &resource.id,
                "organizations.CreateOrganization",
                org_id,
            )?;
            let outcome = self
                .read_organizations_organization(&resource.id, &identifier)
                .await;
            Ok((
                sent,
                self.create_outcome_from_read(&resource.id, identifier, attributes, outcome),
            ))
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
