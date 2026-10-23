use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, CreateOutcome, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_string_attr;

impl AwsProvider {
    /// Read an EC2 Route Table
    pub(crate) async fn read_ec2_route_table(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("route-table-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_route_tables()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe route tables",
                    "ec2.DescribeRouteTables",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(rt) = result.route_tables().first() {
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value = Self::extract_ec2_route_table_attributes(rt, &mut attributes);

            // Convert routes to list (complex nested structure)
            let mut routes_list = Vec::new();
            for route in rt.routes() {
                let mut route_map: IndexMap<String, Value> = IndexMap::new();
                if let Some(dest) = route.destination_cidr_block() {
                    route_map.insert(
                        "destination".to_string(),
                        Value::Concrete(ConcreteValue::String(dest.to_string().into())),
                    );
                }
                if let Some(gw) = route.gateway_id() {
                    route_map.insert(
                        "gateway_id".to_string(),
                        Value::Concrete(ConcreteValue::String(gw.to_string().into())),
                    );
                }
                if !route_map.is_empty() {
                    routes_list.push(Value::Concrete(ConcreteValue::Map(route_map)));
                }
            }
            if !routes_list.is_empty() {
                attributes.insert(
                    "routes".to_string(),
                    Value::Concrete(ConcreteValue::List(routes_list)),
                );
            }

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(rt.tags()) {
                attributes.insert("tags".to_string(), tags_value);
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

    /// Create an EC2 Route Table
    pub(crate) async fn create_ec2_route_table(
        &self,
        resource: &Resource,
    ) -> ProviderResult<CreateOutcome> {
        let vpc_id = require_string_attr(resource, "vpc_id")?;

        // Create Route Table
        let result = self
            .ec2_client
            .create_route_table()
            .vpc_id(&vpc_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to create route table", "ec2.CreateRouteTable", e)
                    .for_resource(resource.id.clone())
            })?;

        let rt_id = result
            .route_table()
            .and_then(|rt| rt.route_table_id())
            .ok_or_else(|| {
                ProviderError::api_error("Route Table created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Apply tags
        self.apply_ec2_tags(&resource.id, rt_id, &resource.resolved_attributes(), None)
            .await?;

        // Add routes
        if let Some(Value::Concrete(ConcreteValue::List(routes))) = resource.get_attr("routes") {
            for route in routes {
                if let Value::Concrete(ConcreteValue::Map(route_map)) = route {
                    let destination = route_map.get("destination").and_then(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s)
                        } else {
                            None
                        }
                    });
                    let gateway_id = route_map.get("gateway_id").and_then(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s)
                        } else {
                            None
                        }
                    });

                    if let (Some(dest), Some(gw_id)) = (destination, gateway_id) {
                        self.ec2_client
                            .create_route()
                            .route_table_id(rt_id)
                            .destination_cidr_block(dest.expose_plaintext())
                            .gateway_id(gw_id.expose_plaintext())
                            .send()
                            .await
                            .map_err(|e| {
                                api_error_with_meta("Failed to create route", "ec2.CreateRoute", e)
                                    .for_resource(resource.id.clone())
                            })?;
                    }
                }
            }
        }

        // Read back using route table ID (reliable identifier)
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "ec2.CreateRouteTable", rt_id)?;
        let outcome = self.read_ec2_route_table(&resource.id, &identifier).await;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }
}
