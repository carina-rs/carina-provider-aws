use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    PollState, build_tag_specification, optional_enum_struct_field, require_string_attr,
    wait_for_ec2_state,
};

fn build_create_transit_gateway_attachment_options(
    resource: &Resource,
) -> Option<aws_sdk_ec2::types::CreateTransitGatewayVpcAttachmentRequestOptions> {
    use aws_sdk_ec2::types::{
        ApplianceModeSupportValue, DnsSupportValue, Ipv6SupportValue,
        SecurityGroupReferencingSupportValue,
    };

    let mut options =
        aws_sdk_ec2::types::CreateTransitGatewayVpcAttachmentRequestOptions::builder();
    let mut has_options = false;

    if let Some(v) = optional_enum_struct_field(resource, "options", "appliance_mode_support") {
        options = options.appliance_mode_support(ApplianceModeSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, "options", "dns_support") {
        options = options.dns_support(DnsSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, "options", "ipv6_support") {
        options = options.ipv6_support(Ipv6SupportValue::from(v));
        has_options = true;
    }
    if let Some(v) =
        optional_enum_struct_field(resource, "options", "security_group_referencing_support")
    {
        options = options
            .security_group_referencing_support(SecurityGroupReferencingSupportValue::from(v));
        has_options = true;
    }

    has_options.then(|| options.build())
}

impl AwsProvider {
    /// Read an EC2 Transit Gateway VPC Attachment
    pub(crate) async fn read_ec2_transit_gateway_attachment(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .ec2_client
            .describe_transit_gateway_vpc_attachments()
            .transit_gateway_attachment_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe transit gateway VPC attachments",
                    "ec2.DescribeTransitGatewayVpcAttachments",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(att) = result.transit_gateway_vpc_attachments().first() {
            // Skip deleted attachments
            if att.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(State::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let identifier_value =
                Self::extract_ec2_transit_gateway_attachment_attributes(att, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(att.tags()) {
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

    /// Create an EC2 Transit Gateway VPC Attachment
    pub(crate) async fn create_ec2_transit_gateway_attachment(
        &self,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let transit_gateway_id = require_string_attr(resource, "transit_gateway_id")?;
        let vpc_id = require_string_attr(resource, "vpc_id")?;

        let subnet_ids = match resource.get_attr("subnet_ids") {
            Some(Value::Concrete(ConcreteValue::List(ids))) => {
                let mut result = Vec::new();
                for id_val in ids {
                    if let Value::Concrete(ConcreteValue::String(s)) = id_val {
                        result.push(s.clone());
                    }
                }
                if result.is_empty() {
                    return Err(ProviderError::invalid_input("subnet_ids must not be empty")
                        .for_resource(resource.id.clone()));
                }
                result
            }
            _ => {
                return Err(ProviderError::invalid_input("subnet_ids is required")
                    .for_resource(resource.id.clone()));
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

        if let Some(options) = build_create_transit_gateway_attachment_options(resource) {
            req = req.options(options);
        }

        // Apply tags via TagSpecifications
        if let Some(tag_spec) = build_tag_specification(
            resource,
            aws_sdk_ec2::types::ResourceType::TransitGatewayAttachment,
        ) {
            req = req.tag_specifications(tag_spec);
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta(
                "Failed to create transit gateway VPC attachment",
                "ec2.CreateTransitGatewayVpcAttachment",
                e,
            )
            .for_resource(resource.id.clone())
        })?;

        let att_id = result
            .transit_gateway_vpc_attachment()
            .and_then(|att| att.transit_gateway_attachment_id())
            .ok_or_else(|| {
                ProviderError::api_error("Transit Gateway Attachment created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Wait for attachment to become available
        self.wait_for_transit_gateway_attachment_available(&resource.id, att_id)
            .await?;

        // Read back
        self.read_ec2_transit_gateway_attachment(&resource.id, Some(att_id))
            .await
    }

    /// Update an EC2 Transit Gateway VPC Attachment (tags only)
    pub(crate) async fn update_ec2_transit_gateway_attachment(
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
        self.read_ec2_transit_gateway_attachment(&id, Some(identifier))
            .await
    }

    /// Delete an EC2 Transit Gateway VPC Attachment
    pub(crate) async fn delete_ec2_transit_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway_vpc_attachment()
            .transit_gateway_attachment_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to delete transit gateway VPC attachment",
                    "ec2.DeleteTransitGatewayVpcAttachment",
                    e,
                )
                .for_resource(id.clone())
            })?;

        // Wait for attachment to be deleted
        self.wait_for_transit_gateway_attachment_deleted(&id, identifier)
            .await?;

        Ok(())
    }

    /// Wait for a transit gateway attachment to reach the "available" state
    async fn wait_for_transit_gateway_attachment_available(
        &self,
        id: &ResourceId,
        attachment_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateway_vpc_attachments()
                    .transit_gateway_attachment_ids(attachment_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway VPC attachment",
                            "ec2.DescribeTransitGatewayVpcAttachments",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(
                    if let Some(att) = result.transit_gateway_vpc_attachments().first()
                        && let Some(state) = att.state()
                    {
                        match state.as_str() {
                            "available" => PollState::Ready,
                            "failed" | "deleted" => PollState::Failed,
                            _ => PollState::Pending,
                        }
                    } else {
                        PollState::Pending
                    },
                )
            },
            60,
            "Timeout waiting for transit gateway attachment to become available",
            "Transit gateway attachment creation failed",
        )
        .await
    }

    /// Wait for a transit gateway attachment to be deleted
    async fn wait_for_transit_gateway_attachment_deleted(
        &self,
        id: &ResourceId,
        attachment_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateway_vpc_attachments()
                    .transit_gateway_attachment_ids(attachment_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway VPC attachment",
                            "ec2.DescribeTransitGatewayVpcAttachments",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(
                    if let Some(att) = result.transit_gateway_vpc_attachments().first() {
                        if att.state().map(|s| s.as_str()) == Some("deleted") {
                            PollState::Gone
                        } else {
                            PollState::Pending
                        }
                    } else {
                        PollState::Gone
                    },
                )
            },
            60,
            "Timeout waiting for transit gateway attachment to be deleted",
            "Transit gateway attachment deletion failed",
        )
        .await
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
                Value::Concrete(ConcreteValue::String(value.to_string())),
            )]
            .into_iter()
            .collect(),
        )
    }

    #[test]
    fn create_reads_nested_options_appliance_mode_support() {
        let resource = one_enum_option("appliance_mode_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource).expect("options");
        assert_eq!(
            options.appliance_mode_support(),
            Some(&ApplianceModeSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_dns_support() {
        let resource = one_enum_option("dns_support", "disable");
        let options = build_create_transit_gateway_attachment_options(&resource).expect("options");
        assert_eq!(options.dns_support(), Some(&DnsSupportValue::Disable));
    }

    #[test]
    fn create_reads_nested_options_ipv6_support() {
        let resource = one_enum_option("ipv6_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource).expect("options");
        assert_eq!(options.ipv6_support(), Some(&Ipv6SupportValue::Enable));
    }

    #[test]
    fn create_reads_nested_options_security_group_referencing_support() {
        let resource = one_enum_option("security_group_referencing_support", "enable");
        let options = build_create_transit_gateway_attachment_options(&resource).expect("options");
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
                "enable".to_string()
            )))
        );
        assert_eq!(
            options.get("ipv6_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "disable".to_string()
            )))
        );
        assert_eq!(
            options.get("security_group_referencing_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "enable".to_string()
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
            Value::Concrete(ConcreteValue::String("disable".to_string())),
        );
        assert!(build_create_transit_gateway_attachment_options(&resource).is_none());
    }
}
