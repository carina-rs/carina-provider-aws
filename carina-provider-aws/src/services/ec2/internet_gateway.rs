use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::AwsCreateAttempt;

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
                api_error_with_meta(
                    "Failed to describe internet gateways",
                    "ec2.DescribeInternetGateways",
                    e,
                )
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
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        // Create Internet Gateway
        let result = match self.ec2_client.create_internet_gateway().send().await {
            Ok(result) => result,
            Err(error) => {
                return Err(attempt.untracked(
                    api_error_with_meta(
                        "Failed to create internet gateway",
                        "ec2.CreateInternetGateway",
                        error,
                    )
                    .for_resource(resource.id.clone()),
                ));
            }
        };

        let igw_id = match result
            .internet_gateway()
            .and_then(|igw| igw.internet_gateway_id())
        {
            Some(igw_id) => igw_id,
            None => {
                return Err(attempt.untracked(
                    ProviderError::api_error("Internet Gateway created but no ID returned")
                        .for_resource(resource.id.clone()),
                ));
            }
        };
        let identifier = match crate::helpers::applied_identifier(
            &resource.id,
            "ec2.CreateInternetGateway",
            igw_id,
        ) {
            Ok(identifier) => identifier,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let issued = attempt.issued(identifier.clone(), Self::sent_only(resource, &[]));

        // Apply tags
        if let Err(error) = self
            .apply_ec2_tags(&resource.id, igw_id, &resource.resolved_attributes(), None)
            .await
        {
            return Err(self.failed_issued(&resource.id, issued, error).await);
        }

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
                let error = api_error_with_meta(
                    "Failed to attach internet gateway",
                    "ec2.AttachInternetGateway",
                    error,
                )
                .for_resource(resource.id.clone());
                return Err(self.failed_issued(&resource.id, issued, error).await);
            }
        }

        // Read back using IGW ID (reliable identifier)
        let outcome = self
            .read_ec2_internet_gateway(&resource.id, &identifier)
            .await;
        let completion = self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        );
        Ok(self.finish_issued(issued, completion))
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
                api_error_with_meta(
                    "Failed to describe internet gateway",
                    "ec2.DescribeInternetGateways",
                    e,
                )
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
                        api_error_with_meta(
                            "Failed to detach internet gateway",
                            "ec2.DetachInternetGateway",
                            e,
                        )
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
                api_error_with_meta(
                    "Failed to delete internet gateway",
                    "ec2.DeleteInternetGateway",
                    e,
                )
                .for_resource(id.clone())
            })?;

        Ok(())
    }
}
