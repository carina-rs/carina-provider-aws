use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, CreateOutcome, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::optional_enum_attr;

impl AwsProvider {
    /// Read an EC2 Elastic IP
    pub(crate) async fn read_ec2_eip(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("allocation-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_addresses()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe addresses", "ec2.DescribeAddresses", e)
                    .for_resource(id.clone())
            })?;

        if let Some(addr) = result.addresses().first() {
            let mut attributes = HashMap::new();

            // Extract attributes
            let _identifier_value = Self::extract_ec2_eip_attributes(addr, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(addr.tags()) {
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

    /// Create an EC2 Elastic IP
    pub(crate) async fn create_ec2_eip(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateOutcome> {
        let mut req = self.ec2_client.allocate_address();

        if let Some(domain) = optional_enum_attr(resource, schema, "domain")? {
            use aws_sdk_ec2::types::DomainType;
            req = req.domain(DomainType::from(domain));
        } else {
            // Default to VPC
            req = req.domain(aws_sdk_ec2::types::DomainType::Vpc);
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to allocate address", "ec2.AllocateAddress", e)
                .for_resource(resource.id.clone())
        })?;

        let alloc_id = result.allocation_id().ok_or_else(|| {
            ProviderError::api_error("EIP allocated but no allocation ID returned")
                .for_resource(resource.id.clone())
        })?;

        // Apply tags
        self.apply_ec2_tags(
            &resource.id,
            alloc_id,
            &resource.resolved_attributes(),
            None,
        )
        .await?;

        // Read back using allocation ID
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "ec2.AllocateAddress", alloc_id)?;
        let outcome = self.read_ec2_eip(&resource.id, &identifier).await;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    /// Update an EC2 Elastic IP (tags only)
    pub(crate) async fn update_ec2_eip(
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
        Ok(self.read_ec2_eip(&id, identifier).await)
    }

    /// Delete an EC2 Elastic IP
    pub(crate) async fn delete_ec2_eip(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.ec2_client
            .release_address()
            .allocation_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to release address", "ec2.ReleaseAddress", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }
}
