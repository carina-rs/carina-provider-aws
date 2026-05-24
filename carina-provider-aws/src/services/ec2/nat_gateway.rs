use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};

use aws_sdk_ec2::types::NatGatewayState;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    PollState, RetryPolicy, require_string_attr, retry_aws_operation, wait_for_ec2_state,
};

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
    pub(crate) async fn create_ec2_nat_gateway(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let subnet_id = require_string_attr(&resource, "subnet_id")?;

        let mut req = self.ec2_client.create_nat_gateway().subnet_id(&subnet_id);

        if let Some(Value::Concrete(ConcreteValue::String(alloc_id))) =
            resource.get_attr("allocation_id")
        {
            req = req.allocation_id(alloc_id);
        }

        if let Some(Value::Concrete(ConcreteValue::String(conn_type))) =
            resource.get_attr("connectivity_type")
        {
            use aws_sdk_ec2::types::ConnectivityType;
            req = req.connectivity_type(ConnectivityType::from(conn_type.as_str()));
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

        // Wait for NAT gateway to become available
        self.wait_for_nat_gateway_available(&resource.id, ngw_id)
            .await?;

        // Read back using NAT gateway ID
        self.read_ec2_nat_gateway(&resource.id, Some(ngw_id)).await
    }

    /// Update an EC2 NAT Gateway (tags only)
    pub(crate) async fn update_ec2_nat_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: ManagedResource,
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
    pub(crate) async fn delete_ec2_nat_gateway(
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

        // Wait for NAT gateway to be deleted
        self.wait_for_nat_gateway_deleted(&id, identifier).await?;

        Ok(())
    }

    /// Wait for a NAT gateway to reach the "available" state
    async fn wait_for_nat_gateway_available(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
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
                        .for_resource(rid.clone())
                    })?;
                Ok(
                    if let Some(ngw) = result.nat_gateways().first()
                        && let Some(state) = ngw.state()
                    {
                        if *state == NatGatewayState::Available {
                            PollState::Ready
                        } else if *state == NatGatewayState::Failed {
                            PollState::Failed
                        } else {
                            PollState::Pending
                        }
                    } else {
                        PollState::Pending
                    },
                )
            },
            60,
            "Timeout waiting for NAT gateway to become available",
            "NAT gateway creation failed",
        )
        .await
    }

    /// Wait for a NAT gateway to be deleted
    async fn wait_for_nat_gateway_deleted(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        // NAT Gateways can take 5+ minutes to delete; use 90 iterations (7.5 min).
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
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
                        .for_resource(rid.clone())
                    })?;
                Ok(if let Some(ngw) = result.nat_gateways().first() {
                    if ngw.state() == Some(&NatGatewayState::Deleted) {
                        PollState::Gone
                    } else {
                        PollState::Pending
                    }
                } else {
                    PollState::Gone
                })
            },
            90,
            "Timeout waiting for NAT gateway to be deleted",
            "NAT gateway deletion failed",
        )
        .await
    }
}
