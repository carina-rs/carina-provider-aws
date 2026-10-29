use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, build_tag_specification_with_create_token, optional_bool_attr, optional_enum_attr,
    optional_string_attr, require_string_attr, retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2VpcEndpointReadModelSupplement;

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
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let service_name = match require_string_attr(resource, "service_name") {
            Ok(service_name) => service_name,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let mut req = self.ec2_client.create_vpc_endpoint().configure(|builder| {
            builder
                .vpc_id(&vpc_id)
                .service_name(&service_name)
                .tag_specifications(build_tag_specification_with_create_token(
                    resource,
                    aws_sdk_ec2::types::ResourceType::VpcEndpoint,
                    client_token,
                ))
        });

        let endpoint_type = match optional_enum_attr(resource, schema, "vpc_endpoint_type") {
            Ok(endpoint_type) => endpoint_type,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(ep_type) = endpoint_type {
            use aws_sdk_ec2::types::VpcEndpointType;
            req =
                req.configure(|builder| builder.vpc_endpoint_type(VpcEndpointType::from(ep_type)));
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) =
            resource.get_attr("route_table_ids")
        {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.configure(|builder| builder.route_table_ids(s.expose_plaintext()));
                }
            }
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) = resource.get_attr("subnet_ids") {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.configure(|builder| builder.subnet_ids(s.expose_plaintext()));
                }
            }
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) =
            resource.get_attr("security_group_ids")
        {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.configure(|builder| builder.security_group_ids(s.expose_plaintext()));
                }
            }
        }

        if let Some(v) = optional_bool_attr(resource, "private_dns_enabled") {
            req = req.configure(|builder| builder.private_dns_enabled(v));
        }

        if let Some(Value::Concrete(ConcreteValue::String(policy))) =
            resource.get_attr("policy_document")
        {
            req = req.configure(|builder| builder.policy_document(policy.expose_plaintext()));
        } else if let Some(Value::Concrete(ConcreteValue::Map(map))) =
            resource.get_attr("policy_document")
        {
            // Convert Value::Map to JSON string for the API
            let json_str = match crate::services::iam::role::value_to_iam_policy_json(
                &Value::Concrete(ConcreteValue::Map(map.clone())),
            ) {
                Ok(json) => json,
                Err(error) => {
                    return Err(attempt.untracked(
                        ProviderError::internal(format!(
                            "Failed to serialize policy_document: {error}"
                        ))
                        .for_resource(resource.id.clone()),
                    ));
                }
            };
            req = req.configure(|builder| builder.policy_document(&json_str));
        }

        let resource_configuration_arn =
            match optional_string_attr(resource, "resource_configuration_arn") {
                Ok(value) => value,
                Err(error) => return Err(attempt.untracked(error)),
            };
        if let Some(value) = resource_configuration_arn {
            req = req.configure(|builder| builder.resource_configuration_arn(value));
        }
        let service_network_arn = match optional_string_attr(resource, "service_network_arn") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(value) = service_network_arn {
            req = req.configure(|builder| builder.service_network_arn(value));
        }
        let service_region = match optional_enum_attr(resource, schema, "service_region") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(value) = service_region {
            req = req.configure(|builder| builder.service_region(value));
        }

        let result =
            retry_aws_operation_with_token(client_token, RetryPolicy::default(), || req.clone())
                .await;
        let sent = self.sent_only(
            resource,
            &[
                "vpc_id",
                "service_name",
                "vpc_endpoint_type",
                "route_table_ids",
                "subnet_ids",
                "security_group_ids",
                "private_dns_enabled",
                "policy_document",
                "resource_configuration_arn",
                "service_network_arn",
                "service_region",
                "tags",
            ],
        );

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::VpcEndpoint.create_context(client_token, sent),
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
        let mut req = self
            .ec2_client
            .modify_vpc_endpoint()
            .vpc_endpoint_id(identifier.as_str());

        let mut has_modifications = false;

        // Update route_table_ids
        if let Some(Value::Concrete(ConcreteValue::List(new_ids))) = to.get_attr("route_table_ids")
        {
            let old_ids: Vec<String> = if let Some(Value::Concrete(ConcreteValue::List(old))) =
                from.attributes.get("route_table_ids")
            {
                old.iter()
                    .filter_map(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s.expose_plaintext().to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![]
            };
            let new_id_strs: Vec<String> = new_ids
                .iter()
                .filter_map(|v| {
                    if let Value::Concrete(ConcreteValue::String(s)) = v {
                        Some(s.expose_plaintext().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            for id_val in &new_id_strs {
                if !old_ids.contains(id_val) {
                    req = req.add_route_table_ids(id_val);
                    has_modifications = true;
                }
            }
            for id_val in &old_ids {
                if !new_id_strs.contains(id_val) {
                    req = req.remove_route_table_ids(id_val);
                    has_modifications = true;
                }
            }
        }

        // Update subnet_ids
        if let Some(Value::Concrete(ConcreteValue::List(new_ids))) = to.get_attr("subnet_ids") {
            let old_ids: Vec<String> = if let Some(Value::Concrete(ConcreteValue::List(old))) =
                from.attributes.get("subnet_ids")
            {
                old.iter()
                    .filter_map(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s.expose_plaintext().to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![]
            };
            let new_id_strs: Vec<String> = new_ids
                .iter()
                .filter_map(|v| {
                    if let Value::Concrete(ConcreteValue::String(s)) = v {
                        Some(s.expose_plaintext().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            for id_val in &new_id_strs {
                if !old_ids.contains(id_val) {
                    req = req.add_subnet_ids(id_val);
                    has_modifications = true;
                }
            }
            for id_val in &old_ids {
                if !new_id_strs.contains(id_val) {
                    req = req.remove_subnet_ids(id_val);
                    has_modifications = true;
                }
            }
        }

        // Update security_group_ids
        if let Some(Value::Concrete(ConcreteValue::List(new_ids))) =
            to.get_attr("security_group_ids")
        {
            let old_ids: Vec<String> = if let Some(Value::Concrete(ConcreteValue::List(old))) =
                from.attributes.get("security_group_ids")
            {
                old.iter()
                    .filter_map(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s.expose_plaintext().to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![]
            };
            let new_id_strs: Vec<String> = new_ids
                .iter()
                .filter_map(|v| {
                    if let Value::Concrete(ConcreteValue::String(s)) = v {
                        Some(s.expose_plaintext().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            for id_val in &new_id_strs {
                if !old_ids.contains(id_val) {
                    req = req.add_security_group_ids(id_val);
                    has_modifications = true;
                }
            }
            for id_val in &old_ids {
                if !new_id_strs.contains(id_val) {
                    req = req.remove_security_group_ids(id_val);
                    has_modifications = true;
                }
            }
        }

        // Update private_dns_enabled
        if let Some(v) = optional_bool_attr(&to, "private_dns_enabled") {
            req = req.private_dns_enabled(v);
            has_modifications = true;
        }

        // Update policy_document
        if let Some(Value::Concrete(ConcreteValue::String(policy))) = to.get_attr("policy_document")
        {
            req = req.policy_document(policy.expose_plaintext());
            has_modifications = true;
        } else if let Some(Value::Concrete(ConcreteValue::Map(map))) =
            to.get_attr("policy_document")
        {
            let json_str = crate::services::iam::role::value_to_iam_policy_json(&Value::Concrete(
                ConcreteValue::Map(map.clone()),
            ))
            .map_err(|e| {
                ProviderError::internal(format!("Failed to serialize policy_document: {}", e))
                    .for_resource(id.clone())
            })?;
            req = req.policy_document(&json_str);
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
            &to.resolved_attributes(),
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
