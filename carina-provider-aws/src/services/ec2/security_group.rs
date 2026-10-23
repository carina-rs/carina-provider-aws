use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, CreateOutcome, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation};

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
    pub(crate) async fn create_ec2_security_group(
        &self,
        resource: &Resource,
    ) -> ProviderResult<CreateOutcome> {
        let vpc_id = require_string_attr(resource, "vpc_id")?;

        let description = match resource.get_attr("description") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => String::new(),
        };

        // group_name is required for CreateSecurityGroup API
        let group_name = match resource.get_attr("group_name") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
            _ => resource
                .id
                .identity_str()
                .ok_or_else(|| {
                    ProviderError::invalid_input(
                        "security group without group_name requires a resolved resource identity",
                    )
                    .for_resource(resource.id.clone())
                })?
                .to_string(),
        };

        // Create Security Group
        let ec2 = &self.ec2_client;
        let rid = resource.id.clone();
        let result = retry_aws_operation("create security group", RetryPolicy::default(), || {
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
        .map_err(|e| {
            api_error_with_meta(
                "Failed to create security group",
                "ec2.CreateSecurityGroup",
                e,
            )
            .for_resource(rid.clone())
        })?;

        let sg_id = result.group_id().ok_or_else(|| {
            ProviderError::api_error("Security Group created but no ID returned")
                .for_resource(resource.id.clone())
        })?;

        // Apply tags
        self.apply_ec2_tags(&resource.id, sg_id, &resource.resolved_attributes(), None)
            .await?;

        // Read back using security group ID (reliable identifier)
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "ec2.CreateSecurityGroup", sg_id)?;
        let outcome = self
            .read_ec2_security_group(&resource.id, &identifier)
            .await;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }
}
