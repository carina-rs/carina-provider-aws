use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_enum_attr;
use crate::provider::AwsCreateAttempt;

impl AwsProvider {
    /// Read an EC2 VPN Gateway
    pub(crate) async fn read_ec2_vpn_gateway(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("vpn-gateway-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_vpn_gateways()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe VPN gateways",
                    "ec2.DescribeVpnGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(vgw) = result.vpn_gateways().first() {
            // Skip deleted VPN gateways
            if vgw.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let _identifier_value = Self::extract_ec2_vpn_gateway_attributes(vgw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(vgw.tags()) {
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

    /// Create an EC2 VPN Gateway
    pub(crate) async fn create_ec2_vpn_gateway<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let gw_type = match require_enum_attr(resource, schema, "type") {
            Ok(gateway_type) => gateway_type,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let mut req = self
            .ec2_client
            .create_vpn_gateway()
            .r#type(aws_sdk_ec2::types::GatewayType::from(gw_type.as_str()));

        if let Some(Value::Concrete(ConcreteValue::Int(asn))) = resource.get_attr("amazon_side_asn")
        {
            req = req.amazon_side_asn(*asn);
        }

        let result = match req.send().await {
            Ok(result) => result,
            Err(error) => {
                return Err(attempt.untracked(
                    api_error_with_meta(
                        "Failed to create VPN gateway",
                        "ec2.CreateVpnGateway",
                        error,
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
        };

        let vgw_id = match result.vpn_gateway().and_then(|vgw| vgw.vpn_gateway_id()) {
            Some(vgw_id) => vgw_id,
            None => {
                return Err(attempt.untracked(
                    ProviderError::api_error("VPN Gateway created but no ID returned")
                        .for_resource(resource.id.clone()),
                ));
            }
        };
        let identifier = match crate::helpers::applied_identifier(
            &resource.id,
            "ec2.CreateVpnGateway",
            vgw_id,
        ) {
            Ok(identifier) => identifier,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let issued = attempt.issued(
            identifier.clone(),
            Self::sent_only(resource, &["type", "amazon_side_asn"]),
        );

        // Apply tags
        if let Err(error) = self
            .apply_ec2_tags(&resource.id, vgw_id, &resource.resolved_attributes(), None)
            .await
        {
            return Err(self.failed_issued(&resource.id, issued, error).await);
        }

        // Read back
        let outcome = self.read_ec2_vpn_gateway(&resource.id, &identifier).await;
        let completion = self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        );
        Ok(self.finish_issued(issued, completion))
    }

    /// Update an EC2 VPN Gateway (tags only)
    pub(crate) async fn update_ec2_vpn_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_vpn_gateway(&id, identifier).await)
    }

    /// Delete an EC2 VPN Gateway
    pub(crate) async fn delete_ec2_vpn_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.ec2_client
            .delete_vpn_gateway()
            .vpn_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete VPN gateway", "ec2.DeleteVpnGateway", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }
}
