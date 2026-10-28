use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, optional_enum_attr, require_enum_attr,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

impl AwsProvider {
    /// Read an EC2 Flow Log
    pub(crate) async fn read_ec2_flow_log(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let flow_log_ids = identifier
            .as_str()
            .split(',')
            .map(str::to_string)
            .collect::<Vec<_>>();

        // Measured with account carina-test-009 in ap-northeast-1 on
        // 2026-10-24: both
        // `aws ec2 describe-flow-logs --flow-log-ids <unknown>` and
        // `aws ec2 describe-flow-logs --filters Name=flow-log-id,Values=<unknown>`
        // returned `{"FlowLogs": []}`. An empty response is therefore the
        // service's not-found signal for this read.
        let result = self
            .ec2_client
            .describe_flow_logs()
            .set_flow_log_ids(Some(flow_log_ids))
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe flow logs", "ec2.DescribeFlowLogs", e)
                    .for_resource(id.clone())
            })?;

        if let Some(fl) = result.flow_logs().first() {
            let mut attributes = HashMap::new();

            let _identifier_value = Self::extract_ec2_flow_log_attributes(fl, &mut attributes);

            // A single CreateFlowLogs request can create one flow log per
            // requested resource. Preserve the complete set under one provider
            // identifier and reconstruct the schema's plural resource_ids.
            attributes.remove("resource_id");
            let resource_ids = result
                .flow_logs()
                .iter()
                .filter_map(|flow_log| flow_log.resource_id())
                .map(|resource_id| {
                    Value::Concrete(ConcreteValue::String(resource_id.to_string().into()))
                })
                .collect::<Vec<_>>();
            if !resource_ids.is_empty() {
                attributes.insert(
                    "resource_ids".to_string(),
                    Value::Concrete(ConcreteValue::List(resource_ids)),
                );
            }

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(fl.tags()) {
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

    /// Create an EC2 Flow Log
    pub(crate) async fn create_ec2_flow_log<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let resource_ids_val: Vec<String> = match resource.get_attr("resource_ids") {
            Some(Value::Concrete(ConcreteValue::List(items))) => items
                .iter()
                .filter_map(|v| {
                    if let Value::Concrete(ConcreteValue::String(s)) = v {
                        Some(s.expose_plaintext().to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        if resource_ids_val.is_empty() {
            return Err(attempt.untracked(
                ProviderError::invalid_input(
                    "resource_ids must be a non-empty list of resource identifiers",
                )
                .for_resource(resource.id.clone()),
            ));
        }

        let resource_type_val = match require_enum_attr(resource, schema, "resource_type") {
            Ok(resource_type) => resource_type,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let mut req = self
            .ec2_client
            .create_flow_logs()
            .set_resource_ids(Some(resource_ids_val.clone()))
            .client_token(client_token.as_str())
            .resource_type(aws_sdk_ec2::types::FlowLogsResourceType::from(
                resource_type_val.as_str(),
            ));

        let traffic_type = match optional_enum_attr(resource, schema, "traffic_type") {
            Ok(traffic_type) => traffic_type,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(traffic_type) = traffic_type {
            use aws_sdk_ec2::types::TrafficType;
            req = req.traffic_type(TrafficType::from(traffic_type));
        }

        let log_destination_type =
            match optional_enum_attr(resource, schema, "log_destination_type") {
                Ok(destination_type) => destination_type,
                Err(error) => return Err(attempt.untracked(error)),
            };
        if let Some(log_dest_type) = log_destination_type {
            use aws_sdk_ec2::types::LogDestinationType;
            req = req.log_destination_type(LogDestinationType::from(log_dest_type));
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_dest))) =
            resource.get_attr("log_destination")
        {
            req = req.log_destination(log_dest.expose_plaintext());
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_group))) =
            resource.get_attr("log_group_name")
        {
            req = req.log_group_name(log_group.expose_plaintext());
        }

        if let Some(Value::Concrete(ConcreteValue::String(perm_arn))) =
            resource.get_attr("deliver_logs_permission_arn")
        {
            req = req.deliver_logs_permission_arn(perm_arn.expose_plaintext());
        }

        if let Some(Value::Concrete(ConcreteValue::String(log_format))) =
            resource.get_attr("log_format")
        {
            req = req.log_format(log_format.expose_plaintext());
        }

        if let Some(Value::Concrete(ConcreteValue::Int(interval))) =
            resource.get_attr("max_aggregation_interval")
        {
            req = req.max_aggregation_interval(*interval as i32);
        }

        req = req.tag_specifications(build_tag_specification_with_create_token(
            resource,
            aws_sdk_ec2::types::ResourceType::VpcFlowLog,
            client_token,
        ));

        // Retry loop for IAM eventual consistency: newly created IAM roles may
        // not be usable immediately by create_flow_logs.
        let mut last_error = String::new();
        for retry_index in 0..12 {
            if retry_index > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let resp = match req.clone().send().await {
                Ok(resp) => resp,
                Err(error) => {
                    let err_str = format!("{error:?}");
                    last_error = err_str.clone();
                    // Retry on IAM propagation errors (check both Display and Debug output)
                    if err_str.contains("Unable to assume")
                        || err_str.contains("Not authorized")
                        || err_str.contains("Access Denied")
                    {
                        continue;
                    }
                    return Self::tagged_ec2_send_error(
                        attempt,
                        &resource.id,
                        Ec2TaggedResourceKind::FlowLog,
                        client_token,
                        carina_core::provider::Sent::AllRequestAttributes,
                        error,
                    );
                }
            };

            let mut flow_log_ids = resp.flow_log_ids().to_vec();
            flow_log_ids.sort();
            flow_log_ids.dedup();
            let unsuccessful = resp.unsuccessful().first();

            if !flow_log_ids.is_empty() {
                let identifier_text = flow_log_ids.join(",");
                let identifier = match crate::helpers::applied_identifier(
                    &resource.id,
                    "ec2.CreateFlowLogs",
                    identifier_text,
                ) {
                    Ok(identifier) => identifier,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                let sent = if unsuccessful.is_some() {
                    carina_core::provider::Sent::only(
                        resource
                            .attributes
                            .keys()
                            .filter(|name| name.as_str() != "resource_ids"),
                    )
                } else {
                    carina_core::provider::Sent::AllRequestAttributes
                };
                let issued = attempt.issued(identifier.clone(), sent);
                if let Some(error_item) = unsuccessful {
                    let message = error_item
                        .error()
                        .and_then(|error| error.message())
                        .map_or("unknown error", |message| message);
                    let error = ProviderError::api_error(format!(
                        "CreateFlowLogs created {} flow log(s), but another item failed: {message}",
                        flow_log_ids.len(),
                    ))
                    .for_resource(resource.id.clone());
                    return Err(self.failed_issued(&resource.id, issued, error).await);
                }

                return self
                    .finish_tagged_ec2_issued(
                        resource,
                        Ec2TaggedResourceKind::FlowLog,
                        identifier,
                        issued,
                    )
                    .await;
            }

            // With no returned ids, AWS proved that this response created no
            // trackable flow logs, so retrying an IAM propagation failure is safe.
            if let Some(error_item) = unsuccessful {
                let msg = error_item
                    .error()
                    .and_then(|error| error.message())
                    .map_or("unknown error", |message| message);
                let code = error_item
                    .error()
                    .and_then(|error| error.code())
                    .map_or("", |code| code);
                last_error = format!("{msg} ({code})");
                // Retry on IAM propagation errors
                if msg.contains("Unable to assume IAM role")
                    || msg.contains("Not authorized")
                    || msg.contains("Access Denied")
                    || code == "403"
                {
                    continue;
                }
                return Err(attempt.untracked(
                    ProviderError::api_error(format!("Failed to create flow log: {msg}"))
                        .for_resource(resource.id.clone()),
                ));
            }

            return Self::pending_tagged_ec2(
                attempt,
                Ec2TaggedResourceKind::FlowLog,
                client_token,
                carina_core::provider::Sent::AllRequestAttributes,
            );
        }

        Err(attempt.untracked(
            ProviderError::api_error(format!(
                "Failed to create flow log after retries: {last_error}"
            ))
            .for_resource(resource.id.clone()),
        ))
    }

    /// Update an EC2 Flow Log (tags only - all other attributes are create_only)
    pub(crate) async fn update_ec2_flow_log(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        for flow_log_id in identifier.as_str().split(',') {
            self.apply_ec2_tags(
                &id,
                flow_log_id,
                &to.resolved_attributes(),
                Some(&from.attributes),
            )
            .await?;
        }
        Ok(self.read_ec2_flow_log(&id, identifier).await)
    }

    /// Delete an EC2 Flow Log
    pub(crate) async fn delete_ec2_flow_log(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let flow_log_ids = identifier
            .as_str()
            .split(',')
            .map(str::to_string)
            .collect::<Vec<_>>();
        let result = self
            .ec2_client
            .delete_flow_logs()
            .set_flow_log_ids(Some(flow_log_ids))
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete flow logs", "ec2.DeleteFlowLogs", e)
                    .for_resource(id.clone())
            })?;

        // Check for unsuccessful items
        if let Some(err) = result.unsuccessful().first() {
            let msg = err
                .error()
                .and_then(|e| e.message())
                .map_or("unknown error", |message| message);
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
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
            );
        }
        if let Some(v) = obj.resource_id() {
            attributes.insert(
                "resource_id".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
            );
        }
        if let Some(v) = obj.traffic_type() {
            attributes.insert(
                "traffic_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
            );
        }
        if let Some(v) = obj.log_destination_type() {
            attributes.insert(
                "log_destination_type".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
            );
        }
        if let Some(v) = obj.log_destination() {
            attributes.insert(
                "log_destination".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
            );
        }
        if let Some(v) = obj.log_group_name() {
            attributes.insert(
                "log_group_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
            );
        }
        if let Some(v) = obj.deliver_logs_permission_arn() {
            attributes.insert(
                "deliver_logs_permission_arn".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
            );
        }
        if let Some(v) = obj.log_format() {
            attributes.insert(
                "log_format".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string().into())),
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
                        Value::Concrete(ConcreteValue::String(
                            resource_type_str.to_string().into(),
                        )),
                    );
                }
            }
        }
        obj.flow_log_id().map(String::from)
    }
}
