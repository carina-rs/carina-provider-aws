use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId, State};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{build_tag_specification_with_create_token, send_aws_operation_with_token};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2EgressOnlyInternetGatewayReadModelSupplement;
use crate::send_models_generated::{
    Ec2EgressOnlyInternetGatewayCreateInput, Ec2EgressOnlyInternetGatewayUpdateInput,
};

impl AwsProvider {
    /// Read an EC2 Egress-Only Internet Gateway
    pub(crate) async fn read_ec2_egress_only_internet_gateway(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let result = self
            .ec2_client
            .describe_egress_only_internet_gateways()
            .egress_only_internet_gateway_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe egress-only internet gateways", e)
                    .for_resource(id.clone())
            })?;

        if let Some(eigw) = result.egress_only_internet_gateways().first() {
            let fields = Self::extract_ec2_egress_only_internet_gateway_attributes(eigw);

            let (_, model) = fields.into_model(
                id,
                Ec2EgressOnlyInternetGatewayReadModelSupplement {
                    tags: Self::ec2_tags_to_value(eigw.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Egress-Only Internet Gateway
    pub(crate) async fn create_ec2_egress_only_internet_gateway<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2EgressOnlyInternetGatewayCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (tags, vpc_id) = input.into_parts();

        let req = self
            .ec2_client
            .create_egress_only_internet_gateway()
            .configure(|builder| {
                builder.vpc_id(&vpc_id).tag_specifications(
                    build_tag_specification_with_create_token(
                        tags.as_ref(),
                        aws_sdk_ec2::types::ResourceType::EgressOnlyInternetGateway,
                        client_token,
                    ),
                )
            });
        let result = send_aws_operation_with_token(client_token, req).await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::EgressOnlyInternetGateway.create_context(client_token, sent),
            result,
            |output| {
                output
                    .egress_only_internet_gateway()
                    .and_then(|gateway| gateway.egress_only_internet_gateway_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 Egress-Only Internet Gateway (tags only)
    pub(crate) async fn update_ec2_egress_only_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2EgressOnlyInternetGatewayUpdateInput::decode(&to)?;
        let (tags,) = input.into_parts();
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self
            .read_ec2_egress_only_internet_gateway(&id, identifier)
            .await)
    }

    /// Delete an EC2 Egress-Only Internet Gateway
    pub(crate) async fn delete_ec2_egress_only_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.ec2_client
            .delete_egress_only_internet_gateway()
            .egress_only_internet_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete egress-only internet gateway", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }
}
