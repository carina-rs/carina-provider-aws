use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, SnapshotState,
    Value,
};
use carina_core::schema::CompiledResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::{AwsItemFailures, api_error_with_item_failure, api_error_with_meta};
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2VpcEndpointReadModelSupplement;
use crate::send_models_generated::{Ec2VpcEndpointCreateInput, Ec2VpcEndpointUpdateInput};
use aws_types::request_id::RequestId;
use carina_aws_types::sdk::ec2::operation::delete_vpc_endpoints::DeleteVpcEndpointsError;

fn decode_endpoint_policy(id: &ResourceId, raw: Option<&str>) -> ProviderResult<Option<Value>> {
    raw.map(crate::services::iam::role::iam_policy_json_to_value)
        .transpose()
        .map_err(|error| {
            ProviderError::api_error_caused_by("Failed to decode endpoint policy", error)
                .for_resource(id.clone())
        })
}

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

            let policy = decode_endpoint_policy(id, endpoint.policy_document())?;
            let fields =
                Self::extract_ec2_vpc_endpoint_attributes(endpoint).map_policy_document(|_| policy);
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
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
        _schema: &CompiledResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2VpcEndpointCreateInput::decode(resource, _schema) {
            Ok(input) => input,
            Err(error) => return Err(attempt.not_created(error)),
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
            Err(error) => return Err(attempt.not_created(error)),
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
                    carina_aws_types::sdk::ec2::types::ResourceType::VpcEndpoint,
                    client_token,
                ))
        });

        if let Some(ep_type) = vpc_endpoint_type {
            use carina_aws_types::sdk::ec2::types::VpcEndpointType;
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

        let result = retry_aws_operation_with_token(
            &self.diagnostics,
            client_token,
            RetryPolicy::default(),
            || req.clone(),
        )
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
        from: &SnapshotState,
        to: &crate::helpers::UpdateResource,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2VpcEndpointUpdateInput::decode(to)?;
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
        if let Some(failures) = AwsItemFailures::from_items(result.unsuccessful()) {
            return Err(api_error_with_item_failure::<DeleteVpcEndpointsError>(
                "Failed to delete VPC endpoint",
                failures,
                result.request_id(),
            )
            .for_resource(id));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::iam::role::IamPolicyDecodeError;
    use carina_core::provider::ProviderErrorKind;

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn delete_vpc_endpoints_keeps_typed_item_failure() {
        use crate::error_helpers::AwsItemFailures;
        use crate::test_http::provider;

        const AWS_MESSAGE: &str = "private VPC endpoint per-item diagnostic";
        const AWS_CODE: &str = "InvalidVpcEndpointState";

        let (provider, responses) = provider([(
            200,
            format!(
                r#"<DeleteVpcEndpointsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><unsuccessful><item><resourceId>vpce-b</resourceId><error><code>{AWS_CODE}</code><message>{AWS_MESSAGE}</message></error></item></unsuccessful></DeleteVpcEndpointsResponse>"#
            ),
        )]);
        let id = ResourceId::with_provider_identity("aws", "ec2.VpcEndpoint", "test", None);
        let error = provider
            .delete_ec2_vpc_endpoint(
                id,
                &AppliedIdentifier::new("vpce-0123456789abcdef0").unwrap(),
            )
            .await
            .unwrap_err();
        let error =
            carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);

        let requests = responses.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("Action=DeleteVpcEndpoints"));
        responses.assert_finished();
        assert_eq!(error.detail().context(), "Failed to delete VPC endpoint");
        let wire = error.to_wire();
        assert_eq!(wire.operation.as_deref(), Some("ec2.DeleteVpcEndpoints"));
        assert_eq!(wire.request_id.as_deref(), Some("test-request"));
        assert!(error.to_string().contains("vpce-b"));
        assert_eq!(
            error.detail().code().as_ref().map(|code| code.as_str()),
            Some(AWS_CODE)
        );
        assert!(
            error
                .detail()
                .cause()
                .expect("typed item failure")
                .as_cause()
                .matches_for_test::<AwsItemFailures>(|failures| failures.iter().count() == 1
                    && failures.first().code() == Some(AWS_CODE)
                    && failures.first().message() == Some(AWS_MESSAGE))
        );
    }

    #[test]
    fn endpoint_read_policy_decode_failure_keeps_its_typed_cause() {
        let id = ResourceId::with_provider_identity("aws", "ec2.VpcEndpoint", "test", None);
        for raw in [
            "",
            "{",
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":1e999}}}]}"#,
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":-1e999}}}]}"#,
        ] {
            let error = decode_endpoint_policy(&id, Some(raw))
                .expect_err("a malformed policy must fail the read");
            assert_eq!(error.kind(), ProviderErrorKind::ApiError);
            let decode = error
                .detail()
                .cause()
                .unwrap()
                .downcast_ref::<IamPolicyDecodeError>()
                .expect("typed IAM policy decode error");
            assert!(
                decode.matches_for_test::<IamPolicyDecodeError>(|error| matches!(
                    error,
                    IamPolicyDecodeError::Json(_)
                ))
            );
            assert!(decode.source().unwrap().is::<serde_json::Error>());
        }
    }

    #[test]
    fn endpoint_read_policy_preserves_absence_and_decodes_json() {
        let id = ResourceId::with_provider_identity("aws", "ec2.VpcEndpoint", "test", None);
        assert_eq!(decode_endpoint_policy(&id, None).unwrap(), None);
        assert_eq!(
            decode_endpoint_policy(&id, Some("{}")).unwrap(),
            Some(empty_map_value()),
        );
    }
}
