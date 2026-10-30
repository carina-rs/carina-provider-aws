use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use aws_sdk_ec2::types::TransitGatewayState;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    CreatePoll, DeletePoll, build_tag_specification_with_create_token, optional_enum_struct_field,
    optional_int_struct_field, optional_string_list_struct_field,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2TransitGatewayReadModelSupplement;
use crate::send_models_generated::{Ec2TransitGatewayCreateInput, Ec2TransitGatewayUpdateInput};

fn transit_gateway_create_poll_state(state: Option<&TransitGatewayState>) -> CreatePoll<()> {
    match state {
        Some(TransitGatewayState::Available) => CreatePoll::Ready(()),
        Some(TransitGatewayState::Deleted) => CreatePoll::Failed,
        // The current SDK has no named Failed variant, but preserve the
        // protocol contract if AWS adds or returns that terminal spelling.
        Some(state) if state.as_str() == "failed" => CreatePoll::Failed,
        Some(
            TransitGatewayState::Pending
            | TransitGatewayState::Modifying
            | TransitGatewayState::Deleting,
        )
        | None => CreatePoll::Pending,
        // The SDK enum is non-exhaustive. Other future states are not proof
        // that AWS finished or discarded the create, so retain the record.
        Some(_unrecognized_state) => CreatePoll::Pending,
    }
}

fn transit_gateway_delete_poll_state(state: Option<&TransitGatewayState>) -> DeletePoll {
    match state {
        Some(TransitGatewayState::Deleted) | None => DeletePoll::Gone,
        Some(
            TransitGatewayState::Available
            | TransitGatewayState::Deleting
            | TransitGatewayState::Modifying
            | TransitGatewayState::Pending,
        ) => DeletePoll::Pending,
        // The SDK enum is non-exhaustive. An unknown future state does not
        // prove that the transit gateway is gone.
        Some(_unrecognized_state) => DeletePoll::Pending,
    }
}

fn build_create_transit_gateway_options(
    id: &ResourceId,
    options_value: Option<&Value>,
    schema: &ResourceSchema,
) -> ProviderResult<Option<aws_sdk_ec2::types::TransitGatewayRequestOptions>> {
    use aws_sdk_ec2::types::{
        AutoAcceptSharedAttachmentsValue, DefaultRouteTableAssociationValue,
        DefaultRouteTablePropagationValue, DnsSupportValue, MulticastSupportValue,
        SecurityGroupReferencingSupportValue, VpnEcmpSupportValue,
    };

    let Some(options_value) = options_value else {
        return Ok(None);
    };
    let mut projected = Resource::from_id(id.clone());
    projected
        .attributes
        .insert("options".to_string(), options_value.clone());
    let resource = &projected;
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
    )? {
        options = options.auto_accept_shared_attachments(AutoAcceptSharedAttachmentsValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "default_route_table_association",
    )? {
        options =
            options.default_route_table_association(DefaultRouteTableAssociationValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "default_route_table_propagation",
    )? {
        options =
            options.default_route_table_propagation(DefaultRouteTablePropagationValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "dns_support")? {
        options = options.dns_support(DnsSupportValue::from(v));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "multicast_support")? {
        options = options.multicast_support(MulticastSupportValue::from(v));
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
    if let Some(blocks) =
        optional_string_list_struct_field(resource, "options", "transit_gateway_cidr_blocks")
        && !blocks.is_empty()
    {
        options = options.set_transit_gateway_cidr_blocks(Some(blocks));
        has_options = true;
    }
    if let Some(v) = optional_enum_struct_field(resource, schema, "options", "vpn_ecmp_support")? {
        options = options.vpn_ecmp_support(VpnEcmpSupportValue::from(v));
        has_options = true;
    }

    Ok(has_options.then(|| options.build()))
}

impl AwsProvider {
    /// Read an EC2 Transit Gateway
    pub(crate) async fn read_ec2_transit_gateway(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Measured with account carina-test-009 in ap-northeast-1 on
        // 2026-10-24: `aws ec2 describe-transit-gateways
        // --transit-gateway-ids <unknown>` returned
        // InvalidTransitGatewayID.NotFound.
        let result = self
            .ec2_client
            .describe_transit_gateways()
            .transit_gateway_ids(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateways", e)
                    .for_resource(id.clone())
            })?;

        if let Some(tgw) = result.transit_gateways().first() {
            // Skip deleted transit gateways
            if tgw.state() == Some(&TransitGatewayState::Deleted) {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let fields = Self::extract_ec2_transit_gateway_attributes(tgw);

            let (_, model) = fields.into_model(
                id,
                Ec2TransitGatewayReadModelSupplement {
                    tags: Self::ec2_tags_to_value(tgw.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Transit Gateway
    pub(crate) async fn create_ec2_transit_gateway_start<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let input = match Ec2TransitGatewayCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (description, options, tags) = input.into_parts();
        let mut req = self.ec2_client.create_transit_gateway();

        if let Some(description) = description {
            req = req.description(description);
        }

        let options =
            match build_create_transit_gateway_options(&resource.id, options.as_ref(), schema) {
                Ok(options) => options,
                Err(error) => return Err(attempt.untracked(error)),
            };
        if let Some(options) = options {
            req = req.options(options);
        }

        req = req.tag_specifications(build_tag_specification_with_create_token(
            tags.as_ref(),
            aws_sdk_ec2::types::ResourceType::TransitGateway,
            client_token,
        ));

        let result = req.send().await;

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::TransitGateway.create_context(client_token, sent),
            result,
            |output| {
                output
                    .transit_gateway()
                    .and_then(|gateway| gateway.transit_gateway_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 Transit Gateway.
    pub(crate) async fn update_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2TransitGatewayUpdateInput::decode(&to)?;
        let (description, tags) = input.into_parts();
        let from_description = match from.attributes.get("description") {
            Some(Value::Concrete(ConcreteValue::String(value))) => Some(value.expose_plaintext()),
            _ => None,
        };
        if from_description != description.as_deref() {
            self.ec2_client
                .modify_transit_gateway()
                .transit_gateway_id(identifier.as_str())
                .description(description.as_deref().unwrap_or(""))
                .send()
                .await
                .map_err(|error| {
                    api_error_with_meta("Failed to update transit gateway description", error)
                        .for_resource(id.clone())
                })?;
        }
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_transit_gateway(&id, identifier).await)
    }

    /// Perform one describe call for a pending transit gateway create.
    pub(crate) async fn poll_ec2_transit_gateway_create(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<CreatePoll<()>> {
        // The same 2026-10-24 ap-northeast-1 command above returns
        // InvalidTransitGatewayID.NotFound for an unknown id. On poll/resume
        // of an issued record the caller deliberately keeps that error
        // pending: EC2 can briefly hide a just-created resource, and
        // discarding the record would risk a duplicate create.
        let result = self
            .ec2_client
            .describe_transit_gateways()
            .transit_gateway_ids(transit_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateway", e)
                    .for_resource(id.clone())
            })?;
        Ok(transit_gateway_create_poll_state(
            result
                .transit_gateways()
                .first()
                .and_then(|tgw| tgw.state()),
        ))
    }

    /// Perform one describe call for a pending transit gateway delete.
    pub(crate) async fn poll_ec2_transit_gateway_delete(
        &self,
        id: &ResourceId,
        transit_gateway_id: &str,
    ) -> ProviderResult<DeletePoll> {
        let result = self
            .ec2_client
            .describe_transit_gateways()
            .transit_gateway_ids(transit_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe transit gateway", e)
                    .for_resource(id.clone())
            })?;
        Ok(transit_gateway_delete_poll_state(
            result
                .transit_gateways()
                .first()
                .and_then(|tgw| tgw.state()),
        ))
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

    #[test]
    fn create_poll_preserves_waiter_state_semantics() {
        for state in TransitGatewayState::values() {
            let expected = match *state {
                "available" => CreatePoll::Ready(()),
                "deleted" => CreatePoll::Failed,
                "pending" | "modifying" | "deleting" => CreatePoll::Pending,
                new_known_state => {
                    panic!("classify new SDK transit gateway state {new_known_state}")
                }
            };
            assert_eq!(
                transit_gateway_create_poll_state(Some(&TransitGatewayState::from(*state))),
                expected,
                "state {state}"
            );
        }
        assert_eq!(transit_gateway_create_poll_state(None), CreatePoll::Pending);
        assert_eq!(
            transit_gateway_create_poll_state(Some(&TransitGatewayState::from("failed"))),
            CreatePoll::Failed
        );
        assert_eq!(
            transit_gateway_create_poll_state(Some(&TransitGatewayState::from("future-state"))),
            CreatePoll::Pending
        );
    }

    #[test]
    fn delete_poll_preserves_waiter_state_semantics() {
        assert_eq!(
            transit_gateway_delete_poll_state(Some(&TransitGatewayState::Deleted)),
            DeletePoll::Gone
        );
        assert_eq!(transit_gateway_delete_poll_state(None), DeletePoll::Gone);
        assert_eq!(
            transit_gateway_delete_poll_state(Some(&TransitGatewayState::Available)),
            DeletePoll::Pending
        );
        assert_eq!(
            transit_gateway_delete_poll_state(Some(&TransitGatewayState::from("failed"))),
            DeletePoll::Pending
        );
    }

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
        use carina_core::schema::{AttributeType, enum_identity};

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
        crate::test_rootless_type(attr_type)
            .canonicalize_state(Value::Concrete(ConcreteValue::String(api_value.into())))
    }

    fn one_enum_option(field: &str, value: Value) -> Resource {
        resource_with_options([(field.to_string(), value)].into_iter().collect())
    }

    #[test]
    fn create_reads_nested_options_dns_support_canonical_enum() {
        let resource = one_enum_option("dns_support", canonical_dns_support("disable"));
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(options.dns_support(), Some(&DnsSupportValue::Disable));
    }

    #[test]
    fn create_reads_nested_options_vpn_ecmp_support() {
        let resource = one_enum_option(
            "vpn_ecmp_support",
            Value::Concrete(ConcreteValue::String("enable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(
            options.vpn_ecmp_support(),
            Some(&VpnEcmpSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_multicast_support() {
        let resource = one_enum_option(
            "multicast_support",
            Value::Concrete(ConcreteValue::String("enable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(
            options.multicast_support(),
            Some(&MulticastSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_security_group_referencing_support() {
        let resource = one_enum_option(
            "security_group_referencing_support",
            Value::Concrete(ConcreteValue::String("enable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(
            options.security_group_referencing_support(),
            Some(&SecurityGroupReferencingSupportValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_auto_accept_shared_attachments() {
        let resource = one_enum_option(
            "auto_accept_shared_attachments",
            Value::Concrete(ConcreteValue::String("enable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(
            options.auto_accept_shared_attachments(),
            Some(&AutoAcceptSharedAttachmentsValue::Enable)
        );
    }

    #[test]
    fn create_reads_nested_options_default_route_table_association() {
        let resource = one_enum_option(
            "default_route_table_association",
            Value::Concrete(ConcreteValue::String("disable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(
            options.default_route_table_association(),
            Some(&DefaultRouteTableAssociationValue::Disable)
        );
    }

    #[test]
    fn create_reads_nested_options_default_route_table_propagation() {
        let resource = one_enum_option(
            "default_route_table_propagation",
            Value::Concrete(ConcreteValue::String("disable".to_string().into())),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
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
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
        assert_eq!(options.amazon_side_asn(), Some(64512));
    }

    #[test]
    fn create_reads_nested_options_transit_gateway_cidr_blocks_list() {
        let resource = resource_with_options(
            [(
                "transit_gateway_cidr_blocks".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::String("10.0.0.0/24".to_string().into()),
                )])),
            )]
            .into_iter()
            .collect(),
        );
        let options = build_create_transit_gateway_options(
            &resource.id,
            resource.get_attr("options"),
            &schema(),
        )
        .expect("valid options")
        .expect("options");
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
            build_create_transit_gateway_options(
                &resource.id,
                resource.get_attr("options"),
                &schema(),
            )
            .expect("valid options")
            .is_none(),
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
        let fields = AwsProvider::extract_ec2_transit_gateway_attributes(&tgw);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = fields.options() else {
            panic!("expected nested options map");
        };
        assert_eq!(
            options.get("dns_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "enable".to_string().into()
            )))
        );
        assert_eq!(
            options.get("vpn_ecmp_support"),
            Some(&Value::Concrete(ConcreteValue::String(
                "disable".to_string().into()
            )))
        );
        assert_eq!(
            options.get("transit_gateway_cidr_blocks"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("10.0.0.0/24".to_string().into()))
            ])))
        );
    }

    #[test]
    fn create_top_level_dns_support_is_ignored() {
        let mut resource = Resource::with_provider("aws", "ec2.TransitGateway", "test", None);
        resource.set_attr(
            "dns_support".to_string(),
            Value::Concrete(ConcreteValue::String("disable".to_string().into())),
        );
        assert!(
            build_create_transit_gateway_options(
                &resource.id,
                resource.get_attr("options"),
                &schema(),
            )
            .expect("valid options")
            .is_none()
        );
    }
}
