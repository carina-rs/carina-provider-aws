use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use aws_sdk_ec2::types::NatGatewayState;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    CreatePoll, DeletePoll, RetryPolicy, build_tag_specification_with_create_token,
    optional_enum_attr, require_string_attr, retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};

fn nat_gateway_create_poll_state(state: Option<&NatGatewayState>) -> CreatePoll<()> {
    match state {
        Some(NatGatewayState::Available) => CreatePoll::Ready(()),
        Some(NatGatewayState::Failed | NatGatewayState::Deleted) => CreatePoll::Failed,
        Some(NatGatewayState::Pending | NatGatewayState::Deleting) | None => CreatePoll::Pending,
        // The SDK enum is non-exhaustive. An unrecognized state is not proof
        // that AWS either finished or discarded the create, so retain it.
        Some(_unrecognized_state) => CreatePoll::Pending,
    }
}

fn nat_gateway_delete_poll_state(state: Option<&NatGatewayState>) -> DeletePoll {
    match state {
        Some(NatGatewayState::Deleted) | None => DeletePoll::Gone,
        Some(
            NatGatewayState::Available
            | NatGatewayState::Deleting
            | NatGatewayState::Failed
            | NatGatewayState::Pending,
        ) => DeletePoll::Pending,
        // The SDK enum is non-exhaustive. An unknown future state does not
        // prove that the gateway is gone.
        Some(_unrecognized_state) => DeletePoll::Pending,
    }
}

impl AwsProvider {
    /// Read an EC2 NAT Gateway
    pub(crate) async fn read_ec2_nat_gateway(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Measured with account carina-test-009 in ap-northeast-1 on
        // 2026-10-24: `aws ec2 describe-nat-gateways --filters
        // Name=nat-gateway-id,Values=<well-formed-unknown>` returned an empty
        // list. Use the filter form so an absent gateway is an ordinary
        // ReadOutcome::NotFound rather than a service error.
        let filter = Filter::builder()
            .name("nat-gateway-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_nat_gateways()
            .filter(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe NAT gateways", e).for_resource(id.clone())
            })?;

        if let Some(ngw) = result.nat_gateways().first() {
            // Skip deleted NAT gateways
            if ngw.state() == Some(&NatGatewayState::Deleted) {
                return Ok(ReadOutcome::not_found(id.clone()));
            }

            let mut attributes = HashMap::new();

            // Extract attributes
            let _identifier_value = Self::extract_ec2_nat_gateway_attributes(ngw, &mut attributes);

            // Extract user-defined tags
            if let Some(tags_value) = Self::ec2_tags_to_value(ngw.tags()) {
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

    /// Create an EC2 NAT Gateway
    pub(crate) async fn create_ec2_nat_gateway_start<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let subnet_id = match require_string_attr(resource, "subnet_id") {
            Ok(subnet_id) => subnet_id,
            Err(error) => return Err(attempt.untracked(error)),
        };

        let mut req = self.ec2_client.create_nat_gateway().configure(|builder| {
            builder.subnet_id(&subnet_id).tag_specifications(
                build_tag_specification_with_create_token(
                    resource,
                    aws_sdk_ec2::types::ResourceType::Natgateway,
                    client_token,
                ),
            )
        });

        if let Some(Value::Concrete(ConcreteValue::String(alloc_id))) =
            resource.get_attr("allocation_id")
        {
            req = req.configure(|builder| builder.allocation_id(alloc_id.expose_plaintext()));
        }

        let connectivity_type = match optional_enum_attr(resource, schema, "connectivity_type") {
            Ok(connectivity_type) => connectivity_type,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(conn_type) = connectivity_type {
            use aws_sdk_ec2::types::ConnectivityType;
            req = req
                .configure(|builder| builder.connectivity_type(ConnectivityType::from(conn_type)));
        }

        let result =
            retry_aws_operation_with_token(client_token, RetryPolicy::default(), || req.clone())
                .await;
        let sent = Self::sent_only(
            resource,
            &["subnet_id", "allocation_id", "connectivity_type", "tags"],
        );

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::NatGateway.create_context(client_token, sent),
            result,
            |output| {
                output
                    .nat_gateway()
                    .and_then(|gateway| gateway.nat_gateway_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    /// Update an EC2 NAT Gateway (tags only)
    pub(crate) async fn update_ec2_nat_gateway(
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
        Ok(self.read_ec2_nat_gateway(&id, identifier).await)
    }

    /// Perform one describe call for a pending NAT gateway create.
    pub(crate) async fn poll_ec2_nat_gateway_create(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<CreatePoll<()>> {
        // Measured with account carina-test-009 in ap-northeast-1 on
        // 2026-10-24: `aws ec2 describe-nat-gateways --nat-gateway-ids
        // <well-formed-unknown>` returned NatGatewayNotFound, while a malformed
        // id returned NatGatewayMalformed. The issued-create caller keeps
        // either error pending: EC2 can briefly hide a just-created gateway,
        // and discarding the record here would risk a duplicate create.
        let result = self
            .ec2_client
            .describe_nat_gateways()
            .nat_gateway_ids(nat_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe NAT gateway", e).for_resource(id.clone())
            })?;
        Ok(nat_gateway_create_poll_state(
            result.nat_gateways().first().and_then(|ngw| ngw.state()),
        ))
    }

    /// Perform one describe call for a pending NAT gateway delete.
    pub(crate) async fn poll_ec2_nat_gateway_delete(
        &self,
        id: &ResourceId,
        nat_gateway_id: &str,
    ) -> ProviderResult<DeletePoll> {
        let result = self
            .ec2_client
            .describe_nat_gateways()
            .nat_gateway_ids(nat_gateway_id)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe NAT gateway", e).for_resource(id.clone())
            })?;
        Ok(nat_gateway_delete_poll_state(
            result.nat_gateways().first().and_then(|ngw| ngw.state()),
        ))
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;

    #[test]
    fn create_poll_maps_pending_ready_and_failed_states() {
        for state in NatGatewayState::values() {
            let expected = match *state {
                "available" => CreatePoll::Ready(()),
                "failed" | "deleted" => CreatePoll::Failed,
                "pending" | "deleting" => CreatePoll::Pending,
                new_known_state => {
                    panic!("classify new SDK NAT gateway state {new_known_state}")
                }
            };
            assert_eq!(
                nat_gateway_create_poll_state(Some(&NatGatewayState::from(*state))),
                expected,
                "state {state}"
            );
        }
        assert_eq!(nat_gateway_create_poll_state(None), CreatePoll::Pending);
        assert_eq!(
            nat_gateway_create_poll_state(Some(&NatGatewayState::from("future-state"))),
            CreatePoll::Pending
        );
    }

    #[test]
    fn delete_poll_maps_pending_and_gone_states() {
        assert_eq!(
            nat_gateway_delete_poll_state(Some(&NatGatewayState::Deleting)),
            DeletePoll::Pending
        );
        assert_eq!(
            nat_gateway_delete_poll_state(Some(&NatGatewayState::Deleted)),
            DeletePoll::Gone
        );
        assert_eq!(nat_gateway_delete_poll_state(None), DeletePoll::Gone);
    }
}
