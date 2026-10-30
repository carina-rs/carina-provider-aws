use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId, State};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::build_tag_specification_with_create_token;
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2VpnGatewayReadModelSupplement;
use crate::send_models_generated::{Ec2VpnGatewayCreateInput, Ec2VpnGatewayUpdateInput};

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
                api_error_with_meta("Failed to describe VPN gateways", e).for_resource(id.clone())
            })?;

        if let Some(vgw) = result.vpn_gateways().first() {
            // Skip deleted VPN gateways
            if vgw.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let fields = Self::extract_ec2_vpn_gateway_attributes(vgw);

            let (_, model) = fields.into_model(
                id,
                Ec2VpnGatewayReadModelSupplement {
                    tags: Self::ec2_tags_to_value(vgw.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPN Gateway
    pub(crate) async fn create_ec2_vpn_gateway<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2VpnGatewayCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (amazon_side_asn, availability_zone, tags, gateway_type) = input.into_parts();

        let mut req = self
            .ec2_client
            .create_vpn_gateway()
            .r#type(aws_sdk_ec2::types::GatewayType::from(gateway_type.as_str()))
            .tag_specifications(build_tag_specification_with_create_token(
                tags.as_ref(),
                aws_sdk_ec2::types::ResourceType::VpnGateway,
                client_token,
            ));

        if let Some(asn) = amazon_side_asn {
            req = req.amazon_side_asn(asn);
        }
        if let Some(availability_zone) = availability_zone {
            req = req.availability_zone(availability_zone);
        }

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::VpnGateway.create_context(client_token, sent),
            result,
            |output| {
                output
                    .vpn_gateway()
                    .and_then(|gateway| gateway.vpn_gateway_id())
                    .map(str::to_owned)
            },
        )
        .await
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
        let input = Ec2VpnGatewayUpdateInput::decode(&to)?;
        let (tags,) = input.into_parts();
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
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
                api_error_with_meta("Failed to delete VPN gateway", e).for_resource(id.clone())
            })?;
        Ok(())
    }
}
