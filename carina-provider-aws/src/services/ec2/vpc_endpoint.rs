use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, retry_aws_operation, sdk_error_message};

impl AwsProvider {
    /// Read an EC2 VPC Endpoint
    pub(crate) async fn read_ec2_vpc_endpoint(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .ec2_client
            .describe_vpc_endpoints()
            .vpc_endpoint_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to describe VPC endpoints", &e))
                    .for_resource(id.clone())
            })?;

        if let Some(endpoint) = result.vpc_endpoints().first() {
            // Skip deleted endpoints
            if endpoint.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(State::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let identifier_value =
                Self::extract_ec2_vpc_endpoint_attributes(endpoint, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(endpoint.tags()) {
                attributes.insert("tags".to_string(), tags_value);
            }

            let state = State::existing(id.clone(), attributes);
            Ok(if let Some(id_val) = identifier_value {
                state.with_identifier(id_val)
            } else {
                state
            })
        } else {
            Ok(State::not_found(id.clone()))
        }
    }

    /// Create an EC2 VPC Endpoint
    pub(crate) async fn create_ec2_vpc_endpoint(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let vpc_id = require_string_attr(&resource, "vpc_id")?;
        let service_name = require_string_attr(&resource, "service_name")?;

        let mut req = self
            .ec2_client
            .create_vpc_endpoint()
            .vpc_id(&vpc_id)
            .service_name(&service_name);

        if let Some(Value::Concrete(ConcreteValue::String(ep_type))) =
            resource.get_attr("vpc_endpoint_type")
        {
            use aws_sdk_ec2::types::VpcEndpointType;
            req = req.vpc_endpoint_type(VpcEndpointType::from(ep_type.as_str()));
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) =
            resource.get_attr("route_table_ids")
        {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.route_table_ids(s);
                }
            }
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) = resource.get_attr("subnet_ids") {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.subnet_ids(s);
                }
            }
        }

        if let Some(Value::Concrete(ConcreteValue::List(ids))) =
            resource.get_attr("security_group_ids")
        {
            for id_val in ids {
                if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                    req = req.security_group_ids(s);
                }
            }
        }

        if let Some(Value::Concrete(ConcreteValue::Bool(v))) =
            resource.get_attr("private_dns_enabled")
        {
            req = req.private_dns_enabled(*v);
        }

        if let Some(Value::Concrete(ConcreteValue::String(policy))) =
            resource.get_attr("policy_document")
        {
            req = req.policy_document(policy);
        } else if let Some(Value::Concrete(ConcreteValue::Map(map))) =
            resource.get_attr("policy_document")
        {
            // Convert Value::Map to JSON string for the API
            let json_str = crate::services::iam::role::value_to_iam_policy_json(&Value::Concrete(
                ConcreteValue::Map(map.clone()),
            ))
            .map_err(|e| {
                ProviderError::internal(format!("Failed to serialize policy_document: {}", e))
                    .for_resource(resource.id.clone())
            })?;
            req = req.policy_document(&json_str);
        }

        let rid = resource.id.clone();
        let result = retry_aws_operation("create VPC endpoint", 5, 5, || {
            let req = req.clone();
            async move { req.send().await }
        })
        .await
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to create VPC endpoint", &e))
                .for_resource(rid.clone())
        })?;

        let endpoint_id = result
            .vpc_endpoint()
            .and_then(|ep| ep.vpc_endpoint_id())
            .ok_or_else(|| {
                ProviderError::api_error("VPC Endpoint created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Apply tags
        self.apply_ec2_tags(
            &resource.id,
            endpoint_id,
            &resource.resolved_attributes(),
            None,
        )
        .await?;

        // Read back
        self.read_ec2_vpc_endpoint(&resource.id, Some(endpoint_id))
            .await
    }

    /// Update an EC2 VPC Endpoint
    pub(crate) async fn update_ec2_vpc_endpoint(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: ManagedResource,
    ) -> ProviderResult<State> {
        let mut req = self
            .ec2_client
            .modify_vpc_endpoint()
            .vpc_endpoint_id(identifier);

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
                            Some(s.clone())
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
                        Some(s.clone())
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
                            Some(s.clone())
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
                        Some(s.clone())
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
                            Some(s.clone())
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
                        Some(s.clone())
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
        if let Some(Value::Concrete(ConcreteValue::Bool(v))) = to.get_attr("private_dns_enabled") {
            req = req.private_dns_enabled(*v);
            has_modifications = true;
        }

        // Update policy_document
        if let Some(Value::Concrete(ConcreteValue::String(policy))) = to.get_attr("policy_document")
        {
            req = req.policy_document(policy);
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
                ProviderError::api_error(sdk_error_message("Failed to modify VPC endpoint", &e))
                    .for_resource(id.clone())
            })?;
        }

        // Apply tags
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;

        self.read_ec2_vpc_endpoint(&id, Some(identifier)).await
    }

    /// Delete an EC2 VPC Endpoint
    pub(crate) async fn delete_ec2_vpc_endpoint(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .ec2_client
            .delete_vpc_endpoints()
            .vpc_endpoint_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to delete VPC endpoint", &e))
                    .for_resource(id.clone())
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

    /// Extract ec2.VpcEndpoint attributes from the SDK response.
    ///
    /// Lives here (not in `provider_generated.rs`) because
    /// `policy_document` is parsed JSON → `Value::Map` via
    /// `iam_policy_json_to_value`, which the codegen template can't
    /// express. `scan_manual_methods` picks this up by name and the
    /// codegen skips emitting a duplicate stub. The list-typed members
    /// (route_table_ids / subnet_ids / Groups[*].group_id) are also
    /// extracted here for the same reason — keeping the extractor
    /// whole reads cleaner than re-deriving half of it from
    /// `derived_attributes`.
    pub(crate) fn extract_ec2_vpc_endpoint_attributes(
        obj: &aws_sdk_ec2::types::VpcEndpoint,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.vpc_endpoint_id() {
            attributes.insert(
                "vpc_endpoint_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.vpc_endpoint_type() {
            attributes.insert(
                "vpc_endpoint_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.vpc_id() {
            attributes.insert(
                "vpc_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.service_name() {
            attributes.insert(
                "service_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.private_dns_enabled() {
            attributes.insert(
                "private_dns_enabled".to_string(),
                Value::Concrete(ConcreteValue::Bool(v)),
            );
        }
        if let Some(v) = obj.policy_document() {
            // Try to parse the policy document JSON into a Value::Map.
            let policy_value = crate::services::iam::role::iam_policy_json_to_value(v)
                .unwrap_or_else(|_| Value::Concrete(ConcreteValue::String(v.to_string())));
            attributes.insert("policy_document".to_string(), policy_value);
        }
        {
            let ids = obj.route_table_ids();
            if !ids.is_empty() {
                let list: Vec<Value> = ids
                    .iter()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string())))
                    .collect();
                attributes.insert(
                    "route_table_ids".to_string(),
                    Value::Concrete(ConcreteValue::List(list)),
                );
            }
        }
        {
            let ids = obj.subnet_ids();
            if !ids.is_empty() {
                let list: Vec<Value> = ids
                    .iter()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.to_string())))
                    .collect();
                attributes.insert(
                    "subnet_ids".to_string(),
                    Value::Concrete(ConcreteValue::List(list)),
                );
            }
        }
        // Extract security group IDs from Groups[*].group_id.
        {
            let groups = obj.groups();
            if !groups.is_empty() {
                let list: Vec<Value> = groups
                    .iter()
                    .filter_map(|g| {
                        g.group_id()
                            .map(|id| Value::Concrete(ConcreteValue::String(id.to_string())))
                    })
                    .collect();
                if !list.is_empty() {
                    attributes.insert(
                        "security_group_ids".to_string(),
                        Value::Concrete(ConcreteValue::List(list)),
                    );
                }
            }
        }
        obj.vpc_endpoint_id().map(String::from)
    }
}
