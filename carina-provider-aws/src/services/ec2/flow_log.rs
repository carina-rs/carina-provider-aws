use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::utils::extract_enum_value;

use crate::AwsProvider;
use crate::helpers::{build_tag_specification, sdk_error_message};

impl AwsProvider {
    /// Read an EC2 Flow Log
    pub(crate) async fn read_ec2_flow_log(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        use aws_sdk_ec2::types::Filter;

        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let filter = Filter::builder()
            .name("flow-log-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_flow_logs()
            .filter(filter)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to describe flow logs", &e))
                    .for_resource(id.clone())
            })?;

        if let Some(fl) = result.flow_logs().first() {
            let mut attributes = HashMap::new();

            let identifier_value = Self::extract_ec2_flow_log_attributes(fl, &mut attributes);

            // Convert the SDK's singular `resource_id` back into the schema's
            // `resource_ids` list. AWS's CreateFlowLogs input takes a list; the
            // describe response exposes one resource_id per FlowLog entity. The
            // generated extractor inserts the singular key, so replace it here
            // with a one-element list matching the schema.
            if let Some(Value::Concrete(ConcreteValue::String(rid))) =
                attributes.remove("resource_id")
            {
                attributes.insert(
                    "resource_ids".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::String(rid)])),
                );
            }

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(fl.tags()) {
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

    /// Create an EC2 Flow Log
    pub(crate) async fn create_ec2_flow_log(&self, resource: Resource) -> ProviderResult<State> {
        let resource_ids_val: Vec<String> = match resource.get_attr("resource_ids") {
            Some(Value::Concrete(ConcreteValue::List(items))) => items
                .iter()
                .filter_map(|v| {
                    if let Value::Concrete(ConcreteValue::String(s)) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        if resource_ids_val.is_empty() {
            return Err(ProviderError::invalid_input(
                "resource_ids must be a non-empty list of resource identifiers",
            )
            .for_resource(resource.id.clone()));
        }

        let resource_type_val = match resource.get_attr("resource_type") {
            Some(Value::Concrete(ConcreteValue::String(s))) => extract_enum_value(s).to_string(),
            _ => {
                return Err(ProviderError::invalid_input("resource_type is required")
                    .for_resource(resource.id.clone()));
            }
        };

        let mut req = self
            .ec2_client
            .create_flow_logs()
            .set_resource_ids(Some(resource_ids_val.clone()))
            .resource_type(aws_sdk_ec2::types::FlowLogsResourceType::from(
                resource_type_val.as_str(),
            ));

        if let Some(Value::Concrete(ConcreteValue::String(traffic_type))) =
            resource.get_attr("traffic_type")
        {
            use aws_sdk_ec2::types::TrafficType;
            let tt = TrafficType::from(extract_enum_value(traffic_type));
            req = req.traffic_type(tt);
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_dest_type))) =
            resource.get_attr("log_destination_type")
        {
            use aws_sdk_ec2::types::LogDestinationType;
            let raw = extract_enum_value(log_dest_type);
            // Map DSL snake_case enum values back to API hyphenated format
            let api_value = match raw {
                "cloud_watch_logs" => "cloud-watch-logs",
                "kinesis_data_firehose" => "kinesis-data-firehose",
                other => other,
            };
            let ldt = LogDestinationType::from(api_value);
            req = req.log_destination_type(ldt);
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_dest))) =
            resource.get_attr("log_destination")
        {
            req = req.log_destination(log_dest);
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_group))) =
            resource.get_attr("log_group_name")
        {
            req = req.log_group_name(log_group);
        }

        if let Some(Value::Concrete(ConcreteValue::String(perm_arn))) =
            resource.get_attr("deliver_logs_permission_arn")
        {
            req = req.deliver_logs_permission_arn(perm_arn);
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_format))) =
            resource.get_attr("log_format")
        {
            req = req.log_format(log_format);
        }

        if let Some(Value::Concrete(ConcreteValue::Int(interval))) =
            resource.get_attr("max_aggregation_interval")
        {
            req = req.max_aggregation_interval(*interval as i32);
        }

        // Apply tags via TagSpecifications
        if let Some(tag_spec) =
            build_tag_specification(&resource, aws_sdk_ec2::types::ResourceType::VpcFlowLog)
        {
            req = req.tag_specifications(tag_spec);
        }

        // Retry loop for IAM eventual consistency: newly created IAM roles may
        // not be usable immediately by create_flow_logs.
        let mut last_error = String::new();
        let mut result = None;
        for attempt in 0..12 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let resp = match req.clone().send().await {
                Ok(resp) => resp,
                Err(e) => {
                    let err_str = format!("{:?}", e);
                    last_error = err_str.clone();
                    // Retry on IAM propagation errors (check both Display and Debug output)
                    if err_str.contains("Unable to assume")
                        || err_str.contains("Not authorized")
                        || err_str.contains("Access Denied")
                    {
                        continue;
                    }
                    return Err(ProviderError::api_error(sdk_error_message(
                        "Failed to create flow logs",
                        &e,
                    ))
                    .for_resource(resource.id.clone()));
                }
            };

            // Check for unsuccessful items
            if let Some(err) = resp.unsuccessful().first() {
                let msg = err
                    .error()
                    .and_then(|e| e.message())
                    .unwrap_or("unknown error");
                let code = err.error().and_then(|e| e.code()).unwrap_or("");
                last_error = format!("{} ({})", msg, code);
                // Retry on IAM propagation errors
                if msg.contains("Unable to assume IAM role")
                    || msg.contains("Not authorized")
                    || msg.contains("Access Denied")
                    || code == "403"
                {
                    continue;
                }
                return Err(ProviderError::api_error(format!(
                    "Failed to create flow log: {}",
                    msg
                ))
                .for_resource(resource.id.clone()));
            }

            result = Some(resp);
            break;
        }

        let result = result.ok_or_else(|| {
            ProviderError::api_error(format!(
                "Failed to create flow log after retries: {}",
                last_error
            ))
            .for_resource(resource.id.clone())
        })?;

        let flow_log_id = result.flow_log_ids().first().ok_or_else(|| {
            ProviderError::api_error("Flow Log created but no ID returned")
                .for_resource(resource.id.clone())
        })?;

        // Read back
        self.read_ec2_flow_log(&resource.id, Some(flow_log_id))
            .await
    }

    /// Update an EC2 Flow Log (tags only - all other attributes are create_only)
    pub(crate) async fn update_ec2_flow_log(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_flow_log(&id, Some(identifier)).await
    }

    /// Delete an EC2 Flow Log
    pub(crate) async fn delete_ec2_flow_log(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .ec2_client
            .delete_flow_logs()
            .flow_log_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to delete flow logs", &e))
                    .for_resource(id.clone())
            })?;

        // Check for unsuccessful items
        if let Some(err) = result.unsuccessful().first() {
            let msg = err
                .error()
                .and_then(|e| e.message())
                .unwrap_or("unknown error");
            return Err(
                ProviderError::api_error(format!("Failed to delete flow log: {}", msg))
                    .for_resource(id.clone()),
            );
        }

        Ok(())
    }

    /// Extract ec2.FlowLog attributes from the SDK response.
    ///
    /// Lives here (not in `provider_generated.rs`) because the
    /// `resource_type` attribute is derived by string-matching the
    /// `resource_id` prefix and is only emitted while
    /// `flow_log_status == "ACTIVE"` — there is no Smithy structural
    /// feature that would let the codegen express that conditional
    /// derivation in one resource. `scan_manual_methods` picks this up
    /// by name and the codegen skips emitting a duplicate stub.
    pub(crate) fn extract_ec2_flow_log_attributes(
        obj: &aws_sdk_ec2::types::FlowLog,
        attributes: &mut HashMap<String, Value>,
    ) -> Option<String> {
        if let Some(v) = obj.flow_log_id() {
            attributes.insert(
                "flow_log_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.resource_id() {
            attributes.insert(
                "resource_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.traffic_type() {
            attributes.insert(
                "traffic_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.log_destination_type() {
            attributes.insert(
                "log_destination_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = obj.log_destination() {
            attributes.insert(
                "log_destination".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.log_group_name() {
            attributes.insert(
                "log_group_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.deliver_logs_permission_arn() {
            attributes.insert(
                "deliver_logs_permission_arn".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.log_format() {
            attributes.insert(
                "log_format".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = obj.max_aggregation_interval() {
            attributes.insert(
                "max_aggregation_interval".to_string(),
                Value::Concrete(ConcreteValue::Int(v as i64)),
            );
        }
        if let Some(v) = obj.flow_log_status()
            && v == "ACTIVE"
        {
            // Only emit resource_type for active flow logs; an inactive
            // record may carry a stale resource_id whose prefix no
            // longer reflects what the flow log actually targets.
            if let Some(rt) = obj.resource_id() {
                let resource_type_str = if rt.starts_with("vpc-") {
                    "VPC"
                } else if rt.starts_with("subnet-") {
                    "Subnet"
                } else if rt.starts_with("eni-") {
                    "NetworkInterface"
                } else {
                    ""
                };
                if !resource_type_str.is_empty() {
                    attributes.insert(
                        "resource_type".to_string(),
                        Value::Concrete(ConcreteValue::String(resource_type_str.to_string())),
                    );
                }
            }
        }
        obj.flow_log_id().map(String::from)
    }
}
