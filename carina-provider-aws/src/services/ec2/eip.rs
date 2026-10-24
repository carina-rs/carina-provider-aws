use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::optional_enum_attr;
use crate::provider::AwsCreateAttempt;

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
    pub(crate) async fn create_ec2_eip<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let mut req = self.ec2_client.allocate_address();

        let domain = match optional_enum_attr(resource, schema, "domain") {
            Ok(domain) => domain,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(domain) = domain {
            use aws_sdk_ec2::types::DomainType;
            req = req.domain(DomainType::from(domain));
        } else {
            // Default to VPC
            req = req.domain(aws_sdk_ec2::types::DomainType::Vpc);
        }

        let result = match req.send().await {
            Ok(result) => result,
            Err(error) => {
                return Err(attempt.untracked(
                    api_error_with_meta("Failed to allocate address", "ec2.AllocateAddress", error)
                        .for_resource(resource.id.clone()),
                ));
            }
        };

        let alloc_id = match result.allocation_id() {
            Some(alloc_id) => alloc_id,
            None => {
                return Err(attempt.untracked(
                    ProviderError::api_error("EIP allocated but no allocation ID returned")
                        .for_resource(resource.id.clone()),
                ));
            }
        };
        let identifier =
            match crate::helpers::applied_identifier(&resource.id, "ec2.AllocateAddress", alloc_id)
            {
                Ok(identifier) => identifier,
                Err(error) => return Err(attempt.untracked(error)),
            };
        let issued = attempt.issued(identifier.clone(), Self::sent_only(resource, &["domain"]));

        // Apply tags
        if let Err(error) = self
            .apply_ec2_tags(
                &resource.id,
                alloc_id,
                &resource.resolved_attributes(),
                None,
            )
            .await
        {
            return Err(self.failed_issued(&resource.id, issued, error).await);
        }

        // Read back using allocation ID
        let outcome = self.read_ec2_eip(&resource.id, &identifier).await;
        let completion = self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        );
        Ok(self.finish_issued(issued, completion))
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
