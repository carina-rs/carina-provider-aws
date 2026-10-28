use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_string_attr;
use crate::provider::CreateCompletion;

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
                    let mut attributes = HashMap::new();

                    attributes.insert(
                        "association_id".to_string(),
                        Value::Concrete(ConcreteValue::String(association_id.to_string().into())),
                    );

                    if let Some(rt_id) = assoc.route_table_id() {
                        attributes.insert(
                            "route_table_id".to_string(),
                            Value::Concrete(ConcreteValue::String(rt_id.to_string().into())),
                        );
                    }

                    if let Some(sid) = assoc.subnet_id() {
                        attributes.insert(
                            "subnet_id".to_string(),
                            Value::Concrete(ConcreteValue::String(sid.to_string().into())),
                        );
                    }

                    return Ok(crate::helpers::found_read(
                        id,
                        applied_identifier,
                        attributes,
                    ));
                }
            }
        }

        Ok(ReadOutcome::not_found(id.clone()))
    }

    /// Create an EC2 Subnet Route Table Association
    pub(crate) async fn create_ec2_subnet_route_table_association(
        &self,
        resource: &Resource,
    ) -> ProviderResult<CreateCompletion> {
        let route_table_id = require_string_attr(resource, "route_table_id")?;
        let subnet_id = require_string_attr(resource, "subnet_id")?;

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
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    /// Update an EC2 Subnet Route Table Association (replace association)
    pub(crate) async fn update_ec2_subnet_route_table_association(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // Parse composite identifier: association_id|subnet_id
        let Some((association_id, subnet_id)) = identifier.as_str().split_once('|') else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid association identifier: {}",
                identifier
            ))
            .for_resource(id.clone()));
        };

        let route_table_id = match to.get_attr("route_table_id") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => {
                return Err(ProviderError::invalid_input("route_table_id is required")
                    .for_resource(id.clone()));
            }
        };

        // Replace the association with a new route table
        let result = self
            .ec2_client
            .replace_route_table_association()
            .association_id(association_id)
            .route_table_id(&route_table_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to replace route table association", e)
                    .for_resource(id.clone())
            })?;

        let new_assoc_id = result.new_association_id().ok_or_else(|| {
            ProviderError::api_error(
                "Route table association replaced but no new association ID returned",
            )
            .for_resource(id.clone())
        })?;
        let new_assoc_id = crate::helpers::applied_identifier(
            &id,
            "ec2.ReplaceRouteTableAssociation",
            new_assoc_id,
        )?;

        let composite = format!("{}|{}", new_assoc_id.as_str(), subnet_id);
        let identifier =
            crate::helpers::applied_identifier(&id, "ec2.ReplaceRouteTableAssociation", composite)?;
        Ok(self
            .read_ec2_subnet_route_table_association(&id, &identifier)
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
