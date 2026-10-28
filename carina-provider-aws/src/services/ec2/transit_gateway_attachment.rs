use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, ReadOutcome, Sent};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use aws_sdk_ec2::types::TransitGatewayAttachmentState;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    CreatePoll, DeletePoll, build_tag_specification_with_create_token, optional_enum_struct_field,
    require_string_attr,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

fn transit_gateway_attachment_create_poll_state(
    state: Option<&TransitGatewayAttachmentState>,
) -> CreatePoll<()> {
    match state {
        Some(TransitGatewayAttachmentState::Available) => CreatePoll::Ready(()),
        Some(
            TransitGatewayAttachmentState::Failed
            | TransitGatewayAttachmentState::Rejected
            | TransitGatewayAttachmentState::Deleted,
        ) => CreatePoll::Failed,
        Some(
            TransitGatewayAttachmentState::Pending
            | TransitGatewayAttachmentState::Modifying
            | TransitGatewayAttachmentState::Deleting
            | TransitGatewayAttachmentState::Failing
            | TransitGatewayAttachmentState::Initiating
            | TransitGatewayAttachmentState::InitiatingRequest
            | TransitGatewayAttachmentState::PendingAcceptance
            | TransitGatewayAttachmentState::Rejecting
            | TransitGatewayAttachmentState::RollingBack,
        )
        | None => CreatePoll::Pending,
        // The SDK enum is non-exhaustive. Other future states are not proof
        // that AWS finished or discarded the create, so retain the record.
        Some(_unrecognized_state) => CreatePoll::Pending,
    }
}

fn transit_gateway_attachment_delete_poll_state(
    state: Option<&TransitGatewayAttachmentState>,
) -> DeletePoll {
    match state {
        Some(TransitGatewayAttachmentState::Deleted) | None => DeletePoll::Gone,
        Some(
            TransitGatewayAttachmentState::Available
            | TransitGatewayAttachmentState::Deleting
            | TransitGatewayAttachmentState::Failed
            | TransitGatewayAttachmentState::Failing
            | TransitGatewayAttachmentState::Initiating
            | TransitGatewayAttachmentState::InitiatingRequest
            | TransitGatewayAttachmentState::Modifying
            | TransitGatewayAttachmentState::Pending
            | TransitGatewayAttachmentState::PendingAcceptance
            | TransitGatewayAttachmentState::Rejected
            | TransitGatewayAttachmentState::Rejecting
            | TransitGatewayAttachmentState::RollingBack,
        ) => DeletePoll::Pending,
        // The SDK enum is non-exhaustive. An unknown future state does not
        // prove that the attachment is gone.
        Some(_unrecognized_state) => DeletePoll::Pending,
    }
}

fn build_create_transit_gateway_attachment_options(
    resource: &Resource,
    schema: &ResourceSchema,
) -> ProviderResult<Option<aws_sdk_ec2::types::CreateTransitGatewayVpcAttachmentRequestOptions>> {
    use aws_sdk_ec2::types::{
        ApplianceModeSupportValue, DnsSupportValue, Ipv6SupportValue,
        SecurityGroupReferencingSupportValue,
    };

    let mut options =
        aws_sdk_ec2::types::CreateTransitGatewayVpcAttachmentRequestOptions::builder();
    let mut has_options = false;

    if let Some(v) =
        optional_enum_struct_field(resource, schema, "options", "appliance_mode_support")?
    {
        options = options.appliance_mode_support(ApplianceModeSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "dns_support")? {
        options = options.dns_support(DnsSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "ipv6_support")? {
        options = options.ipv6_support(Ipv6SupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "security_group_referencing_support",
    )? {
        options = options
            .security_group_referencing_support(SecurityGroupReferencingSupportValue::from(v));
        has_options = true;
    }

    Ok(has_options.then(|| options.build()))
}

impl AwsProvider {
    /// Read an EC2 Transit Gateway VPC Attachment
    pub(crate) async fn read_ec2_transit_gateway_attachment(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Measured with account carina-test-009 in ap-northeast-1 on
        // 2026-10-24: `aws ec2 describe-transit-gateway-vpc-attachments
        // --transit-gateway-attachment-ids <unknown>` returned
        // InvalidTransitGatewayAttachmentID.NotFound.
        let result = self
            .ec2_client
            .describe_transit_gateway_vpc_attachments()
            .transit_gateway_attachment_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateway VPC attachments", e)
                    .for_resource(id.clone())
            })?;

        if let Some(att) = result.transit_gateway_vpc_attachments().first() {
            // Skip deleted attachments
            if att.state() == Some(&TransitGatewayAttachmentState::Deleted) {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let _identifier_value =
                Self::extract_ec2_transit_gateway_attachment_attributes(att, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(att.tags()) {
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

    /// Create an EC2 Transit Gateway VPC Attachment
    pub(crate) async fn create_ec2_transit_gateway_attachment_start<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let transit_gateway_id = match require_string_attr(resource, "transit_gateway_id") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let subnet_ids = match resource.get_attr("subnet_ids") {
            Some(Value::Concrete(ConcreteValue::List(ids))) => {
                let mut result = Vec::new();
                for id_val in ids {
                    if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                        result.push(s.expose_plaintext().to_string());
                    }
                }
                if result.is_empty() {
                    return Err(attempt.untracked(
                        ProviderError::invalid_input("subnet_ids must not be empty")
                            .for_resource(resource.id.clone()),
                    ));
                }
                result
            }
            _ => {
                return Err(attempt.untracked(
                    ProviderError::invalid_input("subnet_ids is required")
                        .for_resource(resource.id.clone()),
                ));
            }
        };

        let mut req = self
            .ec2_client
            .create_transit_gateway_vpc_attachment()
            .transit_gateway_id(&transit_gateway_id)
            .vpc_id(&vpc_id);

        for subnet_id in &subnet_ids {
            req = req.subnet_ids(subnet_id);
        }

        let options = match build_create_transit_gateway_attachment_options(resource, schema) {
            Ok(options) => options,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(options) = options {
            req = req.options(options);
        }

        req = req.tag_specifications(build_tag_specification_with_create_token(
            resource,
            aws_sdk_ec2::types::ResourceType::TransitGatewayAttachment,
            client_token,
        ));

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::TransitGatewayVpcAttachment
                .create_context(client_token, Sent::AllRequestAttributes),
            result,
            |output| {
                output
                    .transit_gateway_vpc_attachment()
                    .and_then(|attachment| attachment.transit_gateway_attachment_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 Transit Gateway VPC Attachment (tags only)
    pub(crate) async fn update_ec2_transit_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self
            .read_ec2_transit_gateway_attachment(&id, identifier)
            .await)
    }

    /// Perform one describe call for a pending transit gateway attachment create.
    pub(crate) async fn poll_ec2_transit_gateway_attachment_create(
        &self,
        id: &ResourceId,
        attachment_id: &str,
    ) -> ProviderResult<CreatePoll<()>> {
        // The same 2026-10-24 ap-northeast-1 command above returns
        // InvalidTransitGatewayAttachmentID.NotFound for an unknown id. On
        // poll/resume of an issued record the caller deliberately keeps that
        // error pending: EC2 can briefly hide a just-created resource, and
        // discarding the record would risk a duplicate create.
        let result = self
            .ec2_client
            .describe_transit_gateway_vpc_attachments()
            .transit_gateway_attachment_ids(attachment_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateway VPC attachment", e)
                    .for_resource(id.clone())
            })?;
        Ok(transit_gateway_attachment_create_poll_state(
            result
                .transit_gateway_vpc_attachments()
                .first()
                .and_then(|att| att.state()),
        ))
    }

    /// Perform one describe call for a pending transit gateway attachment delete.
    pub(crate) async fn poll_ec2_transit_gateway_attachment_delete(
        &self,
        id: &ResourceId,
        attachment_id: &str,
    ) -> ProviderResult<DeletePoll> {
        let result = self
            .ec2_client
            .describe_transit_gateway_vpc_attachments()
            .transit_gateway_attachment_ids(attachment_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateway VPC attachment", e)
                    .for_resource(id.clone())
            })?;
        Ok(transit_gateway_attachment_delete_poll_state(
            result
                .transit_gateway_vpc_attachments()
                .first()
                .and_then(|att| att.state()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ec2::types::{
        ApplianceModeSupportValue, DnsSupportValue, Ipv6SupportValue,
        SecurityGroupReferencingSupportValue, TransitGatewayVpcAttachment,
        TransitGatewayVpcAttachmentOptions,
    };
    use indexmap::IndexMap;

    #[test]
    fn create_poll_preserves_waiter_state_semantics() {
        for state in TransitGatewayAttachmentState::values() {
            let expected = match *state {
                "available" => CreatePoll::Ready(()),
                "failed" | "rejected" | "deleted" => CreatePoll::Failed,
                "pending" | "modifying" | "deleting" | "failing" | "initiating"
                | "initiatingRequest" | "pendingAcceptance" | "rejecting" | "rollingBack" => {
                    CreatePoll::Pending
                }
                new_known_state => {
                    panic!("classify new SDK transit gateway attachment state {new_known_state}")
                }
            };
            assert_eq!(
                transit_gateway_attachment_create_poll_state(Some(
                    &TransitGatewayAttachmentState::from(*state),
                )),
                expected,
                "state {state}"
            );
        }
        assert_eq!(
            transit_gateway_attachment_create_poll_state(None),
            CreatePoll::Pending
        );
        assert_eq!(
            transit_gateway_attachment_create_poll_state(Some(
                &TransitGatewayAttachmentState::from("future-state"),
            )),
            CreatePoll::Pending
        );
    }

    #[test]
    fn delete_poll_preserves_waiter_state_semantics() {
        assert_eq!(
            transit_gateway_attachment_delete_poll_state(Some(
                &TransitGatewayAttachmentState::Deleted,
            )),
            DeletePoll::Gone
        );
        assert_eq!(
            transit_gateway_attachment_delete_poll_state(None),
            DeletePoll::Gone
        );
        assert_eq!(
            transit_gateway_attachment_delete_poll_state(Some(
                &TransitGatewayAttachmentState::Available,
            )),
            DeletePoll::Pending
        );
        assert_eq!(
            transit_gateway_attachment_delete_poll_state(Some(
                &TransitGatewayAttachmentState::Failed,
            )),
            DeletePoll::Pending
        );
    }

    fn schema() -> ResourceSchema {
        crate::schemas::generated::ec2::transit_gateway_attachment::ec2_transit_gateway_attachment_config().schema
    }

    fn resource_with_options(options: IndexMap<String, Value>) -> Resource {
        let mut resource =
            Resource::with_provider("aws", "ec2.TransitGatewayAttachment", "test", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );
        resource
    }

    fn one_enum_option(field: &str, value: &str) -> Resource {
        resource_with_options(
            [(
                field.to_string(),
                Value::Concrete(ConcreteValue::String(value.to_string().into())),
            )]
            .into_iter()
            .collect(),
        )
    }

    #[test]
    fn create_reads_nested_options_appliance_mode_support() {
        let resource = one_enum_option("appliance_mode_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource, &schema())
            .expect("valid options")
            .expect("options");
        assert_eq!(
            options.appliance_mode_support(),
            Some(&ApplianceModeSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_dns_support() {
        let resource = one_enum_option("dns_support", "disable");
        let options = build_create_transit_gateway_attachment_options(&resource, &schema())
            .expect("valid options")
            .expect("options");
        assert_eq!(options.dns_support(), Some(&DnsSupportValue::Disable));
    }

    #[test]
    fn create_reads_nested_options_ipv6_support() {
        let resource = one_enum_option("ipv6_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource, &schema())
            .expect("valid options")
            .expect("options");
        assert_eq!(options.ipv6_support(), Some(&Ipv6SupportValue::Enable));
    }

    #[test]
    fn create_reads_nested_options_security_group_referencing_support() {
        let resource = one_enum_option("security_group_referencing_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource, &schema())
            .expect("valid options")
            .expect("options");
        assert_eq!(
            options.security_group_referencing_support(),
            Some(&SecurityGroupReferencingSupportValue::Enable)
        );
    }

    #[test]
    fn state_read_emits_nested_options_map() {
        let attachment = TransitGatewayVpcAttachment::builder()
            .transit_gateway_attachment_id("tgw-attach-123")
            .options(
                TransitGatewayVpcAttachmentOptions::builder()
                    .dns_support(DnsSupportValue::Enable)
                    .ipv6_support(Ipv6SupportValue::Disable)
                    .security_group_referencing_support(
                        SecurityGroupReferencingSupportValue::Enable,
                    )
                    .build(),
            )
            .build();
        let mut attrs = HashMap::new();
        AwsProvider::extract_ec2_transit_gateway_attachment_attributes(&attachment, &mut attrs);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = attrs.get("options") else {
            panic!("expected nested options map, got {attrs:?}");
        };
        assert_eq!(
            options.get("dns_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "enable".to_string().into()
            )))
        );
        assert_eq!(
            options.get("ipv6_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "disable".to_string().into()
            )))
        );
        assert_eq!(
            options.get("security_group_referencing_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "enable".to_string().into()
            )))
        );
        assert!(!attrs.contains_key("dns_support"));
    }

    #[test]
    fn create_top_level_dns_support_is_ignored() {
        let mut resource =
            Resource::with_provider("aws", "ec2.TransitGatewayAttachment", "test", None);
        resource.set_attr(
            "dns_support".to_string(),
            Value::Concrete(ConcreteValue::String("disable".to_string().into())),
        );
        assert!(
            build_create_transit_gateway_attachment_options(&resource, &schema())
                .expect("valid options")
                .is_none()
        );
    }
}
