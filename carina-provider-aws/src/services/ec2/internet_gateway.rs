use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::build_tag_specification_with_create_token;
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

impl AwsProvider {
    /// Read an EC2 Internet Gateway
    pub(crate) async fn read_ec2_internet_gateway(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("internet-gateway-id")
            .values(identifier)
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
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value =
                Self::extract_ec2_internet_gateway_attributes(igw, &mut attributes);

            // Store attached VPC ID (from Attachments, not a direct member)
            if let Some(attachment) = igw.attachments().first()
                && let Some(vpc_id) = attachment.vpc_id()
            {
                attributes.insert(
                    "vpc_id".to_string(),
                    Value::Concrete(ConcreteValue::String(vpc_id.to_string().into())),
                );
            }

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(igw.tags()) {
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

    /// Create an EC2 Internet Gateway
    pub(crate) async fn create_ec2_internet_gateway<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        // Create Internet Gateway
        let sent = Self::sent_only(resource, &["tags"]);
        let result = self
            .ec2_client
            .create_internet_gateway()
            .tag_specifications(build_tag_specification_with_create_token(
                resource,
                aws_sdk_ec2::types::ResourceType::InternetGateway,
                client_token,
            ))
            .send()
            .await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::InternetGateway.create_context(client_token, sent),
            result,
            |output| {
                output
                    .internet_gateway()
                    .and_then(|gateway| gateway.internet_gateway_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    pub(crate) async fn continue_ec2_internet_gateway_create(
        &self,
        resource: &Resource,
        igw_id: &str,
    ) -> ProviderResult<()> {
        // Attach to VPC if specified
        if let Some(Value::Concrete(ConcreteValue::String(vpc_id))) = resource.get_attr("vpc_id") {
            let result = self
                .ec2_client
                .attach_internet_gateway()
                .internet_gateway_id(igw_id)
                .vpc_id(vpc_id.expose_plaintext())
                .send()
                .await;
            if let Err(error) = result {
                let error = api_error_with_meta("Failed to attach internet gateway", error)
                    .for_resource(resource.id.clone());
                return Err(error);
            }
        }
        Ok(())
    }

    /// Delete an EC2 Internet Gateway
    pub(crate) async fn delete_ec2_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        // Look up the IGW to check for VPC attachments before deleting
        let result = self
            .ec2_client
            .describe_internet_gateways()
            .internet_gateway_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe internet gateway", e)
                    .for_resource(id.clone())
            })?;

        if let Some(igw) = result.internet_gateways().first() {
            // Detach from VPC first
            if let Some(attachment) = igw.attachments().first()
                && let Some(vpc_id) = attachment.vpc_id()
            {
                self.ec2_client
                    .detach_internet_gateway()
                    .internet_gateway_id(identifier)
                    .vpc_id(vpc_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta("Failed to detach internet gateway", e)
                            .for_resource(id.clone())
                    })?;
            }
        }

        // Delete Internet Gateway
        self.ec2_client
            .delete_internet_gateway()
            .internet_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete internet gateway", e).for_resource(id.clone())
            })?;

        Ok(())
    }
}
