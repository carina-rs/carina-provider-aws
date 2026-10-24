use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation};
use crate::provider::AwsCreateAttempt;

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
                api_error_with_meta(
                    "Failed to describe security groups",
                    "ec2.DescribeSecurityGroups",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(sg) = result.security_groups().first() {
            let mut attributes = HashMap::new();

            // Auto-generated attribute extraction
            let _identifier_value =
                Self::extract_ec2_security_group_attributes(sg, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(sg.tags()) {
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

    /// Create an EC2 Security Group
    pub(crate) async fn create_ec2_security_group<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
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
        let rid = resource.id.clone();
        let result =
            match retry_aws_operation("create security group", RetryPolicy::default(), || {
                let group_name = group_name.clone();
                let description = description.clone();
                let vpc_id = vpc_id.clone();
                async move {
                    ec2.create_security_group()
                        .group_name(&group_name)
                        .description(&description)
                        .vpc_id(&vpc_id)
                        .send()
                        .await
                }
            })
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    return Err(attempt.untracked(
                        api_error_with_meta(
                            "Failed to create security group",
                            "ec2.CreateSecurityGroup",
                            error,
                        )
                        .for_resource(rid),
                    ));
                }
            };

        let sg_id = match result.group_id() {
            Some(sg_id) => sg_id,
            None => {
                return Err(attempt.untracked(
                    ProviderError::api_error("Security Group created but no ID returned")
                        .for_resource(resource.id.clone()),
                ));
            }
        };
        let identifier = match crate::helpers::applied_identifier(
            &resource.id,
            "ec2.CreateSecurityGroup",
            sg_id,
        ) {
            Ok(identifier) => identifier,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let issued = attempt.issued(
            identifier.clone(),
            Self::sent_only(resource, &["vpc_id", "description", "group_name"]),
        );

        // Apply tags
        if let Err(error) = self
            .apply_ec2_tags(&resource.id, sg_id, &resource.resolved_attributes(), None)
            .await
        {
            return Err(self.failed_issued(&resource.id, issued, error).await);
        }

        // Read back using security group ID (reliable identifier)
        let outcome = self
            .read_ec2_security_group(&resource.id, &identifier)
            .await;
        let completion = self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        );
        Ok(self.finish_issued(issued, completion))
    }
}
