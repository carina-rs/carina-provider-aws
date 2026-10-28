use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_string_attr;
use crate::provider::CreateCompletion;

impl AwsProvider {
    /// Read an EC2 Route (routes are identified by route_table_id + destination)
    pub(crate) async fn read_ec2_route(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        // Parse composite identifier: route_table_id|destination_cidr_block
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let Some((route_table_id, destination_cidr_block)) = identifier.split_once('|') else {
            return Ok(ReadOutcome::not_found(id.clone()));
        };

        // Describe the route table to get its routes
        let result = self
            .ec2_client
            .describe_route_tables()
            .route_table_ids(route_table_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe route table", e).for_resource(id.clone())
            })?;

        if let Some(rt) = result.route_tables().first() {
            // Find the route matching destination_cidr_block
            for route in rt.routes() {
                if route.destination_cidr_block() == Some(destination_cidr_block) {
                    let mut attributes = HashMap::new();

                    // Auto-generated attribute extraction
                    Self::extract_ec2_route_attributes(route, &mut attributes);

                    // route_table_id is not in the Route struct, add from parameter
                    attributes.insert(
                        "route_table_id".to_string(),
                        Value::Concrete(ConcreteValue::String(route_table_id.to_string().into())),
                    );

                    // Route identifier is route_table_id|destination_cidr_block
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

    /// Create an EC2 Route
    pub(crate) async fn create_ec2_route(
        &self,
        resource: &Resource,
    ) -> ProviderResult<CreateCompletion> {
        let route_table_id = require_string_attr(resource, "route_table_id")?;
        let destination_cidr = require_string_attr(resource, "destination_cidr_block")?;

        let mut req = self
            .ec2_client
            .create_route()
            .route_table_id(&route_table_id)
            .destination_cidr_block(&destination_cidr);

        // Add gateway_id if specified
        if let Some(Value::Concrete(ConcreteValue::String(gw_id))) = resource.get_attr("gateway_id")
        {
            req = req.gateway_id(gw_id.expose_plaintext());
        }

        // Add nat_gateway_id if specified
        if let Some(Value::Concrete(ConcreteValue::String(nat_gw_id))) =
            resource.get_attr("nat_gateway_id")
        {
            req = req.nat_gateway_id(nat_gw_id.expose_plaintext());
        }

        req.send().await.map_err(|e| {
            api_error_with_meta("Failed to create route", e).for_resource(resource.id.clone())
        })?;

        // Route identifier is route_table_id|destination_cidr_block
        let identifier = format!("{}|{}", route_table_id, destination_cidr);
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "ec2.CreateRoute", identifier)?;
        let outcome = self.read_ec2_route(&resource.id, &identifier).await;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    /// Update an EC2 Route (replace the route)
    pub(crate) async fn update_ec2_route(
        &self,
        id: ResourceId,
        _identifier: &AppliedIdentifier,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let route_table_id = match to.get_attr("route_table_id") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => {
                return Err(ProviderError::invalid_input("route_table_id is required")
                    .for_resource(id.clone()));
            }
        };

        let destination_cidr = match to.get_attr("destination_cidr_block") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => {
                return Err(
                    ProviderError::invalid_input("destination_cidr_block is required")
                        .for_resource(id.clone()),
                );
            }
        };

        let mut req = self
            .ec2_client
            .replace_route()
            .route_table_id(&route_table_id)
            .destination_cidr_block(&destination_cidr);

        // Add gateway_id if specified
        if let Some(Value::Concrete(ConcreteValue::String(gw_id))) = to.get_attr("gateway_id") {
            req = req.gateway_id(gw_id.expose_plaintext());
        }

        // Add nat_gateway_id if specified
        if let Some(Value::Concrete(ConcreteValue::String(nat_gw_id))) =
            to.get_attr("nat_gateway_id")
        {
            req = req.nat_gateway_id(nat_gw_id.expose_plaintext());
        }

        req.send().await.map_err(|e| {
            api_error_with_meta("Failed to update route", e).for_resource(id.clone())
        })?;

        // Route identifier is route_table_id|destination_cidr_block
        let identifier = format!("{}|{}", route_table_id, destination_cidr);
        let identifier = crate::helpers::applied_identifier(&id, "ec2.ReplaceRoute", identifier)?;
        Ok(self.read_ec2_route(&id, &identifier).await)
    }

    /// Delete an EC2 Route
    pub(crate) async fn delete_ec2_route(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        // Parse composite identifier: route_table_id|destination_cidr_block
        let Some((route_table_id, destination_cidr_block)) = identifier.split_once('|') else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid route identifier: {}",
                identifier
            ))
            .for_resource(id));
        };

        self.ec2_client
            .delete_route()
            .route_table_id(route_table_id)
            .destination_cidr_block(destination_cidr_block)
            .send()
            .await
            .map_err(|e| api_error_with_meta("Failed to delete route", e).for_resource(id))?;

        Ok(())
    }
}
