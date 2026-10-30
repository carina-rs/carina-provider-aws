use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, build_tag_specification_with_create_token, retry_aws_operation};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2SecurityGroupReadModelSupplement;
use crate::send_models_generated::Ec2SecurityGroupCreateInput;

impl AwsProvider {
    /// Read an EC2 Security Group
    pub(crate) async fn read_ec2_security_group(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("group-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_security_groups()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe security groups", e)
                    .for_resource(id.clone())
            })?;

        if let Some(sg) = result.security_groups().first() {
            let fields = Self::extract_ec2_security_group_attributes(sg);

            let (_, model) = fields.into_model(
                id,
                Ec2SecurityGroupReadModelSupplement {
                    tags: Self::ec2_tags_to_value(sg.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Security Group
    pub(crate) async fn create_ec2_security_group<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2SecurityGroupCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (description, group_name, tags, vpc_id) = input.into_parts();

        // Create Security Group
        let ec2 = &self.ec2_client;
        let tag_specification = build_tag_specification_with_create_token(
            tags.as_ref(),
            aws_sdk_ec2::types::ResourceType::SecurityGroup,
            client_token,
        );
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let group_name = group_name.clone();
            let description = description.clone();
            let vpc_id = vpc_id.clone();
            let mut request = ec2
                .create_security_group()
                .group_name(&group_name)
                .description(&description)
                .tag_specifications(tag_specification.clone());
            if let Some(vpc_id) = &vpc_id {
                request = request.vpc_id(vpc_id);
            }
            request
        })
        .await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::SecurityGroup.create_context(client_token, sent),
            result,
            |output| output.group_id().map(str::to_owned),
        )
        .await
    }
}
