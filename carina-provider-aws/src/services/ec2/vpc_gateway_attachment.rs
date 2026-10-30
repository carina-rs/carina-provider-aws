use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::Ec2VpcGatewayAttachmentReadModel;
use crate::send_models_generated::{
    Ec2VpcGatewayAttachmentCreateInput, Ec2VpcGatewayAttachmentUpdateInput,
};

impl AwsProvider {
    /// Read an EC2 VPC Gateway Attachment
    ///
    /// The identifier is a composite: `vpc_id|igw_id` or `vpc_id|vgw_id`
    pub(crate) async fn read_ec2_vpc_gateway_attachment(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Parse composite identifier: vpc_id|gateway_id
        let Some((vpc_id, gateway_id)) = identifier.split_once('|') else {
            return Ok(ReadOutcome::not_found(id.clone()));
        };

        if gateway_id.starts_with("igw-") {
            // Internet Gateway attachment
            self.read_igw_attachment(id, vpc_id, gateway_id, applied_identifier)
                .await
        } else if gateway_id.starts_with("vgw-") {
            // VPN Gateway attachment
            self.read_vgw_attachment(id, vpc_id, gateway_id, applied_identifier)
                .await
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    async fn read_igw_attachment(
        &self,
        id: &ResourceId,
        vpc_id: &str,
        igw_id: &str,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let filter = Filter::builder()
            .name("internet-gateway-id")
            .values(igw_id)
            .build();

        let result = self
            .ec2_client
            .describe_internet_gateways()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe internet gateways", e)
                    .for_resource(id.clone())
            })?;

        if let Some(igw) = result.internet_gateways().first() {
            // Check if the IGW is attached to the specified VPC
            for attachment in igw.attachments() {
                if attachment.vpc_id() == Some(vpc_id) {
                    let model = Ec2VpcGatewayAttachmentReadModel {
                        internet_gateway_id: Some(Value::Concrete(ConcreteValue::String(
                            igw_id.to_string().into(),
                        ))),
                        vpc_id: Value::Concrete(ConcreteValue::String(vpc_id.to_string().into())),
                        vpn_gateway_id: None,
                    };
                    return found_managed_read(id, identifier, model);
                }
            }
        }

        Ok(ReadOutcome::not_found(id.clone()))
    }

    async fn read_vgw_attachment(
        &self,
        id: &ResourceId,
        vpc_id: &str,
        vgw_id: &str,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let filter = Filter::builder()
            .name("vpn-gateway-id")
            .values(vgw_id)
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
            for attachment in vgw.vpc_attachments() {
                if attachment.vpc_id() == Some(vpc_id) {
                    let state_str = attachment.state().map(|s| s.as_str());
                    if state_str == Some("attached") {
                        let model = Ec2VpcGatewayAttachmentReadModel {
                            internet_gateway_id: None,
                            vpc_id: Value::Concrete(ConcreteValue::String(
                                vpc_id.to_string().into(),
                            )),
                            vpn_gateway_id: Some(Value::Concrete(ConcreteValue::String(
                                vgw_id.to_string().into(),
                            ))),
                        };
                        return found_managed_read(id, identifier, model);
                    }
                }
            }
        }

        Ok(ReadOutcome::not_found(id.clone()))
    }

    /// Create an EC2 VPC Gateway Attachment
    pub(crate) async fn create_ec2_vpc_gateway_attachment(
        &self,
        resource: &Resource<impl Sized>,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = Ec2VpcGatewayAttachmentCreateInput::decode(resource)?;
        let sent = input.sent();
        let (internet_gateway_id, vpc_id, vpn_gateway_id) = input.into_parts();

        if let Some(igw_id) = internet_gateway_id {
            // Attach Internet Gateway
            self.ec2_client
                .attach_internet_gateway()
                .internet_gateway_id(&igw_id)
                .vpc_id(&vpc_id)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to attach internet gateway", e)
                        .for_resource(resource.id.clone())
                })?;

            let composite = format!("{vpc_id}|{igw_id}");
            let identifier = crate::helpers::applied_identifier(
                &resource.id,
                "ec2.AttachInternetGateway",
                composite,
            )?;
            let outcome = self
                .read_ec2_vpc_gateway_attachment(&resource.id, &identifier)
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
        } else if let Some(vgw_id) = vpn_gateway_id {
            // Attach VPN Gateway
            self.ec2_client
                .attach_vpn_gateway()
                .vpn_gateway_id(&vgw_id)
                .vpc_id(&vpc_id)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to attach VPN gateway", e)
                        .for_resource(resource.id.clone())
                })?;

            let composite = format!("{vpc_id}|{vgw_id}");
            let identifier = crate::helpers::applied_identifier(
                &resource.id,
                "ec2.AttachVpnGateway",
                composite,
            )?;
            let outcome = self
                .read_ec2_vpc_gateway_attachment(&resource.id, &identifier)
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
        } else {
            Err(ProviderError::invalid_input(
                "Either internet_gateway_id or vpn_gateway_id is required",
            )
            .for_resource(resource.id.clone()))
        }
    }

    /// Update an EC2 VPC Gateway Attachment (not supported - create_only)
    pub(crate) async fn update_ec2_vpc_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: &Resource<impl Sized>,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2VpcGatewayAttachmentUpdateInput::decode(to)?;
        let () = input.into_parts();
        // VPC Gateway Attachment attributes are all create_only,
        // so updates should not occur. Read back the current state.
        Ok(self.read_ec2_vpc_gateway_attachment(&id, identifier).await)
    }

    /// Delete an EC2 VPC Gateway Attachment
    pub(crate) async fn delete_ec2_vpc_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let Some((vpc_id, gateway_id)) = identifier.split_once('|') else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid gateway attachment identifier: {}",
                identifier
            ))
            .for_resource(id));
        };

        if gateway_id.starts_with("igw-") {
            self.ec2_client
                .detach_internet_gateway()
                .internet_gateway_id(gateway_id)
                .vpc_id(vpc_id)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to detach internet gateway", e)
                        .for_resource(id.clone())
                })?;
        } else if gateway_id.starts_with("vgw-") {
            self.ec2_client
                .detach_vpn_gateway()
                .vpn_gateway_id(gateway_id)
                .vpc_id(vpc_id)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to detach VPN gateway", e).for_resource(id.clone())
                })?;
        } else {
            return Err(ProviderError::invalid_input(format!(
                "Unknown gateway type in identifier: {}",
                identifier
            ))
            .for_resource(id));
        }

        Ok(())
    }
}
