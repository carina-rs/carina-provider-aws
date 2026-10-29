use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, require_string_attr,
    retry_aws_operation,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::provider_generated::Ec2SecurityGroupReadFields;
use crate::read_model::{empty_map_value, found_managed_read, require_read_value};
use crate::read_models_generated::Ec2SecurityGroupReadModel;

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
            let Ec2SecurityGroupReadFields {
                description,
                group_id,
                group_name,
                vpc_id,
                applied_identifier: _,
            } = Self::extract_ec2_security_group_attributes(sg);

            let model = Ec2SecurityGroupReadModel {
                description: require_read_value(id, description, "description")?,
                group_name: require_read_value(id, group_name, "group_name")?,
                vpc_id,
                group_id,
                tags: Self::ec2_tags_to_value(sg.tags()).unwrap_or_else(empty_map_value),
            };

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
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let description = match resource.get_attr("description") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => String::new(),
        };

        // group_name is required for CreateSecurityGroup API
        let group_name = match resource.get_attr("group_name") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => match resource.id.identity_str() {
                Some(identity) => identity.to_string(),
                None => {
                    return Err(attempt.untracked(
                        ProviderError::invalid_input(
                        "security group without group_name requires a resolved resource identity",
                    )
                        .for_resource(resource.id.clone()),
                    ));
                }
            },
        };

        // Create Security Group
        let ec2 = &self.ec2_client;
        let tag_specification = build_tag_specification_with_create_token(
            resource,
            aws_sdk_ec2::types::ResourceType::SecurityGroup,
            client_token,
        );
        let sent = Self::sent_only(resource, &["vpc_id", "description", "group_name", "tags"]);
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let group_name = group_name.clone();
            let description = description.clone();
            let vpc_id = vpc_id.clone();
            ec2.create_security_group()
                .group_name(&group_name)
                .description(&description)
                .vpc_id(&vpc_id)
                .tag_specifications(tag_specification.clone())
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
