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
use crate::read_models_generated::Ec2EipReadModelSupplement;
use crate::send_models_generated::{Ec2EipCreateInput, Ec2EipUpdateInput};

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
                api_error_with_meta("Failed to describe addresses", e).for_resource(id.clone())
            })?;

        if let Some(addr) = result.addresses().first() {
            let fields = Self::extract_ec2_eip_attributes(addr);
            let (_, model) = fields.into_model(
                id,
                Ec2EipReadModelSupplement {
                    tags: Self::ec2_tags_to_value(addr.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Elastic IP
    pub(crate) async fn create_ec2_eip<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2EipCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (address, domain, public_ipv4_pool, tags) = input.into_parts();
        let mut req = self.ec2_client.allocate_address().tag_specifications(
            build_tag_specification_with_create_token(
                tags.as_ref(),
                aws_sdk_ec2::types::ResourceType::ElasticIp,
                client_token,
            ),
        );

        if let Some(address) = address {
            req = req.address(address);
        }
        if let Some(public_ipv4_pool) = public_ipv4_pool {
            req = req.public_ipv4_pool(public_ipv4_pool);
        }

        if let Some(domain) = domain {
            use aws_sdk_ec2::types::DomainType;
            req = req.domain(DomainType::from(domain.as_str()));
        } else {
            // Default to VPC
            req = req.domain(aws_sdk_ec2::types::DomainType::Vpc);
        }

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::Address.create_context(client_token, sent),
            result,
            |output| output.allocation_id().map(str::to_owned),
        )
        .await
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
        let input = Ec2EipUpdateInput::decode(&to)?;
        let (tags,) = input.into_parts();
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
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
                api_error_with_meta("Failed to release address", e).for_resource(id.clone())
            })?;
        Ok(())
    }
}
