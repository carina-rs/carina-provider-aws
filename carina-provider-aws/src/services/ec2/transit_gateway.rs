use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    PollState, build_tag_specification, optional_enum_struct_field, optional_int_struct_field,
    optional_string_list_struct_field, wait_for_ec2_state,
};

fn build_create_transit_gateway_options(
    resource: &Resource,
    schema: &ResourceSchema,
) -> Option<aws_sdk_ec2::types::TransitGatewayRequestOptions> {
    use aws_sdk_ec2::types::{
        AutoAcceptSharedAttachmentsValue, DefaultRouteTableAssociationValue,
        DefaultRouteTablePropagationValue, DnsSupportValue, MulticastSupportValue,
        SecurityGroupReferencingSupportValue, VpnEcmpSupportValue,
    };

    let mut options = aws_sdk_ec2::types::TransitGatewayRequestOptions::builder();
    let mut has_options = false;

    if let Some(asn) = optional_int_struct_field(resource, "options", "amazon_side_asn") {
        options = options.amazon_side_asn(asn);
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "auto_accept_shared_attachments",
    ) {
        options = options.auto_accept_shared_attachments(AutoAcceptSharedAttachmentsValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "default_route_table_association",
    ) {
        options =
            options.default_route_table_association(DefaultRouteTableAssociationValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "default_route_table_propagation",
    ) {
        options =
            options.default_route_table_propagation(DefaultRouteTablePropagationValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "dns_support") {
        options = options.dns_support(DnsSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "multicast_support") {
        options = options.multicast_support(MulticastSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "security_group_referencing_support",
    ) {
        options = options
            .security_group_referencing_support(SecurityGroupReferencingSupportValue::from(v));
        has_options = true;
    }
    if let Some(blocks) =
        optional_string_list_struct_field(resource, "options", "transit_gateway_cidr_blocks")
        && !blocks.is_empty()
    {
        options = options.set_transit_gateway_cidr_blocks(Some(blocks));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "vpn_ecmp_support") {
        options = options.vpn_ecmp_support(VpnEcmpSupportValue::from(v));
        has_options = true;
    }

    has_options.then(|| options.build())
}

impl AwsProvider {
    /// Read an EC2 Transit Gateway
    pub(crate) async fn read_ec2_transit_gateway(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .ec2_client
            .describe_transit_gateways()
            .transit_gateway_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to describe transit gateways",
                    "ec2.DescribeTransitGateways",
                    e,
                )
                .for_resource(id.clone())
            })?;

        if let Some(tgw) = result.transit_gateways().first() {
            // Skip deleted transit gateways
            if tgw.state().map(|s| s.as_str()) == Some("deleted") {
                return Ok(State::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            let identifier_value =
                Self::extract_ec2_transit_gateway_attributes(tgw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(tgw.tags()) {
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

    /// Create an EC2 Transit Gateway
    pub(crate) async fn create_ec2_transit_gateway(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let mut req = self.ec2_client.create_transit_gateway();

        if let Some(Value::Concrete(ConcreteValue::String(desc))) = resource.get_attr("description")
        {
            req = req.description(desc);
        }

        if let Some(options) = build_create_transit_gateway_options(resource, schema) {
            req = req.options(options);
        }

        // Apply tags via TagSpecifications
        if let Some(tag_spec) =
            build_tag_specification(resource, aws_sdk_ec2::types::ResourceType::TransitGateway)
        {
            req = req.tag_specifications(tag_spec);
        }

        let result = req.send().await.map_err(|e| {
            api_error_with_meta(
                "Failed to create transit gateway",
                "ec2.CreateTransitGateway",
                e,
            )
            .for_resource(resource.id.clone())
        })?;

        let tgw_id = result
            .transit_gateway()
            .and_then(|tgw| tgw.transit_gateway_id())
            .ok_or_else(|| {
                ProviderError::api_error("Transit Gateway created but no ID returned")
                    .for_resource(resource.id.clone())
            })?;

        // Wait for transit gateway to become available
        self.wait_for_transit_gateway_available(&resource.id, tgw_id)
            .await?;

        // Read back
        self.read_ec2_transit_gateway(&resource.id, Some(tgw_id))
            .await
    }

    /// Update an EC2 Transit Gateway (tags only for now)
    pub(crate) async fn update_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        self.apply_ec2_tags(
            &id,
            identifier,
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        self.read_ec2_transit_gateway(&id, Some(identifier)).await
    }

    /// Delete an EC2 Transit Gateway
    pub(crate) async fn delete_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway()
            .transit_gateway_id(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to delete transit gateway",
                    "ec2.DeleteTransitGateway",
                    e,
                )
                .for_resource(id.clone())
            })?;

        // Wait for transit gateway to be deleted
        self.wait_for_transit_gateway_deleted(&id, identifier)
            .await?;

        Ok(())
    }

    /// Wait for a transit gateway to reach the "available" state
    async fn wait_for_transit_gateway_available(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateways()
                    .transit_gateway_ids(transit_gateway_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway",
                            "ec2.DescribeTransitGateways",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(
                    if let Some(tgw) = result.transit_gateways().first()
                        && let Some(state) = tgw.state()
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
            "Timeout waiting for transit gateway to become available",
            "Transit gateway creation failed",
        )
        .await
    }

    /// Wait for a transit gateway to be deleted
    async fn wait_for_transit_gateway_deleted(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<()> {
        let ec2 = &self.ec2_client;
        let rid = id.clone();
        wait_for_ec2_state(
            id,
            || async {
                let result = ec2
                    .describe_transit_gateways()
                    .transit_gateway_ids(transit_gateway_id)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta(
                            "Failed to describe transit gateway",
                            "ec2.DescribeTransitGateways",
                            e,
                        )
                        .for_resource(rid.clone())
                    })?;
                Ok(if let Some(tgw) = result.transit_gateways().first() {
                    if tgw.state().map(|s| s.as_str()) == Some("deleted") {
                        PollState::Gone
                    } else {
                        PollState::Pending
                    }
                } else {
                    PollState::Gone
                })
            },
            60,
            "Timeout waiting for transit gateway to be deleted",
            "Transit gateway deletion failed",
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ec2::types::{
        AutoAcceptSharedAttachmentsValue, DefaultRouteTableAssociationValue,
        DefaultRouteTablePropagationValue, DnsSupportValue, MulticastSupportValue,
        SecurityGroupReferencingSupportValue, TransitGateway, TransitGatewayOptions,
        VpnEcmpSupportValue,
    };
    use indexmap::IndexMap;

    fn schema() -> ResourceSchema {
        crate::schemas::generated::ec2::transit_gateway::ec2_transit_gateway_config().schema
    }

    fn resource_with_options(options: IndexMap<String, Value>) -> Resource {
        let mut resource = Resource::with_provider("aws", "ec2.TransitGateway", "test", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );
        resource
    }

    fn canonical_dns_support(api_value: &str) -> Value {
        use carina_core::schema::{AttributeType, Schema, enum_identity};

        let attr_type = AttributeType::enum_(
            enum_identity(
                "DnsSupport",
                Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions"),
            ),
            Some(vec!["enable".to_string(), "disable".to_string()]),
            vec![
                ("enable".to_string(), "enable".to_string()),
                ("disable".to_string(), "disable".to_string()),
            ],
            None,
            None,
        );
        Schema::flat(attr_type)
            .canonicalize(Value::Concrete(ConcreteValue::enum_identifier(api_value)))
    }

    fn one_enum_option(field: &str, value: Value) -> Resource {
        resource_with_options([(field.to_string(), value)].into_iter().collect())
    }

    #[test]
    fn create_reads_nested_options_dns_support_canonical_enum() {
        let resource = one_enum_option("dns_support", canonical_dns_support("disable"));
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(options.dns_support(), Some(&DnsSupportValue::Disable));
    }

    #[test]
    fn create_reads_nested_options_vpn_ecmp_support() {
        let resource = one_enum_option(
            "vpn_ecmp_support",
            Value::Concrete(ConcreteValue::String("enable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.vpn_ecmp_support(),
            Some(&VpnEcmpSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_multicast_support() {
        let resource = one_enum_option(
            "multicast_support",
            Value::Concrete(ConcreteValue::String("enable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.multicast_support(),
            Some(&MulticastSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_security_group_referencing_support() {
        let resource = one_enum_option(
            "security_group_referencing_support",
            Value::Concrete(ConcreteValue::String("enable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.security_group_referencing_support(),
            Some(&SecurityGroupReferencingSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_auto_accept_shared_attachments() {
        let resource = one_enum_option(
            "auto_accept_shared_attachments",
            Value::Concrete(ConcreteValue::String("enable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.auto_accept_shared_attachments(),
            Some(&AutoAcceptSharedAttachmentsValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_default_route_table_association() {
        let resource = one_enum_option(
            "default_route_table_association",
            Value::Concrete(ConcreteValue::String("disable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.default_route_table_association(),
            Some(&DefaultRouteTableAssociationValue::Disable)
        );
    }

    #[test]
    fn create_reads_nested_options_default_route_table_propagation() {
        let resource = one_enum_option(
            "default_route_table_propagation",
            Value::Concrete(ConcreteValue::String("disable".to_string())),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(
            options.default_route_table_propagation(),
            Some(&DefaultRouteTablePropagationValue::Disable)
        );
    }

    #[test]
    fn create_reads_nested_options_amazon_side_asn_int() {
        let resource = resource_with_options(
            [(
                "amazon_side_asn".to_string(),
                Value::Concrete(ConcreteValue::Int(64512)),
            )]
            .into_iter()
            .collect(),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(options.amazon_side_asn(), Some(64512));
    }

    #[test]
    fn create_reads_nested_options_transit_gateway_cidr_blocks_list() {
        let resource = resource_with_options(
            [(
                "transit_gateway_cidr_blocks".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::String("10.0.0.0/24".to_string()),
                )])),
            )]
            .into_iter()
            .collect(),
        );
        let options = build_create_transit_gateway_options(&resource, &schema()).expect("options");
        assert_eq!(options.transit_gateway_cidr_blocks(), ["10.0.0.0/24"]);
    }

    #[test]
    fn create_tgw_skips_explicit_empty_cidr_blocks_list() {
        let resource = resource_with_options(
            [(
                "transit_gateway_cidr_blocks".to_string(),
                Value::Concrete(ConcreteValue::List(vec![])),
            )]
            .into_iter()
            .collect(),
        );

        assert!(
            build_create_transit_gateway_options(&resource, &schema()).is_none(),
            "an empty CIDR list must not cause the SDK request to carry options"
        );
    }

    #[test]
    fn state_read_emits_nested_options_map() {
        let tgw = TransitGateway::builder()
            .transit_gateway_id("tgw-123")
            .options(
                TransitGatewayOptions::builder()
                    .dns_support(DnsSupportValue::Enable)
                    .vpn_ecmp_support(VpnEcmpSupportValue::Disable)
                    .transit_gateway_cidr_blocks("10.0.0.0/24")
                    .build(),
            )
            .build();
        let mut attrs = HashMap::new();
        AwsProvider::extract_ec2_transit_gateway_attributes(&tgw, &mut attrs);

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
            options.get("vpn_ecmp_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "disable".to_string()
            )))
        );
        assert_eq!(
            options.get("transit_gateway_cidr_blocks"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("10.0.0.0/24".to_string()))
            ])))
        );
        assert!(!attrs.contains_key("dns_support"));
    }

    #[test]
    fn create_top_level_dns_support_is_ignored() {
        let mut resource = Resource::with_provider("aws", "ec2.TransitGateway", "test", None);
        resource.set_attr(
            "dns_support".to_string(),
            Value::Concrete(ConcreteValue::String("disable".to_string())),
        );
        assert!(build_create_transit_gateway_options(&resource, &schema()).is_none());
    }
}
