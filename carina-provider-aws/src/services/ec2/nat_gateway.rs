use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use aws_sdk_ec2::types::{AvailabilityZoneAddress, NatGatewayState};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    CreatePoll, DeletePoll, RetryPolicy, build_tag_specification_with_create_token,
    optional_enum_attr, optional_string_attr, require_string_attr, retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::provider_generated::Ec2NatGatewayReadFields;
use crate::read_model::{empty_map_value, found_managed_read, require_read_value};
use crate::read_models_generated::Ec2NatGatewayReadModel;

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

fn build_availability_zone_addresses(
    id: &ResourceId,
    value: Option<&Value>,
) -> ProviderResult<Vec<AvailabilityZoneAddress>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Concrete(ConcreteValue::List(items)) = value else {
        return Err(
            ProviderError::invalid_input("availability_zone_addresses must be a list")
                .for_resource(id.clone()),
        );
    };

    items
        .iter()
        .map(|item| {
            let Value::Concrete(ConcreteValue::Map(fields)) = item else {
                return Err(ProviderError::invalid_input(
                    "each availability_zone_addresses item must be a map",
                )
                .for_resource(id.clone()));
            };
            let string_field = |name: &str| -> ProviderResult<Option<String>> {
                match fields.get(name) {
                    Some(Value::Concrete(ConcreteValue::String(value))) => {
                        Ok(Some(value.expose_plaintext().to_string()))
                    }
                    Some(Value::Concrete(ConcreteValue::CanonicalEnum(value))) => {
                        Ok(Some(value.api_value().to_string()))
                    }
                    None => Ok(None),
                    Some(_) => Err(ProviderError::invalid_input(format!(
                        "availability_zone_addresses.{name} must be a string"
                    ))
                    .for_resource(id.clone())),
                }
            };
            let allocation_ids = match fields.get("allocation_ids") {
                Some(Value::Concrete(ConcreteValue::List(values))) => values
                    .iter()
                    .map(|value| match value {
                        Value::Concrete(ConcreteValue::String(value)) => {
                            Ok(value.expose_plaintext().to_string())
                        }
                        _ => Err(ProviderError::invalid_input(
                            "availability_zone_addresses.allocation_ids must contain strings",
                        )
                        .for_resource(id.clone())),
                    })
                    .collect::<ProviderResult<Vec<_>>>()?,
                Some(Value::Concrete(ConcreteValue::StringList(values))) => values
                    .iter()
                    .map(|value| value.expose_plaintext().to_string())
                    .collect(),
                None => Vec::new(),
                Some(_) => {
                    return Err(ProviderError::invalid_input(
                        "availability_zone_addresses.allocation_ids must be a list",
                    )
                    .for_resource(id.clone()));
                }
            };
            let availability_zone = string_field("availability_zone")?;
            let availability_zone_id = string_field("availability_zone_id")?;
            if availability_zone.is_some() && availability_zone_id.is_some() {
                return Err(ProviderError::invalid_input(
                    "availability_zone_addresses.availability_zone and availability_zone_id are mutually exclusive",
                )
                .for_resource(id.clone()));
            }
            Ok(AvailabilityZoneAddress::builder()
                .set_availability_zone(availability_zone)
                .set_availability_zone_id(availability_zone_id)
                .set_allocation_ids(Some(allocation_ids))
                .build())
        })
        .collect()
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

            let Ec2NatGatewayReadFields {
                allocation_id,
                availability_mode,
                connectivity_type,
                nat_gateway_id,
                subnet_id,
                vpc_id,
                applied_identifier: _,
            } = Self::extract_ec2_nat_gateway_attributes(ngw);

            let mut grouped_addresses: indexmap::IndexMap<
                (Option<String>, Option<String>),
                Vec<String>,
            > = indexmap::IndexMap::new();
            for address in ngw.nat_gateway_addresses() {
                let key = (
                    address.availability_zone().map(str::to_string),
                    address.availability_zone_id().map(str::to_string),
                );
                let allocation_ids = grouped_addresses.entry(key).or_default();
                if let Some(allocation_id) = address.allocation_id() {
                    allocation_ids.push(allocation_id.to_string());
                }
            }
            let mut availability_zone_addresses = Vec::new();
            for ((availability_zone, availability_zone_id), allocation_ids) in grouped_addresses {
                let mut fields = indexmap::IndexMap::new();
                if !allocation_ids.is_empty() {
                    fields.insert(
                        "allocation_ids".to_string(),
                        Value::Concrete(ConcreteValue::List(
                            allocation_ids
                                .into_iter()
                                .map(|allocation_id| {
                                    Value::Concrete(ConcreteValue::String(allocation_id.into()))
                                })
                                .collect(),
                        )),
                    );
                }
                if let Some(availability_zone) = availability_zone {
                    fields.insert(
                        "availability_zone".to_string(),
                        Value::Concrete(ConcreteValue::String(availability_zone.into())),
                    );
                }
                if let Some(availability_zone_id) = availability_zone_id {
                    fields.insert(
                        "availability_zone_id".to_string(),
                        Value::Concrete(ConcreteValue::String(availability_zone_id.into())),
                    );
                }
                if !fields.is_empty() {
                    availability_zone_addresses.push(Value::Concrete(ConcreteValue::Map(fields)));
                }
            }
            let private_ip_address = ngw
                .nat_gateway_addresses()
                .iter()
                .find(|address| address.is_primary().unwrap_or(false))
                .or_else(|| ngw.nat_gateway_addresses().first())
                .and_then(|address| address.private_ip())
                .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
            let model = Ec2NatGatewayReadModel {
                allocation_id,
                availability_mode,
                availability_zone_addresses: Value::Concrete(ConcreteValue::List(
                    availability_zone_addresses,
                )),
                connectivity_type,
                private_ip_address,
                subnet_id: require_read_value(id, subnet_id, "subnet_id")?,
                vpc_id,
                nat_gateway_id,
                tags: Self::ec2_tags_to_value(ngw.tags()).unwrap_or_else(empty_map_value),
            };

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 NAT Gateway
    pub(crate) async fn create_ec2_nat_gateway_start<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
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

        let availability_mode = match optional_enum_attr(resource, schema, "availability_mode") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(availability_mode) = availability_mode {
            req = req.configure(|builder| {
                builder.availability_mode(aws_sdk_ec2::types::AvailabilityMode::from(
                    availability_mode,
                ))
            });
        }
        let vpc_id = match optional_string_attr(resource, "vpc_id") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(vpc_id) = vpc_id {
            req = req.configure(|builder| builder.vpc_id(vpc_id));
        }
        let private_ip_address = match optional_string_attr(resource, "private_ip_address") {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if let Some(private_ip_address) = private_ip_address {
            req = req.configure(|builder| builder.private_ip_address(private_ip_address));
        }
        let availability_zone_addresses = match build_availability_zone_addresses(
            &resource.id,
            resource.get_attr("availability_zone_addresses"),
        ) {
            Ok(value) => value,
            Err(error) => return Err(attempt.untracked(error)),
        };
        if !availability_zone_addresses.is_empty() {
            req = req.configure(|builder| {
                builder.set_availability_zone_addresses(Some(availability_zone_addresses.clone()))
            });
        }

        let result =
            retry_aws_operation_with_token(client_token, RetryPolicy::default(), || req.clone())
                .await;
        let sent = Self::sent_only(
            resource,
            &[
                "subnet_id",
                "allocation_id",
                "availability_mode",
                "availability_zone_addresses",
                "connectivity_type",
                "private_ip_address",
                "vpc_id",
                "tags",
            ],
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
