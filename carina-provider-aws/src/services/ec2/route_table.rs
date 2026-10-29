use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    build_tag_specification_with_create_token, require_string_attr, send_aws_operation_with_token,
};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2RouteTableReadModelSupplement;

impl AwsProvider {
    /// Read an EC2 Route Table
    pub(crate) async fn read_ec2_route_table(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        use aws_sdk_ec2::types::Filter;

        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        let filter = Filter::builder()
            .name("route-table-id")
            .values(identifier)
            .build();

        let result = self
            .ec2_client
            .describe_route_tables()
            .filters(filter)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to describe route tables", e).for_resource(id.clone())
            })?;

        if let Some(rt) = result.route_tables().first() {
            let fields = Self::extract_ec2_route_table_attributes(rt);

            let (_, model) = fields.into_model(
                id,
                Ec2RouteTableReadModelSupplement {
                    tags: Self::ec2_tags_to_value(rt.tags()).unwrap_or_else(empty_map_value),
                },
            )?;

            found_managed_read(id, applied_identifier, model)
        } else {
            Ok(ReadOutcome::not_found(id.clone()))
        }
    }

    /// Create an EC2 Route Table
    pub(crate) async fn create_ec2_route_table<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        client_token: &ClientToken,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let vpc_id = match require_string_attr(resource, "vpc_id") {
            Ok(vpc_id) => vpc_id,
            Err(error) => return Err(attempt.untracked(error)),
        };

        // Create Route Table
        let req = self.ec2_client.create_route_table().configure(|builder| {
            builder
                .vpc_id(&vpc_id)
                .tag_specifications(build_tag_specification_with_create_token(
                    resource,
                    aws_sdk_ec2::types::ResourceType::RouteTable,
                    client_token,
                ))
        });
        let result = send_aws_operation_with_token(client_token, req).await;
        let sent = Self::sent_only(resource, &["vpc_id", "tags"]);

        self.finish_tagged_ec2_create(
            resource,
            attempt,
            Ec2TaggedResourceKind::RouteTable.create_context(client_token, sent),
            result,
            |output| {
                output
                    .route_table()
                    .and_then(|table| table.route_table_id())
                    .map(str::to_owned)
            },
        )
        .await
    }

    pub(crate) async fn continue_ec2_route_table_create(
        &self,
        resource: &Resource<impl Sized>,
        rt_id: &str,
    ) -> ProviderResult<()> {
        // Add routes
        if let Some(Value::Concrete(ConcreteValue::List(routes))) = resource.get_attr("routes") {
            for route in routes {
                if let Value::Concrete(ConcreteValue::Map(route_map)) = route {
                    let destination = route_map.get("destination").and_then(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s)
                        } else {
                            None
                        }
                    });
                    let gateway_id = route_map.get("gateway_id").and_then(|v| {
                        if let Value::Concrete(ConcreteValue::String(s)) = v {
                            Some(s)
                        } else {
                            None
                        }
                    });

                    if let (Some(dest), Some(gw_id)) = (destination, gateway_id) {
                        let result = self
                            .ec2_client
                            .create_route()
                            .route_table_id(rt_id)
                            .destination_cidr_block(dest.expose_plaintext())
                            .gateway_id(gw_id.expose_plaintext())
                            .send()
                            .await;
                        if let Err(error) = result {
                            let error = api_error_with_meta("Failed to create route", error)
                                .for_resource(resource.id.clone());
                            return Err(error);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
