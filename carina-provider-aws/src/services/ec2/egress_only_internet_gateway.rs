use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome, Sent};
use carina_core::resource::{Resource, ResourceId, State};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, require_string_attr, send_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

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
            let mut attributes = HashMap::new();

            let _identifier_value =
                Self::extract_ec2_egress_only_internet_gateway_attributes(eigw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(eigw.tags()) {
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

    /// Create an EC2 Egress-Only Internet Gateway
    pub(crate) async fn create_ec2_egress_only_internet_gateway<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let req = self
            .ec2_client
            .create_egress_only_internet_gateway()
            .configure(|builder| {
                builder.vpc_id(&vpc_id).tag_specifications(
                    build_tag_specification_with_create_token(
                        resource,
                        aws_sdk_ec2::types::ResourceType::EgressOnlyInternetGateway,
                        client_token,
                    ),
                )
            });
        let result = send_aws_operation_with_token(client_token, req).await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::EgressOnlyInternetGateway
                .create_context(client_token, Sent::AllRequestAttributes),
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
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
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
