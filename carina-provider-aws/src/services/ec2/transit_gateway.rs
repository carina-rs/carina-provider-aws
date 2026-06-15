use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{PollState, build_tag_specification, optional_enum_attr, wait_for_ec2_state};

impl AwsProvider {
    /// Read an EC2 Transit Gateway
    pub(crate) async fn read_ec2_transit_gateway(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .ec2_client
            .describe_transit_gateways()
            .transit_gateway_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe transit gateways",
                    "ec2.DescribeTransitGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(tgw) = result.transit_gateways().first() {
            // Skip deleted transit gateways
            if tgw.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(State::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let identifier_value =
                Self::extract_ec2_transit_gateway_attributes(tgw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(tgw.tags()) {
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

    /// Create an EC2 Transit Gateway
    pub(crate) async fn create_ec2_transit_gateway(
        &self,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let mut req = self.ec2_client.create_transit_gateway();

        if let Some(Value::Concrete(ConcreteValue::String(desc))) = resource.get_attr("description")
        {
            req = req.description(desc);
        }

        // Build options
        let mut options = aws_sdk_ec2::types::TransitGatewayRequestOptions::builder();
        let mut has_options = false;

        if let Some(Value::Concrete(ConcreteValue::Int(asn))) = resource.get_attr("amazon_side_asn")
        {
            options = options.amazon_side_asn(*asn);
            has_options = true;
        }

        if let Some(v) = optional_enum_attr(resource, "auto_accept_shared_attachments") {
            use aws_sdk_ec2::types::AutoAcceptSharedAttachmentsValue;
            options =
                options.auto_accept_shared_attachments(AutoAcceptSharedAttachmentsValue::from(v));
            has_options = true;
        }

        if let Some(v) = optional_enum_attr(resource, "default_route_table_association") {
            use aws_sdk_ec2::types::DefaultRouteTableAssociationValue;
            options =
                options.default_route_table_association(DefaultRouteTableAssociationValue::from(v));
            has_options = true;
        }

        if let Some(v) = optional_enum_attr(resource, "default_route_table_propagation") {
            use aws_sdk_ec2::types::DefaultRouteTablePropagationValue;
            options =
                options.default_route_table_propagation(DefaultRouteTablePropagationValue::from(v));
            has_options = true;
        }

        if let Some(v) = optional_enum_attr(resource, "dns_support") {
            use aws_sdk_ec2::types::DnsSupportValue;
            options = options.dns_support(DnsSupportValue::from(v));
            has_options = true;
        }

        if let Some(v) = optional_enum_attr(resource, "vpn_ecmp_support") {
            use aws_sdk_ec2::types::VpnEcmpSupportValue;
            options = options.vpn_ecmp_support(VpnEcmpSupportValue::from(v));
            has_options = true;
        }

        if has_options {
            req = req.options(options.build());
        }

        // Apply tags via TagSpecifications
        if let Some(tag_spec) =
            build_tag_specification(resource, aws_sdk_ec2::types::ResourceType::TransitGateway)
        {
            req = req.tag_specifications(tag_spec);
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta(
                "Failed to create transit gateway",
                "ec2.CreateTransitGateway",
                e,
            )
            .for_resource(resource.id.clone())
        })?;

        let tgw_id = result
            .transit_gateway()
            .and_then(|tgw| tgw.transit_gateway_id())
            .ok_or_else(|| {
                ProviderError::api_error("Transit Gateway created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Wait for transit gateway to become available
        self.wait_for_transit_gateway_available(&resource.id, tgw_id)
            .await?;

        // Read back
        self.read_ec2_transit_gateway(&resource.id, Some(tgw_id))
            .await
    }

    /// Update an EC2 Transit Gateway (tags only for now)
    pub(crate) async fn update_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_transit_gateway(&id, Some(identifier)).await
    }

    /// Delete an EC2 Transit Gateway
    pub(crate) async fn delete_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway()
            .transit_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to delete transit gateway",
                    "ec2.DeleteTransitGateway",
                    e,
                )
                .for_resource(id.clone())
            })?;

        // Wait for transit gateway to be deleted
        self.wait_for_transit_gateway_deleted(&id, identifier)
            .await?;

        Ok(())
    }

    /// Wait for a transit gateway to reach the "available" state
    async fn wait_for_transit_gateway_available(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateways()
                    .transit_gateway_ids(transit_gateway_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway",
                            "ec2.DescribeTransitGateways",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(
                    if let Some(tgw) = result.transit_gateways().first()
                        && let Some(state) = tgw.state()
                    {
                        match state.as_str() {
                            "available" => PollState::Ready,
                            "failed" | "deleted" => PollState::Failed,
                            _ => PollState::Pending,
                        }
                    } else {
                        PollState::Pending
                    },
                )
            },
            60,
            "Timeout waiting for transit gateway to become available",
            "Transit gateway creation failed",
        )
        .await
    }

    /// Wait for a transit gateway to be deleted
    async fn wait_for_transit_gateway_deleted(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateways()
                    .transit_gateway_ids(transit_gateway_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway",
                            "ec2.DescribeTransitGateways",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(if let Some(tgw) = result.transit_gateways().first() {
                    if tgw.state().map(|s| s.as_str()) == Some("deleted") {
                        PollState::Gone
                    } else {
                        PollState::Pending
                    }
                } else {
                    PollState::Gone
                })
            },
            60,
            "Timeout waiting for transit gateway to be deleted",
            "Transit gateway deletion failed",
        )
        .await
    }
}
