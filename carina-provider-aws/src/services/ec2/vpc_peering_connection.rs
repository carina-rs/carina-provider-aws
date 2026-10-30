use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId, State};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::build_tag_specification_with_create_token;
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2VpcPeeringConnectionReadModelSupplement;
use crate::send_models_generated::{
    Ec2VpcPeeringConnectionCreateInput, Ec2VpcPeeringConnectionUpdateInput,
};

impl AwsProvider {
    /// Read an EC2 VPC Peering Connection
    pub(crate) async fn read_ec2_vpc_peering_connection(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let result = self
            .ec2_client
            .describe_vpc_peering_connections()
            .vpc_peering_connection_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe VPC peering connections", e)
                    .for_resource(id.clone())
            })?;

        if let Some(pcx) = result.vpc_peering_connections().first() {
            // Skip deleted/failed peering connections
            let status_code = pcx
                .status()
                .and_then(|s| s.code())
                .map(|c| c.as_str().to_string());
            if matches!(
                status_code.as_deref(),
                Some("deleted") | Some("failed") | Some("rejected") | Some("expired")
            ) {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let fields = Self::extract_ec2_vpc_peering_connection_attributes(pcx);

            let (_, model) = fields.into_model(
                id,
                Ec2VpcPeeringConnectionReadModelSupplement {
                    tags: Self::ec2_tags_to_value(pcx.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPC Peering Connection
    pub(crate) async fn create_ec2_vpc_peering_connection<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2VpcPeeringConnectionCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (peer_owner_id, peer_vpc_id, tags, vpc_id) = input.into_parts();

        let mut req = self
            .ec2_client
            .create_vpc_peering_connection()
            .vpc_id(&vpc_id)
            .peer_vpc_id(&peer_vpc_id);

        if let Some(owner_id) = peer_owner_id {
            req = req.peer_owner_id(owner_id);
        }

        // `peer_region` is excluded from the schema (resource_defs.rs
        // `exclude_fields: ["DryRun", "TagSpecifications", "PeerRegion"]`),
        // so a valid DSL cannot set it and this read would never fire.
        // Cross-region peering would need `peer_region` declared as a
        // schema attribute + a matching read-side projection;
        // carina-provider-aws#447.

        req = req.tag_specifications(build_tag_specification_with_create_token(
            tags.as_ref(),
            aws_sdk_ec2::types::ResourceType::VpcPeeringConnection,
            client_token,
        ));

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::VpcPeeringConnection.create_context(client_token, sent),
            result,
            |output| {
                output
                    .vpc_peering_connection()
                    .and_then(|connection| connection.vpc_peering_connection_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 VPC Peering Connection (tags only)
    pub(crate) async fn update_ec2_vpc_peering_connection(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2VpcPeeringConnectionUpdateInput::decode(&to)?;
        let (tags,) = input.into_parts();
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_vpc_peering_connection(&id, identifier).await)
    }

    /// Delete an EC2 VPC Peering Connection
    pub(crate) async fn delete_ec2_vpc_peering_connection(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.ec2_client
            .delete_vpc_peering_connection()
            .vpc_peering_connection_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete VPC peering connection", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }
}
