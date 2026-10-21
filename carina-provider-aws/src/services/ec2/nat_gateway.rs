use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use aws_sdk_ec2::types::NatGatewayState;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    CreatePoll, DeletePoll, RetryPolicy, optional_enum_attr, require_string_attr,
    retry_aws_operation,
};

fn nat_gateway_create_poll_state(state: Option<&NatGatewayState>) -> CreatePoll<()> {
    match state {
        Some(NatGatewayState::Available) => CreatePoll::Ready(()),
        Some(NatGatewayState::Failed) => CreatePoll::Failed,
        _ => CreatePoll::Pending,
    }
}

fn nat_gateway_delete_poll_state(state: Option<&NatGatewayState>) -> DeletePoll {
    match state {
        Some(NatGatewayState::Deleted) | None => DeletePoll::Gone,
        _ => DeletePoll::Pending,
    }
}

impl AwsProvider {
    /// Read an EC2 NAT Gateway
    pub(crate) async fn read_ec2_nat_gateway(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        use aws_sdk_ec2::types::Filter;

        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let filter = Filter::builder()
            .name("nat-gateway-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_nat_gateways()
            .filter(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe NAT gateways",
                    "ec2.DescribeNatGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(ngw) = result.nat_gateways().first() {
            // Skip deleted NAT gateways
            if ngw.state() == Some(&NatGatewayState::Deleted) {
                return Ok(State::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            // Extract attributes
            let identifier_value = Self::extract_ec2_nat_gateway_attributes(ngw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(ngw.tags()) {
                attributes.insert("tags".to_string(), tags_value);
            }

            let state = State::existing(id.clone(), attributes);
            Ok(if let Some(id_val) = identifier_value {
                state.with_identifier(id_val)
            } else {
                state
            })
        } else {
            Ok(State::not_found(id.clone()))
        }
    }

    /// Create an EC2 NAT Gateway
    pub(crate) async fn create_ec2_nat_gateway_start(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<String> {
        let subnet_id = require_string_attr(resource, "subnet_id")?;

        let mut req = self.ec2_client.create_nat_gateway().subnet_id(&subnet_id);

        if let Some(Value::Concrete(ConcreteValue::String(alloc_id))) =
            resource.get_attr("allocation_id")
        {
            req = req.allocation_id(alloc_id.expose_plaintext());
        }

        if let Some(conn_type) = optional_enum_attr(resource, schema, "connectivity_type")? {
            use aws_sdk_ec2::types::ConnectivityType;
            req = req.connectivity_type(ConnectivityType::from(conn_type));
        }

        let rid = resource.id.clone();
        let result = retry_aws_operation("create NAT gateway", RetryPolicy::default(), || {
            let req = req.clone();
            async move { req.send().await }
        })
        .await
        .map_err(|e| {
            api_error_with_meta("Failed to create NAT gateway", "ec2.CreateNatGateway", e)
                .for_resource(rid.clone())
        })?;

        let ngw_id = result
            .nat_gateway()
            .and_then(|ngw| ngw.nat_gateway_id())
            .ok_or_else(|| {
                ProviderError::api_error("NAT Gateway created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Apply tags
        self.apply_ec2_tags(&resource.id, ngw_id, &resource.resolved_attributes(), None)
            .await?;
        Ok(ngw_id.to_string())
    }

    /// Update an EC2 NAT Gateway (tags only)
    pub(crate) async fn update_ec2_nat_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_nat_gateway(&id, Some(identifier)).await
    }

    /// Delete an EC2 NAT Gateway
    pub(crate) async fn delete_ec2_nat_gateway_start(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_nat_gateway()
            .nat_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete NAT gateway", "ec2.DeleteNatGateway", e)
                    .for_resource(id.clone())
            })?;

        Ok(())
    }

    /// Perform one describe call for a pending NAT gateway create.
    pub(crate) async fn poll_ec2_nat_gateway_create(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<CreatePoll<()>> {
        let result = self
            .ec2_client
            .describe_nat_gateways()
            .nat_gateway_ids(nat_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe NAT gateway",
                    "ec2.DescribeNatGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;
        Ok(nat_gateway_create_poll_state(
            result.nat_gateways().first().and_then(|ngw| ngw.state()),
        ))
    }

    /// Perform one describe call for a pending NAT gateway delete.
    pub(crate) async fn poll_ec2_nat_gateway_delete(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<DeletePoll> {
        let result = self
            .ec2_client
            .describe_nat_gateways()
            .nat_gateway_ids(nat_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe NAT gateway",
                    "ec2.DescribeNatGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;
        Ok(nat_gateway_delete_poll_state(
            result.nat_gateways().first().and_then(|ngw| ngw.state()),
        ))
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;

    #[test]
    fn create_poll_maps_pending_ready_and_failed_states() {
        assert_eq!(
            nat_gateway_create_poll_state(Some(&NatGatewayState::Pending)),
            CreatePoll::Pending
        );
        assert_eq!(
            nat_gateway_create_poll_state(Some(&NatGatewayState::Available)),
            CreatePoll::Ready(())
        );
        assert_eq!(
            nat_gateway_create_poll_state(Some(&NatGatewayState::Failed)),
            CreatePoll::Failed
        );
        assert_eq!(nat_gateway_create_poll_state(None), CreatePoll::Pending);
    }

    #[test]
    fn delete_poll_maps_pending_and_gone_states() {
        assert_eq!(
            nat_gateway_delete_poll_state(Some(&NatGatewayState::Deleting)),
            DeletePoll::Pending
        );
        assert_eq!(
            nat_gateway_delete_poll_state(Some(&NatGatewayState::Deleted)),
            DeletePoll::Gone
        );
        assert_eq!(nat_gateway_delete_poll_state(None), DeletePoll::Gone);
    }
}
