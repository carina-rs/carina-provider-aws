use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::Ec2SubnetRouteTableAssociationReadModelSupplement;
use crate::send_models_generated::{
    Ec2SubnetRouteTableAssociationCreateInput, Ec2SubnetRouteTableAssociationUpdateInput,
};

impl AwsProvider {
    /// Read an EC2 Subnet Route Table Association
    pub(crate) async fn read_ec2_subnet_route_table_association(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Parse composite identifier: association_id|subnet_id
        let Some((association_id, _subnet_id)) = identifier.split_once('|') else {
            return Ok(ReadOutcome::not_found(id.clone()));
        };

        // Describe route tables filtered by association ID
        use aws_sdk_ec2::types::Filter;

        let filter = Filter::builder()
            .name("association.route-table-association-id")
            .values(association_id)
            .build();

        let result = self
            .ec2_client
            .describe_route_tables()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe route tables", e).for_resource(id.clone())
            })?;

        if let Some(rt) = result.route_tables().first() {
            // Find the association matching the association_id
            for assoc in rt.associations() {
                if assoc.route_table_association_id() == Some(association_id) {
                    let fields = Self::extract_ec2_subnet_route_table_association_attributes(assoc);

                    let (_, model) = fields
                        .into_model(id, Ec2SubnetRouteTableAssociationReadModelSupplement {})?;
                    return found_managed_read(id, applied_identifier, model);
                }
            }
        }

        Ok(ReadOutcome::not_found(id.clone()))
    }

    /// Create an EC2 Subnet Route Table Association
    pub(crate) async fn create_ec2_subnet_route_table_association(
        &self,
        resource: &Resource<impl Sized>,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = Ec2SubnetRouteTableAssociationCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::Ec2SubnetRouteTableAssociationCreateParts {
            route_table_id,
            subnet_id,
        } = input.into_parts();
        let route_table_id = route_table_id.into_inner();
        let subnet_id = subnet_id.into_inner();

        let result = self
            .ec2_client
            .associate_route_table()
            .route_table_id(&route_table_id)
            .subnet_id(&subnet_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to associate route table", e)
                    .for_resource(resource.id.clone())
            })?;

        let assoc_id = result.association_id().ok_or_else(|| {
            ProviderError::api_error(
                "Route table association created but no association ID returned",
            )
            .for_resource(resource.id.clone())
        })?;
        let assoc_id =
            crate::helpers::applied_identifier(&resource.id, "ec2.AssociateRouteTable", assoc_id)?;

        // Composite identifier: association_id|subnet_id
        let composite = format!("{}|{}", assoc_id.as_str(), subnet_id);
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "ec2.AssociateRouteTable", composite)?;
        let outcome = self
            .read_ec2_subnet_route_table_association(&resource.id, &identifier)
            .await;
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

    /// Update an EC2 Subnet Route Table Association (replace association)
    pub(crate) async fn update_ec2_subnet_route_table_association(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2SubnetRouteTableAssociationUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2SubnetRouteTableAssociationUpdateParts =
            input.into_parts();
        Ok(self
            .read_ec2_subnet_route_table_association(&id, identifier)
            .await)
    }

    /// Delete an EC2 Subnet Route Table Association
    pub(crate) async fn delete_ec2_subnet_route_table_association(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        // Parse composite identifier: association_id|subnet_id
        let Some((association_id, _subnet_id)) = identifier.split_once('|') else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid association identifier: {}",
                identifier
            ))
            .for_resource(id));
        };

        self.ec2_client
            .disassociate_route_table()
            .association_id(association_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to disassociate route table", e)
                    .for_resource(id.clone())
            })?;

        Ok(())
    }
}
