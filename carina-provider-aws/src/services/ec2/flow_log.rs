use aws_types::request_id::RequestId;
use carina_aws_types::SendClassification;
use carina_aws_types::sdk::ec2::operation::{
    create_flow_logs::CreateFlowLogsError, delete_flow_logs::DeleteFlowLogsError,
};
use carina_core::provider::{AppliedIdentifier, ReadOutcome, Sent};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::provider_text::ProviderText;
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, SnapshotState,
    Value,
};
use carina_core::schema::CompiledResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::{
    AwsItemFailures, SendFailure, api_error_with_item_failure, api_error_with_meta,
    api_error_with_partial_item_failure,
};
use crate::helpers::{
    build_tag_specification_with_create_token, enum_attr_error, send_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read, require_read_value};
use crate::read_models_generated::Ec2FlowLogReadModelSupplement;
use crate::send_models_generated::{Ec2FlowLogCreateInput, Ec2FlowLogUpdateInput};

const FLOW_LOG_CREATE_ATTEMPTS: usize = 12;

/// Token state for one CreateFlowLogs request.
///
/// Rotation consumes the old value and requires an approved retry decision
/// for a rejection that created no flow log.
#[derive(Debug)]
struct FlowLogRequestToken(ClientToken);

impl FlowLogRequestToken {
    fn from_invocation(token: &ClientToken) -> Self {
        Self(token.clone())
    }

    fn as_client_token(&self) -> &ClientToken {
        &self.0
    }

    fn rotate_after(self, _retryable: &RetryableRejection) -> Self {
        Self(ClientToken::new())
    }

    fn preserve_after(self, _maybe_sent: FlowLogMaybeSent) -> ClientToken {
        self.0
    }
}

mod flow_log_rejection {
    use super::{
        AwsItemFailures, CreateFlowLogsError, Ec2TaggedResourceKind, FLOW_LOG_CREATE_ATTEMPTS,
        ProviderError, SendClassification, SendFailure, api_error_with_item_failure,
        api_error_with_meta,
    };
    use aws_types::request_id::RequestId;

    #[derive(Debug)]
    pub(super) struct RejectedIamRequest {
        failure: SendFailure<CreateFlowLogsError>,
    }

    impl RejectedIamRequest {
        fn try_from_failure(
            failure: SendFailure<CreateFlowLogsError>,
        ) -> Result<Self, SendFailure<CreateFlowLogsError>> {
            use aws_smithy_types::error::metadata::ProvideErrorMetadata;

            if failure.classification() == SendClassification::Rejected
                && FlowLogCreateRejection::is_iam_propagation_failure(
                    failure.code(),
                    failure.message(),
                )
            {
                Ok(Self { failure })
            } else {
                Err(failure)
            }
        }
    }

    #[derive(Debug)]
    pub(super) struct RetryableRejection {
        _rejection: FlowLogCreateRejection,
    }

    #[derive(Debug)]
    pub(super) enum FlowLogCreateRejection {
        Items {
            failures: AwsItemFailures,
            request_id: Option<String>,
        },
        IamPropagationRequest(RejectedIamRequest),
    }

    impl FlowLogCreateRejection {
        pub(super) fn from_output(
            output: &carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsOutput,
        ) -> Option<Self> {
            if !output.flow_log_ids().is_empty() {
                return None;
            }
            let failures = AwsItemFailures::from_items(output.unsuccessful())?;
            Some(Self::Items {
                failures,
                request_id: output.request_id().map(str::to_owned),
            })
        }

        pub(super) fn from_failure(
            failure: SendFailure<CreateFlowLogsError>,
        ) -> Result<Self, SendFailure<CreateFlowLogsError>> {
            RejectedIamRequest::try_from_failure(failure).map(Self::IamPropagationRequest)
        }

        pub(super) fn is_iam_propagation(&self) -> bool {
            match self {
                Self::Items { failures, .. } => failures.iter().all(|failure| {
                    Self::is_iam_propagation_failure(failure.code(), failure.message())
                }),
                Self::IamPropagationRequest(_) => true,
            }
        }

        fn is_iam_propagation_failure(code: Option<&str>, message: Option<&str>) -> bool {
            message.is_some_and(|message| {
                message.contains("Unable to assume IAM role")
                    || message.contains("Not authorized")
                    || message.contains("Access Denied")
            }) || code == Some("403")
        }

        pub(super) fn retry_or_error(
            self,
            retry_index: usize,
        ) -> Result<RetryableRejection, ProviderError> {
            let is_iam_propagation = self.is_iam_propagation();
            if is_iam_propagation && retry_index + 1 < FLOW_LOG_CREATE_ATTEMPTS {
                Ok(RetryableRejection { _rejection: self })
            } else {
                Err(match self {
                    Self::Items {
                        failures,
                        request_id,
                    } => {
                        let context = if is_iam_propagation {
                            "Failed to create flow logs after retries"
                        } else {
                            Ec2TaggedResourceKind::FlowLog.create_error_context()
                        };
                        api_error_with_item_failure::<CreateFlowLogsError>(
                            context,
                            failures,
                            request_id.as_deref(),
                        )
                    }
                    Self::IamPropagationRequest(request) => api_error_with_meta(
                        "Failed to create flow logs after retries",
                        request.failure,
                    ),
                })
            }
        }
    }
}

use flow_log_rejection::{FlowLogCreateRejection, RetryableRejection};

#[derive(Debug)]
struct FlowLogMaybeSent;

impl FlowLogMaybeSent {
    fn from_failure<E>(failure: &SendFailure<E>) -> Option<Self> {
        match failure.classification() {
            SendClassification::MaybeSent { .. } => Some(Self),
            SendClassification::NotSent(_) | SendClassification::Rejected => None,
        }
    }
}

fn build_tag_field_specifications(
    id: &ResourceId,
    value: Option<&Value>,
    schema: &CompiledResourceSchema,
) -> ProviderResult<Option<Vec<carina_aws_types::sdk::ec2::types::TagFieldSpecificationRequest>>> {
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
            .map(|value| {
                carina_aws_types::sdk::ec2::types::TaggableResourceType::from(value.api_value())
            });
            let tag_keys = crate::send_model::optional_string_list_value(
                id,
                "tag_field_specifications.tag_keys",
                fields.get("tag_keys"),
            )?;

            Ok(
                carina_aws_types::sdk::ec2::types::TagFieldSpecificationRequest::builder()
                    .set_resource_type(resource_type)
                    .set_tag_keys(tag_keys)
                    .build(),
            )
        })
        .collect::<ProviderResult<Vec<_>>>()
        .map(Some)
}

fn tag_field_specifications_value(
    specifications: &[carina_aws_types::sdk::ec2::types::TagFieldSpecificationResponse],
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
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
        schema: &CompiledResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2FlowLogCreateInput::decode(resource, schema) {
            Ok(input) => input,
            Err(error) => return Err(attempt.not_created(error)),
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
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let traffic_type = traffic_type.into_inner();
        let tag_field_specifications = match build_tag_field_specifications(
            &resource.id,
            tag_field_specifications.as_ref(),
            schema,
        ) {
            Ok(specifications) => specifications,
            Err(error) => return Err(attempt.not_created(error)),
        };
        if resource_ids.is_empty() {
            return Err(attempt.not_created(
                ProviderError::invalid_input(
                    "resource_ids must be a non-empty list of resource identifiers",
                )
                .for_resource(resource.id.clone()),
            ));
        }

        let mut req = self.ec2_client.create_flow_logs().configure(|builder| {
            builder
                .set_resource_ids(Some(resource_ids.clone()))
                .resource_type(
                    carina_aws_types::sdk::ec2::types::FlowLogsResourceType::from(
                        resource_type.as_str(),
                    ),
                )
                .set_tag_field_specifications(tag_field_specifications.clone())
        });

        if let Some(traffic_type) = traffic_type {
            use carina_aws_types::sdk::ec2::types::TrafficType;
            req = req.configure(|builder| {
                builder.traffic_type(TrafficType::from(traffic_type.as_str()))
            });
        }

        if let Some(log_dest_type) = log_destination_type {
            use carina_aws_types::sdk::ec2::types::LogDestinationType;
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
            req = req.configure(|builder| builder.max_aggregation_interval(interval));
        }

        // Retry loop for IAM eventual consistency: newly created IAM roles may
        // not be usable immediately by create_flow_logs.
        let mut request_token = FlowLogRequestToken::from_invocation(client_token);
        let mut retry_index = 0;
        loop {
            if retry_index > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let request = req.clone().configure(|builder| {
                builder.tag_specifications(build_tag_specification_with_create_token(
                    tags.as_ref(),
                    carina_aws_types::sdk::ec2::types::ResourceType::VpcFlowLog,
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
                    let error = match FlowLogCreateRejection::from_failure(error) {
                        Ok(rejection) => {
                            let retryable = match rejection.retry_or_error(retry_index) {
                                Ok(rejection) => rejection,
                                Err(error) => {
                                    return Err(attempt
                                        .not_created(error.for_resource(resource.id.clone())));
                                }
                            };
                            request_token = request_token.rotate_after(&retryable);
                            retry_index += 1;
                            continue;
                        }
                        Err(error) => error,
                    };
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
            if !flow_log_ids.is_empty() {
                let unsuccessful = AwsItemFailures::from_items(resp.unsuccessful());
                let identifier_text = flow_log_ids.join(",");
                let identifier = match crate::helpers::applied_identifier(
                    &resource.id,
                    "ec2.CreateFlowLogs",
                    identifier_text,
                ) {
                    Ok(identifier) => identifier,
                    Err(error) => return Err(attempt.not_created(error)),
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
                if let Some(failures) = unsuccessful {
                    let error = api_error_with_partial_item_failure::<CreateFlowLogsError>(
                        ProviderText::new(format!(
                            "CreateFlowLogs created {} flow log(s), but another item failed",
                            flow_log_ids.len(),
                        )),
                        failures,
                        resp.request_id(),
                    )
                    .for_resource(resource.id.clone());
                    return Err(self.failed_issued(&resource.id, issued, error).await);
                }

                return self
                    .finish_tagged_ec2_issued(
                        resource,
                        TaggedEc2Create::FlowLog,
                        identifier,
                        issued,
                        carina_provider_protocol::PollRequest {
                            finality: carina_provider_protocol::PollFinality::Continue,
                        },
                    )
                    .await;
            }

            if let Some(rejection) = FlowLogCreateRejection::from_output(&resp) {
                let retryable = match rejection.retry_or_error(retry_index) {
                    Ok(rejection) => rejection,
                    Err(error) => {
                        return Err(attempt.not_created(error.for_resource(resource.id.clone())));
                    }
                };
                request_token = request_token.rotate_after(&retryable);
                retry_index += 1;
                continue;
            }

            return Self::pending_tagged_ec2(
                attempt,
                Ec2TaggedResourceKind::FlowLog,
                request_token.as_client_token(),
                sent,
            );
        }
    }

    /// Update an EC2 Flow Log (tags only - all other attributes are create_only)
    pub(crate) async fn update_ec2_flow_log(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &SnapshotState,
        to: &crate::helpers::UpdateResource,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2FlowLogUpdateInput::decode(to)?;
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
        if let Some(failures) = AwsItemFailures::from_items(result.unsuccessful()) {
            return Err(api_error_with_item_failure::<DeleteFlowLogsError>(
                "Failed to delete flow log",
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
    use crate::error_helpers::unsuccessful_item;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;
    use carina_aws_types::sdk::ec2::error::SdkError;
    use carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsError;
    use carina_aws_types::sdk::ec2::types::UnsuccessfulItem;

    #[cfg(not(target_arch = "wasm32"))]
    mod item_failures {
        use super::*;
        use crate::error_helpers::AwsItemFailures;
        use crate::send_contract_test::CaptureAttempt as Attempt;
        use crate::test_http::provider;

        const AWS_MESSAGE: &str = "private AWS per-item diagnostic";
        const AWS_CODE: &str = "InvalidParameterValue";

        fn response(action: &str, created: bool, items: &[UnsuccessfulItem]) -> String {
            let errors = items
                .iter()
                .map(|item| {
                    let error = item.error();
                    let code = error
                        .and_then(|error| error.code())
                        .map(|code| format!("<code>{code}</code>"))
                        .unwrap_or_default();
                    let message = error
                        .and_then(|error| error.message())
                        .map(|message| format!("<message>{message}</message>"))
                        .unwrap_or_default();
                    let resource_id = item
                        .resource_id()
                        .map(|id| format!("<resourceId>{id}</resourceId>"))
                        .unwrap_or_default();
                    format!("<item>{resource_id}<error>{code}{message}</error></item>")
                })
                .collect::<String>();
            let ids = if created {
                "<flowLogIdSet><item>fl-0123456789abcdef0</item></flowLogIdSet>"
            } else {
                ""
            };
            format!(
                r#"<{action}Response xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">{ids}<unsuccessful>{errors}</unsuccessful></{action}Response>"#
            )
        }

        fn resource() -> Resource {
            let schemas = crate::schemas::all_schemas();
            let schema = schemas
                .iter()
                .find(|schema| schema.as_schema().resource_type() == "ec2.FlowLog")
                .unwrap();
            let string =
                |value: &str| Value::Concrete(ConcreteValue::String(value.to_string().into()));
            let mut resource = Resource::with_provider("aws", "ec2.FlowLog", "test", None)
                .with_attribute(
                    "resource_ids",
                    Value::Concrete(ConcreteValue::List(vec![string("vpc-0123456789abcdef0")])),
                );
            let value = schema
                .effective_attribute("resource_type")
                .expect("resource_type schema")
                .canonicalize_state(string("VPC"))
                .into_present()
                .expect("VPC resource type");
            resource.set_attr("resource_type", value);
            resource
        }

        fn assert_failure(
            error: ProviderError,
            context: &str,
            operation: &str,
            code: Option<&str>,
            matches: fn(&AwsItemFailures) -> bool,
        ) {
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            assert_eq!(error.detail().context(), context);
            let wire = error.to_wire();
            assert_eq!(wire.operation.as_deref(), Some(operation));
            assert_eq!(wire.request_id.as_deref(), Some("test-request"));
            assert_eq!(
                error.detail().code().as_ref().map(|code| code.as_str()),
                code
            );
            assert!(
                error
                    .detail()
                    .cause()
                    .expect("typed item failure")
                    .as_cause()
                    .matches_for_test(matches)
            );
        }

        #[tokio::test]
        async fn delete_flow_logs_keeps_typed_item_failure() {
            let (provider, responses) = provider([(
                200,
                response(
                    "DeleteFlowLogs",
                    false,
                    &[unsuccessful_item(None, Some(AWS_CODE), Some(AWS_MESSAGE))],
                ),
            )]);
            let error = provider
                .delete_ec2_flow_log(
                    resource().id,
                    &AppliedIdentifier::new("fl-0123456789abcdef0").unwrap(),
                )
                .await
                .unwrap_err();
            assert_failure(
                error,
                "Failed to delete flow log",
                "ec2.DeleteFlowLogs",
                Some(AWS_CODE),
                |failures| {
                    let failure = failures.first();
                    failures.iter().count() == 1
                        && failure.code() == Some(AWS_CODE)
                        && failure.message() == Some(AWS_MESSAGE)
                },
            );
            responses.assert_finished();
        }

        #[tokio::test]
        async fn create_flow_logs_partial_success_keeps_typed_item_failure() {
            const CODE: &str = "InvalidVpcID.NotFound";
            let (provider, responses) = provider([
                (200, response("CreateFlowLogs", true, &[unsuccessful_item(None, Some(CODE), Some(AWS_MESSAGE))])),
                (200, r#"<DescribeFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><flowLogSet/></DescribeFlowLogsResponse>"#.into()),
            ]);
            let error = provider
                .create_ec2_flow_log(
                    &resource(),
                    crate::test_resource_schema("ec2.FlowLog"),
                    &ClientToken::new(),
                    Attempt,
                )
                .await
                .unwrap_err();
            assert_eq!(
                error.kind(),
                carina_core::provider::ProviderErrorKind::ApiError
            );
            assert!(!error.is_transient());
            let host = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(
                error.clone(),
            );
            let context = host.detail().context();
            assert_eq!(
                context,
                "CreateFlowLogs created 1 flow log(s), but another item failed"
            );
            assert!(!context.as_str().contains(AWS_MESSAGE));
            let message = host.to_wire().message;
            assert!(message.contains("created 1 flow log(s)"), "{message}");
            assert!(!message.contains(AWS_MESSAGE), "{message}");
            assert_failure(
                error,
                "CreateFlowLogs created 1 flow log(s), but another item failed",
                "ec2.CreateFlowLogs",
                Some(CODE),
                |failures| {
                    let failure = failures.first();
                    failures.iter().count() == 1
                        && failure.code() == Some(CODE)
                        && failure.message() == Some(AWS_MESSAGE)
                },
            );
            responses.assert_finished();
        }

        #[tokio::test(start_paused = true)]
        async fn create_flow_logs_exhausted_retries_keep_last_typed_item_failure() {
            let rejections = (0..FLOW_LOG_CREATE_ATTEMPTS).map(|index| {
                (
                    200,
                    response(
                        "CreateFlowLogs",
                        false,
                        &[unsuccessful_item(
                            None,
                            Some("403"),
                            Some(if index + 1 == FLOW_LOG_CREATE_ATTEMPTS {
                                "Access Denied final rejection"
                            } else {
                                "Unable to assume IAM role"
                            }),
                        )],
                    ),
                )
            });
            let (provider, responses) = provider(rejections);
            let error = provider
                .create_ec2_flow_log(
                    &resource(),
                    crate::test_resource_schema("ec2.FlowLog"),
                    &ClientToken::new(),
                    Attempt,
                )
                .await
                .unwrap_err();
            assert_failure(
                error,
                "Failed to create flow logs after retries",
                "ec2.CreateFlowLogs",
                Some("403"),
                |failures| {
                    let failure = failures.first();
                    failures.iter().count() == 1
                        && failure.code() == Some("403")
                        && failure.message() == Some("Access Denied final rejection")
                },
            );
            assert_eq!(responses.requests().len(), FLOW_LOG_CREATE_ATTEMPTS);
            responses.assert_finished();
        }

        #[tokio::test(start_paused = true)]
        async fn create_flow_logs_exhausted_service_retries_keep_last_sdk_failure() {
            let rejections = (0..FLOW_LOG_CREATE_ATTEMPTS).map(|index| (400, format!(r#"<Response><Errors><Error><Code>UnauthorizedOperation</Code><Message>{}</Message></Error></Errors><RequestID>test-request</RequestID></Response>"#, if index + 1 == FLOW_LOG_CREATE_ATTEMPTS { "Not authorized final service rejection" } else { "Unable to assume IAM role" })));
            let (provider, responses) = provider(rejections);
            let error = provider
                .create_ec2_flow_log(
                    &resource(),
                    crate::test_resource_schema("ec2.FlowLog"),
                    &ClientToken::new(),
                    Attempt,
                )
                .await
                .unwrap_err();
            use carina_core::provider_text::ProviderDiagnostic;

            let diagnostic = ProviderDiagnostic::new(error.clone());
            assert!(diagnostic.source_is::<SdkError<CreateFlowLogsError>>());
            assert!(diagnostic.as_cause().matches_for_test(
                |sdk: &SdkError<CreateFlowLogsError>| {
                    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
                    sdk.message() == Some("Not authorized final service rejection")
                }
            ));
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            assert_eq!(
                error.detail().context(),
                "Failed to create flow logs after retries"
            );
            assert_eq!(error.detail().status, Some(400));
            assert_eq!(
                error.detail().code().as_ref().map(|code| code.as_str()),
                Some("UnauthorizedOperation")
            );
            let wire = error.to_wire();
            assert_eq!(wire.operation.as_deref(), Some("ec2.CreateFlowLogs"));
            assert_eq!(wire.request_id.as_deref(), Some("test-request"));
            assert_eq!(responses.requests().len(), FLOW_LOG_CREATE_ATTEMPTS);
            responses.assert_finished();
        }

        #[tokio::test]
        async fn create_flow_logs_request_and_item_failures_share_context() {
            use carina_core::provider_text::ProviderDiagnostic;

            for (status, body) in [
                (
                    200,
                    response(
                        "CreateFlowLogs",
                        false,
                        &[unsuccessful_item(None, Some(AWS_CODE), Some(AWS_MESSAGE))],
                    ),
                ),
                (
                    400,
                    format!(
                        r#"<Response><Errors><Error><Code>{AWS_CODE}</Code><Message>{AWS_MESSAGE}</Message></Error></Errors><RequestID>test-request</RequestID></Response>"#
                    ),
                ),
            ] {
                let (provider, responses) = provider([(status, body)]);
                let error = provider
                    .create_ec2_flow_log(
                        &resource(),
                        crate::test_resource_schema("ec2.FlowLog"),
                        &ClientToken::new(),
                        Attempt,
                    )
                    .await
                    .unwrap_err();
                let diagnostic = ProviderDiagnostic::new(error.clone());
                if status == 200 {
                    assert!(diagnostic.as_cause().matches_for_test::<AwsItemFailures>(
                        |failures| {
                            failures.iter().count() == 1
                                && failures.first().code() == Some(AWS_CODE)
                                && failures.first().message() == Some(AWS_MESSAGE)
                        }
                    ));
                } else {
                    assert!(diagnostic.source_is::<SdkError<CreateFlowLogsError>>());
                }
                let error =
                    carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(
                        error,
                    );
                assert_eq!(error.detail().context(), "Failed to create flow logs");
                let wire = error.to_wire();
                assert_eq!(wire.operation.as_deref(), Some("ec2.CreateFlowLogs"));
                assert_eq!(wire.request_id.as_deref(), Some("test-request"));
                assert_eq!(responses.requests().len(), 1);
                responses.assert_finished();
            }
        }
    }

    #[test]
    fn iam_propagation_retry_requires_every_item() {
        let permanent =
            unsuccessful_item(None, Some("InvalidParameterValue"), Some("permanent error"));
        let iam = unsuccessful_item(None, Some("403"), Some("Unable to assume IAM role"));
        for (items, expected_retry) in [
            ([permanent.clone(), iam.clone()], false),
            ([iam.clone(), permanent], false),
            (
                [
                    iam,
                    unsuccessful_item(None, None, Some("Unable to assume IAM role")),
                ],
                true,
            ),
        ] {
            let output = carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsOutput::builder()
                .set_unsuccessful(Some(items.into())).build();
            for retry_index in [0, FLOW_LOG_CREATE_ATTEMPTS - 1] {
                let rejection = FlowLogCreateRejection::from_output(&output).unwrap();
                assert_eq!(
                    rejection.retry_or_error(retry_index).is_ok(),
                    expected_retry && retry_index == 0
                );
            }
        }
    }

    #[test]
    fn iam_propagation_matching_stays_exact() {
        for (code, message, expected) in [
            (None, None, false),
            (Some("403"), None, true),
            (Some("AccessDenied"), None, false),
            (None, Some("prefix Unable to assume IAM role suffix"), true),
            (None, Some("prefix Not authorized suffix"), true),
            (None, Some("prefix Access Denied suffix"), true),
            (None, Some("not authorized"), false),
            (Some("4030"), Some("Access denied"), false),
        ] {
            let output = carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsOutput::builder()
                .unsuccessful(unsuccessful_item(None, code, message))
                .build();
            let rejection = FlowLogCreateRejection::from_output(&output).unwrap();
            assert_eq!(
                rejection.is_iam_propagation(),
                expected,
                "{code:?} {message:?}"
            );
            let mut metadata = ErrorMetadata::builder();
            if let Some(code) = code {
                metadata = metadata.code(code);
            }
            if let Some(message) = message {
                metadata = metadata.message(message);
            }
            let metadata = metadata.build();
            for classification in [
                SendClassification::Rejected,
                SendClassification::NotSent(carina_aws_types::Transience::Permanent),
                SendClassification::MaybeSent {
                    final_attempt: carina_aws_types::FinalAttempt::Sent,
                },
            ] {
                let service_error = CreateFlowLogsError::generic(metadata.clone());
                let failure = SendFailure::new(
                    classification,
                    SdkError::service_error(
                        service_error,
                        HttpResponse::new(400.try_into().unwrap(), SdkBody::empty()),
                    ),
                );
                let expected_request = expected && classification == SendClassification::Rejected;
                match FlowLogCreateRejection::from_failure(failure) {
                    Ok(rejection) => {
                        assert!(
                            expected_request,
                            "non-IAM or unproven request must use tagged_ec2_send_error: {classification:?} {code:?} {message:?}"
                        );
                        assert!(rejection.is_iam_propagation());
                    }
                    Err(_) => assert!(
                        !expected_request,
                        "request: {classification:?} {code:?} {message:?}"
                    ),
                }
            }
        }
    }

    #[test]
    fn proven_rejection_rotates_the_flow_log_client_token() {
        let invocation_token = ClientToken::new();
        let attempt_token = FlowLogRequestToken::from_invocation(&invocation_token);
        let output =
            carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsOutput::builder(
            )
            .unsuccessful(unsuccessful_item(
                None,
                Some("403"),
                Some("Unable to assume IAM role"),
            ))
            .build();
        let rejection = FlowLogCreateRejection::from_output(&output)
            .expect("an empty unsuccessful response proves rejection");

        assert!(rejection.is_iam_propagation());
        let retryable = rejection
            .retry_or_error(0)
            .expect("the IAM rejection permits a retry");
        let next = attempt_token.rotate_after(&retryable);

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
            carina_aws_types::SendClassification::Rejected,
            SdkError::service_error(service_error, response),
        );
        let rejection = FlowLogCreateRejection::from_failure(failure)
            .expect("a rejected IAM propagation error is safe to retry");

        assert!(rejection.is_iam_propagation());
        let retryable = rejection
            .retry_or_error(0)
            .expect("the IAM rejection permits a retry");
        let next = attempt_token.rotate_after(&retryable);
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
            carina_aws_types::SendClassification::MaybeSent {
                final_attempt: carina_aws_types::FinalAttempt::Sent,
            },
            SdkError::service_error(service_error, response),
        );
        let maybe_sent = FlowLogMaybeSent::from_failure(&failure)
            .expect("the sticky outcome must override the final throttling rejection");

        let discovery_token = attempt_token.preserve_after(maybe_sent);

        assert_eq!(discovery_token, invocation_token);
    }
}
