use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2VpcEndpointReadModelSupplement;
use crate::send_models_generated::{Ec2VpcEndpointCreateInput, Ec2VpcEndpointUpdateInput};

impl AwsProvider {
    /// Read an EC2 VPC Endpoint
    pub(crate) async fn read_ec2_vpc_endpoint(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let result = self
            .ec2_client
            .describe_vpc_endpoints()
            .vpc_endpoint_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe VPC endpoints", e).for_resource(id.clone())
            })?;

        if let Some(endpoint) = result.vpc_endpoints().first() {
            // Skip deleted endpoints
            if endpoint.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let fields = Self::extract_ec2_vpc_endpoint_attributes(endpoint).map_policy_document(
                |policy_document| {
                    policy_document.map(|value| match value {
                        Value::Concrete(ConcreteValue::String(raw)) => {
                            crate::services::iam::role::iam_policy_json_to_value(
                                raw.expose_plaintext(),
                            )
                            .unwrap_or(Value::Concrete(ConcreteValue::String(raw)))
                        }
                        value => value,
                    })
                },
            );
            let empty_list = || Value::Concrete(ConcreteValue::List(Vec::new()));
            let fields = fields
                .map_route_table_ids(|value| Some(value.unwrap_or_else(empty_list)))
                .map_security_group_ids(|value| Some(value.unwrap_or_else(empty_list)))
                .map_subnet_ids(|value| Some(value.unwrap_or_else(empty_list)));
            let (_, model) = fields.into_model(
                id,
                Ec2VpcEndpointReadModelSupplement {
                    tags: Self::ec2_tags_to_value(endpoint.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPC Endpoint
    pub(crate) async fn create_ec2_vpc_endpoint<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2VpcEndpointCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let crate::send_models_generated::Ec2VpcEndpointCreateParts {
            policy_document,
            private_dns_enabled,
            resource_configuration_arn,
            route_table_ids,
            security_group_ids,
            service_name,
            service_network_arn,
            service_region,
            subnet_ids,
            tags,
            vpc_endpoint_type,
            vpc_id,
        } = input.into_parts();
        let policy_document = policy_document.into_inner();
        let policy_document = match policy_document
            .as_ref()
            .map(|value| {
                crate::services::iam::role::resolve_iam_policy_value(
                    &resource.id,
                    "policy_document",
                    value,
                )
            })
            .transpose()
        {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let private_dns_enabled = private_dns_enabled.into_inner();
        let resource_configuration_arn = resource_configuration_arn.into_inner();
        let route_table_ids = route_table_ids.into_inner();
        let security_group_ids = security_group_ids.into_inner();
        let service_name = service_name.into_inner();
        let service_network_arn = service_network_arn.into_inner();
        let service_region = service_region.into_inner();
        let subnet_ids = subnet_ids.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let vpc_endpoint_type = vpc_endpoint_type.into_inner();
        let vpc_id = vpc_id.into_inner();

        let mut req = self.ec2_client.create_vpc_endpoint().configure(|builder| {
            builder
                .vpc_id(&vpc_id)
                .service_name(&service_name)
                .tag_specifications(build_tag_specification_with_create_token(
                    tags.as_ref(),
                    aws_sdk_ec2::types::ResourceType::VpcEndpoint,
                    client_token,
                ))
        });

        if let Some(ep_type) = vpc_endpoint_type {
            use aws_sdk_ec2::types::VpcEndpointType;
            req = req.configure(|builder| {
                builder.vpc_endpoint_type(VpcEndpointType::from(ep_type.as_str()))
            });
        }

        for route_table_id in route_table_ids.unwrap_or_default() {
            req = req.configure(|builder| builder.route_table_ids(route_table_id));
        }

        for subnet_id in subnet_ids.unwrap_or_default() {
            req = req.configure(|builder| builder.subnet_ids(subnet_id));
        }

        for security_group_id in security_group_ids.unwrap_or_default() {
            req = req.configure(|builder| builder.security_group_ids(security_group_id));
        }

        if let Some(v) = private_dns_enabled {
            req = req.configure(|builder| builder.private_dns_enabled(v));
        }

        if let Some(policy_document) = policy_document {
            req = req.configure(|builder| builder.policy_document(policy_document));
        }

        if let Some(value) = resource_configuration_arn {
            req = req.configure(|builder| builder.resource_configuration_arn(value));
        }
        if let Some(value) = service_network_arn {
            req = req.configure(|builder| builder.service_network_arn(value));
        }
        if let Some(value) = service_region {
            req = req.configure(|builder| builder.service_region(value));
        }

        let result =
            retry_aws_operation_with_token(client_token, RetryPolicy::default(), || req.clone())
                .await;
        self.finish_tagged_ec2_create(
            resource,
            attempt,
            TaggedEc2Create::VpcEndpoint.create_context(client_token, sent),
            result,
            |output| {
                output
                    .vpc_endpoint()
                    .and_then(|endpoint| endpoint.vpc_endpoint_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 VPC Endpoint
    pub(crate) async fn update_ec2_vpc_endpoint(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2VpcEndpointUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2VpcEndpointUpdateParts {
            policy_document,
            private_dns_enabled,
            tags,
        } = input.into_parts();
        let policy_document = policy_document.into_inner();
        let policy_document = policy_document
            .as_ref()
            .map(|value| {
                crate::services::iam::role::resolve_iam_policy_value(&id, "policy_document", value)
            })
            .transpose()?;
        let private_dns_enabled = private_dns_enabled.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let mut req = self
            .ec2_client
            .modify_vpc_endpoint()
            .vpc_endpoint_id(identifier.as_str());

        let mut has_modifications = false;

        if let Some(v) = private_dns_enabled {
            req = req.private_dns_enabled(v);
            has_modifications = true;
        }

        if let Some(policy_document) = policy_document {
            req = req.policy_document(policy_document);
            has_modifications = true;
        }

        if has_modifications {
            req.send().await.map_err(|e| {
                api_error_with_meta("Failed to modify VPC endpoint", e).for_resource(id.clone())
            })?;
        }

        // Apply tags
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;

        Ok(self.read_ec2_vpc_endpoint(&id, identifier).await)
    }

    /// Delete an EC2 VPC Endpoint
    pub(crate) async fn delete_ec2_vpc_endpoint(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = self
            .ec2_client
            .delete_vpc_endpoints()
            .vpc_endpoint_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete VPC endpoint", e).for_resource(id.clone())
            })?;

        // Check for unsuccessful items
        if let Some(err) = result.unsuccessful().first() {
            let msg = err
                .error()
                .and_then(|e| e.message())
                .unwrap_or("unknown error");
            return Err(ProviderError::api_error(format!(
                "Failed to delete VPC endpoint: {}",
                msg
            ))
            .for_resource(id.clone()));
        }

        Ok(())
    }
}
