use carina_aws_types::SendOutcome;
use carina_core::provider::{AppliedIdentifier, ReadOutcome, Sent};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::{SendFailure, api_error_with_meta};
use crate::helpers::{
    build_tag_specification_with_create_token, enum_attr_error, send_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read, require_read_value};
use crate::read_models_generated::Ec2FlowLogReadModelSupplement;
use crate::send_models_generated::{Ec2FlowLogCreateInput, Ec2FlowLogUpdateInput};

/// Token state for one CreateFlowLogs request.
///
/// Rotation consumes the old value and is only available with typed evidence
/// that an AWS response rejected every item without creating a flow log.
#[derive(Debug)]
struct FlowLogRequestToken(ClientToken);

impl FlowLogRequestToken {
    fn from_invocation(token: &ClientToken) -> Self {
        Self(token.clone())
    }

    fn as_client_token(&self) -> &ClientToken {
        &self.0
    }

    fn rotate_after(self, _rejection: &FlowLogCreateRejection) -> Self {
        Self(ClientToken::new())
    }

    fn preserve_after(self, _maybe_sent: FlowLogMaybeSent) -> ClientToken {
        self.0
    }
}

#[derive(Debug)]
struct FlowLogCreateRejection {
    message: String,
    code: String,
}

impl FlowLogCreateRejection {
    fn from_output(
        output: &aws_sdk_ec2::operation::create_flow_logs::CreateFlowLogsOutput,
    ) -> Option<Self> {
        if !output.flow_log_ids().is_empty() {
            return None;
        }
        let item = output.unsuccessful().first()?;
        let message = item
            .error()
            .and_then(|error| error.message())
            .unwrap_or("unknown error")
            .to_string();
        let code = item
            .error()
            .and_then(|error| error.code())
            .unwrap_or("")
            .to_string();
        Some(Self { message, code })
    }

    fn from_failure(
        failure: &SendFailure<aws_sdk_ec2::operation::create_flow_logs::CreateFlowLogsError>,
    ) -> Option<Self> {
        use aws_smithy_types::error::metadata::ProvideErrorMetadata;

        if failure.outcome() != SendOutcome::Rejected {
            return None;
        }
        let service_error = failure.as_service_error()?;
        Some(Self {
            message: service_error
                .message()
                .unwrap_or("unknown error")
                .to_string(),
            code: service_error.code().unwrap_or("").to_string(),
        })
    }

    fn is_iam_propagation(&self) -> bool {
        self.message.contains("Unable to assume IAM role")
            || self.message.contains("Not authorized")
            || self.message.contains("Access Denied")
            || self.code == "403"
    }

    fn rendered(&self) -> String {
        format!("{} ({})", self.message, self.code)
    }
}

#[derive(Debug)]
struct FlowLogMaybeSent;

impl FlowLogMaybeSent {
    fn from_failure<E>(failure: &SendFailure<E>) -> Option<Self> {
        (failure.outcome() == SendOutcome::MaybeSent).then_some(Self)
    }
}

fn build_tag_field_specifications(
    id: &ResourceId,
    value: Option<&Value>,
    schema: &ResourceSchema,
) -> ProviderResult<Option<Vec<aws_sdk_ec2::types::TagFieldSpecificationRequest>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Concrete(ConcreteValue::List(items)) = value else {
        return Err(
            ProviderError::invalid_input("tag_field_specifications must be a list")
                .for_resource(id.clone()),
        );
    };

    items
        .iter()
        .map(|item| {
            let Value::Concrete(ConcreteValue::Map(fields)) = item else {
                return Err(ProviderError::invalid_input(
                    "each tag_field_specifications item must be a map",
                )
                .for_resource(id.clone()));
            };
            let resource_type = carina_core::resource::enum_attr_at_schema_path(
                schema,
                "tag_field_specifications",
                &["resource_type"],
                fields.get("resource_type"),
            )
            .map_err(|error| enum_attr_error(id, error))?
            .map(|value| aws_sdk_ec2::types::TaggableResourceType::from(value.api_value()));
            let tag_keys = match fields.get("tag_keys") {
                Some(Value::Concrete(ConcreteValue::StringList(values))) => Some(
                    values
                        .iter()
                        .map(|value| value.expose_plaintext().to_string())
                        .collect(),
                ),
                Some(Value::Concrete(ConcreteValue::List(values))) => Some(
                    values
                        .iter()
                        .map(|value| match value {
                            Value::Concrete(ConcreteValue::String(value)) => {
                                Ok(value.expose_plaintext().to_string())
                            }
                            _ => Err(ProviderError::invalid_input(
                                "tag_field_specifications.tag_keys must contain strings",
                            )
                            .for_resource(id.clone())),
                        })
                        .collect::<ProviderResult<Vec<_>>>()?,
                ),
                None => None,
                Some(_) => {
                    return Err(ProviderError::invalid_input(
                        "tag_field_specifications.tag_keys must be a list of strings",
                    )
                    .for_resource(id.clone()));
                }
            };

            Ok(aws_sdk_ec2::types::TagFieldSpecificationRequest::builder()
                .set_resource_type(resource_type)
                .set_tag_keys(tag_keys)
                .build())
        })
        .collect::<ProviderResult<Vec<_>>>()
        .map(Some)
}

fn tag_field_specifications_value(
    specifications: &[aws_sdk_ec2::types::TagFieldSpecificationResponse],
) -> Option<Value> {
    (!specifications.is_empty()).then(|| {
        Value::Concrete(ConcreteValue::List(
            specifications
                .iter()
                .map(|specification| {
                    let mut fields = indexmap::IndexMap::new();
                    if let Some(resource_type) = specification.resource_type() {
                        fields.insert(
                            "resource_type".to_string(),
                            Value::Concrete(ConcreteValue::String(
                                resource_type.as_str().to_string().into(),
                            )),
                        );
                    }
                    if !specification.tag_keys().is_empty() {
                        fields.insert(
                            "tag_keys".to_string(),
                            Value::Concrete(ConcreteValue::List(
                                specification
                                    .tag_keys()
                                    .iter()
                                    .map(|tag_key| {
                                        Value::Concrete(ConcreteValue::String(
                                            tag_key.to_string().into(),
                                        ))
                                    })
                                    .collect(),
                            )),
                        );
                    }
                    Value::Concrete(ConcreteValue::Map(fields))
                })
                .collect(),
        ))
    })
}

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
                api_error_with_meta("Failed to describe flow logs", e).for_resource(id.clone())
            })?;

        if let Some(fl) = result.flow_logs().first() {
            let fields = Self::extract_ec2_flow_log_attributes(fl);

            // A single CreateFlowLogs request can create one flow log per
            // requested resource. Preserve the complete set under one provider
            // identifier and reconstruct the schema's plural resource_ids.
            let resource_ids = result
                .flow_logs()
                .iter()
                .filter_map(|flow_log| flow_log.resource_id())
                .map(|resource_id| {
                    Value::Concrete(ConcreteValue::String(resource_id.to_string().into()))
                })
                .collect::<Vec<_>>();
            let resource_ids = Value::Concrete(ConcreteValue::List(resource_ids));
            let resource_type = fl.resource_id().and_then(|resource_id| {
                let resource_type = if resource_id.starts_with("tgw-attach-") {
                    "TransitGatewayAttachment"
                } else if resource_id.starts_with("tgw-") {
                    "TransitGateway"
                } else if resource_id.starts_with("nat-") {
                    "RegionalNatGateway"
                } else if resource_id.starts_with("vpc-") {
                    "VPC"
                } else if resource_id.starts_with("subnet-") {
                    "Subnet"
                } else if resource_id.starts_with("eni-") {
                    "NetworkInterface"
                } else {
                    return None;
                };
                Some(Value::Concrete(ConcreteValue::String(
                    resource_type.to_string().into(),
                )))
            });

            let (_, model) = fields.into_model(
                id,
                Ec2FlowLogReadModelSupplement {
                    resource_ids,
                    resource_type: require_read_value(id, resource_type, "resource_type")?,
                    tag_field_specifications: tag_field_specifications_value(
                        fl.tag_field_specifications(),
                    ),
                    tags: Self::ec2_tags_to_value(fl.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Flow Log
    pub(crate) async fn create_ec2_flow_log<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2FlowLogCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let crate::send_models_generated::Ec2FlowLogCreateParts {
            deliver_logs_permission_arn,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            resource_ids,
            resource_type,
            tag_field_specifications,
            tags,
            traffic_type,
        } = input.into_parts();
        let deliver_logs_permission_arn = deliver_logs_permission_arn.into_inner();
        let log_destination = log_destination.into_inner();
        let log_destination_type = log_destination_type.into_inner();
        let log_format = log_format.into_inner();
        let log_group_name = log_group_name.into_inner();
        let max_aggregation_interval = max_aggregation_interval.into_inner();
        let resource_ids = resource_ids.into_inner();
        let resource_type = resource_type.into_inner();
        let tag_field_specifications = tag_field_specifications.into_inner();
        let tag_field_specifications = match build_tag_field_specifications(
            &resource.id,
            tag_field_specifications.as_ref(),
            schema,
        ) {
            Ok(specifications) => specifications,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let traffic_type = traffic_type.into_inner();
        if resource_ids.is_empty() {
            return Err(attempt.untracked(
                ProviderError::invalid_input(
                    "resource_ids must be a non-empty list of resource identifiers",
                )
                .for_resource(resource.id.clone()),
            ));
        }

        let mut req = self.ec2_client.create_flow_logs().configure(|builder| {
            builder
                .set_resource_ids(Some(resource_ids.clone()))
                .resource_type(aws_sdk_ec2::types::FlowLogsResourceType::from(
                    resource_type.as_str(),
                ))
                .set_tag_field_specifications(tag_field_specifications.clone())
        });

        if let Some(traffic_type) = traffic_type {
            use aws_sdk_ec2::types::TrafficType;
            req = req.configure(|builder| {
                builder.traffic_type(TrafficType::from(traffic_type.as_str()))
            });
        }

        if let Some(log_dest_type) = log_destination_type {
            use aws_sdk_ec2::types::LogDestinationType;
            req = req.configure(|builder| {
                builder.log_destination_type(LogDestinationType::from(log_dest_type.as_str()))
            });
        }

        if let Some(log_destination) = log_destination {
            req = req.configure(|builder| builder.log_destination(log_destination));
        }

        if let Some(log_group_name) = log_group_name {
            req = req.configure(|builder| builder.log_group_name(log_group_name));
        }

        if let Some(deliver_logs_permission_arn) = deliver_logs_permission_arn {
            req = req.configure(|builder| {
                builder.deliver_logs_permission_arn(deliver_logs_permission_arn)
            });
        }

        if let Some(log_format) = log_format {
            req = req.configure(|builder| builder.log_format(log_format));
        }

        if let Some(interval) = max_aggregation_interval {
            req = req.configure(|builder| builder.max_aggregation_interval(interval as i32));
        }

        // Retry loop for IAM eventual consistency: newly created IAM roles may
        // not be usable immediately by create_flow_logs.
        let mut last_error = String::new();
        let mut request_token = FlowLogRequestToken::from_invocation(client_token);
        for retry_index in 0..12 {
            if retry_index > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let request = req.clone().configure(|builder| {
                builder.tag_specifications(build_tag_specification_with_create_token(
                    tags.as_ref(),
                    aws_sdk_ec2::types::ResourceType::VpcFlowLog,
                    request_token.as_client_token(),
                ))
            });
            let resp = match send_aws_operation_with_token(request_token.as_client_token(), request)
                .await
            {
                Ok(resp) => resp,
                Err(error) => {
                    if let Some(maybe_sent) = FlowLogMaybeSent::from_failure(&error) {
                        let discovery_token = request_token.preserve_after(maybe_sent);
                        return Self::tagged_ec2_send_error(
                            attempt,
                            &resource.id,
                            Ec2TaggedResourceKind::FlowLog,
                            &discovery_token,
                            sent.clone(),
                            error,
                        );
                    }
                    if let Some(rejection) = FlowLogCreateRejection::from_failure(&error)
                        && rejection.is_iam_propagation()
                    {
                        last_error = rejection.rendered();
                        request_token = request_token.rotate_after(&rejection);
                        continue;
                    }
                    return Self::tagged_ec2_send_error(
                        attempt,
                        &resource.id,
                        Ec2TaggedResourceKind::FlowLog,
                        request_token.as_client_token(),
                        sent.clone(),
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
                let issued_sent = if unsuccessful.is_some() {
                    match &sent {
                        Sent::Only(names) => {
                            let mut names = names.clone();
                            names.remove("resource_ids");
                            Sent::Only(names)
                        }
                        Sent::AllRequestAttributes => Sent::AllRequestAttributes,
                    }
                } else {
                    sent.clone()
                };
                let issued = attempt.issued(identifier.clone(), issued_sent);
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
                        TaggedEc2Create::FlowLog,
                        identifier,
                        issued,
                    )
                    .await;
            }

            if let Some(rejection) = FlowLogCreateRejection::from_output(&resp) {
                last_error = rejection.rendered();
                if rejection.is_iam_propagation() {
                    request_token = request_token.rotate_after(&rejection);
                    continue;
                }
                return Err(attempt.untracked(
                    ProviderError::api_error(format!(
                        "Failed to create flow log: {}",
                        rejection.message
                    ))
                    .for_resource(resource.id.clone()),
                ));
            }

            return Self::pending_tagged_ec2(
                attempt,
                Ec2TaggedResourceKind::FlowLog,
                request_token.as_client_token(),
                sent,
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
        let input = Ec2FlowLogUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2FlowLogUpdateParts { tags } = input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        for flow_log_id in identifier.as_str().split(',') {
            self.apply_ec2_tags(&id, flow_log_id, tags.as_ref(), Some(&from.attributes))
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
                api_error_with_meta("Failed to delete flow logs", e).for_resource(id.clone())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ec2::error::SdkError;
    use aws_sdk_ec2::operation::create_flow_logs::CreateFlowLogsError;
    use aws_sdk_ec2::types::{UnsuccessfulItem, UnsuccessfulItemError};
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    #[test]
    fn proven_rejection_rotates_the_flow_log_client_token() {
        let invocation_token = ClientToken::new();
        let attempt_token = FlowLogRequestToken::from_invocation(&invocation_token);
        let output = aws_sdk_ec2::operation::create_flow_logs::CreateFlowLogsOutput::builder()
            .unsuccessful(
                UnsuccessfulItem::builder()
                    .error(
                        UnsuccessfulItemError::builder()
                            .code("403")
                            .message("Unable to assume IAM role")
                            .build(),
                    )
                    .build(),
            )
            .build();
        let rejection = FlowLogCreateRejection::from_output(&output)
            .expect("an empty unsuccessful response proves rejection");

        let next = attempt_token.rotate_after(&rejection);

        assert_ne!(next.as_client_token(), &invocation_token);
    }

    #[test]
    fn rejected_iam_service_failure_rotates_the_flow_log_client_token() {
        let invocation_token = ClientToken::new();
        let attempt_token = FlowLogRequestToken::from_invocation(&invocation_token);
        let service_error = CreateFlowLogsError::generic(
            ErrorMetadata::builder()
                .code("UnauthorizedOperation")
                .message("Unable to assume IAM role")
                .build(),
        );
        let response = HttpResponse::new(400.try_into().expect("valid status"), SdkBody::empty());
        let failure = crate::error_helpers::SendFailure::new(
            SendOutcome::Rejected,
            SdkError::service_error(service_error, response),
        );
        let rejection = FlowLogCreateRejection::from_failure(&failure)
            .expect("a rejected IAM propagation error is safe to retry");

        let next = attempt_token.rotate_after(&rejection);

        assert!(rejection.is_iam_propagation());
        assert_ne!(next.as_client_token(), &invocation_token);
    }

    #[test]
    fn sticky_maybe_sent_flow_log_failure_preserves_the_client_token() {
        let invocation_token = ClientToken::new();
        let attempt_token = FlowLogRequestToken::from_invocation(&invocation_token);
        let service_error = CreateFlowLogsError::generic(
            ErrorMetadata::builder()
                .code("RequestLimitExceeded")
                .message("final attempt throttled")
                .build(),
        );
        let response = HttpResponse::new(429.try_into().expect("valid status"), SdkBody::empty());
        let failure = crate::error_helpers::SendFailure::new(
            SendOutcome::MaybeSent,
            SdkError::service_error(service_error, response),
        );
        let maybe_sent = FlowLogMaybeSent::from_failure(&failure)
            .expect("the sticky outcome must override the final throttling rejection");

        let discovery_token = attempt_token.preserve_after(maybe_sent);

        assert_eq!(discovery_token, invocation_token);
    }
}
