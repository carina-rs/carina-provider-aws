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
    retry_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, TaggedEc2Create};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2NatGatewayReadModelSupplement;
use crate::send_models_generated::{Ec2NatGatewayCreateInput, Ec2NatGatewayUpdateInput};

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
            Ok(AvailabilityZoneAddress::builder()
                .set_availability_zone(availability_zone)
                .set_allocation_ids(Some(allocation_ids))
                .build())
        })
        .collect()
}

fn availability_zone_addresses_to_value(
    addresses: &[aws_sdk_ec2::types::NatGatewayAddress],
) -> Value {
    let mut grouped: indexmap::IndexMap<Option<String>, Vec<String>> = indexmap::IndexMap::new();
    for address in addresses {
        // EC2 returns both the AZ name and ID for regional NAT addresses, but
        // create accepts either selector, not both. The schema exposes the
        // name as the sole canonical selector.
        let key = address.availability_zone().map(str::to_string);
        let allocation_ids = grouped.entry(key).or_default();
        if let Some(allocation_id) = address.allocation_id() {
            allocation_ids.push(allocation_id.to_string());
        }
    }
    Value::Concrete(ConcreteValue::List(
        grouped
            .into_iter()
            .filter_map(|(availability_zone, allocation_ids)| {
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
                if fields.is_empty() {
                    None
                } else {
                    Some(Value::Concrete(ConcreteValue::Map(fields)))
                }
            })
            .collect(),
    ))
}

fn concrete_string(value: &Value) -> Option<&str> {
    match value {
        Value::Concrete(ConcreteValue::String(value)) => Some(value.expose_plaintext()),
        Value::Concrete(ConcreteValue::CanonicalEnum(value)) => Some(value.api_value()),
        _ => None,
    }
}

/// Validate the mode-dependent placement inputs while unresolved references
/// are still allowed to defer the decision until apply-time resolution.
pub(crate) fn validate_nat_gateway_inputs<S>(resource: &Resource<S>) -> ProviderResult<()> {
    if resource.id.provider != "aws" || resource.id.resource_type != "ec2.NatGateway" {
        return Ok(());
    }

    let mode = match resource.get_attr("availability_mode") {
        None => Some("zonal"),
        Some(value) => concrete_string(value),
    };
    let Some(mode) = mode else {
        return Ok(());
    };
    let subnet = resource.get_attr("subnet_id");
    match mode {
        "regional" if subnet.is_some_and(|value| concrete_string(value).is_some()) => {
            Err(ProviderError::invalid_input(
                "ec2.NatGateway subnet_id must be omitted when availability_mode = regional",
            )
            .for_resource(resource.id.clone()))
        }
        "zonal" if subnet.is_none() => Err(ProviderError::invalid_input(
            "ec2.NatGateway subnet_id is required when availability_mode = zonal",
        )
        .for_resource(resource.id.clone())),
        _ => Ok(()),
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

            let fields = Self::extract_ec2_nat_gateway_attributes(ngw);

            let availability_zone_addresses =
                availability_zone_addresses_to_value(ngw.nat_gateway_addresses());
            let private_ip_address = ngw
                .nat_gateway_addresses()
                .iter()
                .find(|address| address.is_primary().unwrap_or(false))
                .or_else(|| ngw.nat_gateway_addresses().first())
                .and_then(|address| address.private_ip())
                .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
            let (_, model) = fields.into_model(
                id,
                Ec2NatGatewayReadModelSupplement {
                    availability_zone_addresses,
                    private_ip_address,
                    tags: Self::ec2_tags_to_value(ngw.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 NAT Gateway
    pub(crate) async fn create_ec2_nat_gateway_start<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        if let Err(error) = validate_nat_gateway_inputs(resource) {
            return Err(attempt.untracked(error));
        }
        let input = match Ec2NatGatewayCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let crate::send_models_generated::Ec2NatGatewayCreateParts {
            allocation_id,
            availability_mode,
            availability_zone_addresses,
            connectivity_type,
            private_ip_address,
            subnet_id,
            tags,
            vpc_id,
        } = input.into_parts();
        let allocation_id = allocation_id.into_inner();
        let availability_mode = availability_mode.into_inner();
        let availability_zone_addresses = availability_zone_addresses.into_inner();
        let connectivity_type = connectivity_type.into_inner();
        let private_ip_address = private_ip_address.into_inner();
        let subnet_id = subnet_id.into_inner();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        let vpc_id = vpc_id.into_inner();

        let mut req = self.ec2_client.create_nat_gateway().configure(|builder| {
            builder.tag_specifications(build_tag_specification_with_create_token(
                tags.as_ref(),
                aws_sdk_ec2::types::ResourceType::Natgateway,
                client_token,
            ))
        });
        if let Some(subnet_id) = subnet_id {
            req = req.configure(|builder| builder.subnet_id(subnet_id));
        }

        if let Some(allocation_id) = allocation_id {
            req = req.configure(|builder| builder.allocation_id(allocation_id));
        }

        if let Some(conn_type) = connectivity_type {
            use aws_sdk_ec2::types::ConnectivityType;
            req = req.configure(|builder| {
                builder.connectivity_type(ConnectivityType::from(conn_type.as_str()))
            });
        }

        if let Some(availability_mode) = availability_mode {
            req = req.configure(|builder| {
                builder.availability_mode(aws_sdk_ec2::types::AvailabilityMode::from(
                    availability_mode.as_str(),
                ))
            });
        }
        if let Some(vpc_id) = vpc_id {
            req = req.configure(|builder| builder.vpc_id(vpc_id));
        }
        if let Some(private_ip_address) = private_ip_address {
            req = req.configure(|builder| builder.private_ip_address(private_ip_address));
        }
        let availability_zone_addresses = match build_availability_zone_addresses(
            &resource.id,
            availability_zone_addresses.as_ref(),
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
        self.finish_tagged_ec2_create(
            resource,
            attempt,
            TaggedEc2Create::NatGateway.create_context(client_token, sent),
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
        let input = Ec2NatGatewayUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2NatGatewayUpdateParts { tags } = input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            tags.as_ref(),
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
    use carina_core::differ::{Diff, diff};
    use carina_core::provider::RawSavedAttrs;
    use carina_core::resource::{ResolvedResource, Resource};
    use carina_core::schema::SchemaRegistry;
    use std::collections::HashMap;

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

    #[test]
    fn zonal_and_regional_modes_enforce_subnet_placement() {
        let mut zonal = Resource::with_provider("aws", "ec2.NatGateway", "zonal", None);
        zonal.set_attr(
            "availability_mode",
            Value::Concrete(ConcreteValue::String("zonal".into())),
        );
        assert!(validate_nat_gateway_inputs(&zonal).is_err());
        zonal.set_attr(
            "subnet_id",
            Value::Concrete(ConcreteValue::String("subnet-12345678".into())),
        );
        validate_nat_gateway_inputs(&zonal).expect("zonal mode accepts a subnet");

        let mut regional = Resource::with_provider("aws", "ec2.NatGateway", "regional", None);
        regional.set_attr(
            "availability_mode",
            Value::Concrete(ConcreteValue::String("regional".into())),
        );
        validate_nat_gateway_inputs(&regional).expect("regional mode omits subnet_id");
        regional.set_attr(
            "subnet_id",
            Value::Concrete(ConcreteValue::String("subnet-12345678".into())),
        );
        assert!(validate_nat_gateway_inputs(&regional).is_err());
    }

    #[test]
    fn regional_addresses_read_one_canonical_zone_selector_without_diff() {
        let address = aws_sdk_ec2::types::NatGatewayAddress::builder()
            .allocation_id("eipalloc-12345678")
            .availability_zone("ap-northeast-1a")
            .availability_zone_id("apne1-az4")
            .build();
        let observed = availability_zone_addresses_to_value(&[address]);
        let Value::Concrete(ConcreteValue::List(items)) = &observed else {
            panic!("availability-zone addresses must be a list");
        };
        let Value::Concrete(ConcreteValue::Map(fields)) = &items[0] else {
            panic!("availability-zone address must be a map");
        };
        assert_eq!(
            fields.get("availability_zone"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ap-northeast-1a".into()
            )))
        );
        assert!(!fields.contains_key("availability_zone_id"));

        let mut desired = Resource::with_provider("aws", "ec2.NatGateway", "regional", None);
        desired.set_attr(
            "availability_mode",
            Value::Concrete(ConcreteValue::String("regional".into())),
        );
        desired.set_attr(
            "vpc_id",
            Value::Concrete(ConcreteValue::String("vpc-12345678".into())),
        );
        desired.set_attr("availability_zone_addresses", observed.clone());
        let current = crate::read_model::test_existing_state(
            desired.id.clone(),
            HashMap::from([
                (
                    "availability_mode".to_string(),
                    Value::Concrete(ConcreteValue::String("regional".into())),
                ),
                (
                    "vpc_id".to_string(),
                    Value::Concrete(ConcreteValue::String("vpc-12345678".into())),
                ),
                ("availability_zone_addresses".to_string(), observed),
            ]),
        )
        .with_identifier("nat-12345678");
        let registry = SchemaRegistry::new();
        let saved = RawSavedAttrs::default().lift(&registry);
        let schema = crate::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema;
        let result = diff(
            &ResolvedResource::new(desired),
            &current,
            &saved,
            None,
            Some(&schema),
        );
        assert!(matches!(result, Diff::NoChange(_)), "{result:?}");
    }
}
