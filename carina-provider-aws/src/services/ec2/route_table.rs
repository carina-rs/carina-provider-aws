use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{build_tag_specification_with_create_token, send_aws_operation_with_token};
use crate::provider::{AwsCreateAttempt, Ec2TaggedResourceKind};
use crate::read_model::{empty_map_value, found_managed_read};
use crate::read_models_generated::Ec2RouteTableReadModelSupplement;
use crate::send_models_generated::Ec2RouteTableCreateInput;

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
        let input = match Ec2RouteTableCreateInput::decode(resource) {
            Ok(input) => input,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let sent = input.sent();
        let (tags, vpc_id) = input.into_parts();

        // Create Route Table
        let req = self.ec2_client.create_route_table().configure(|builder| {
            builder
                .vpc_id(&vpc_id)
                .tag_specifications(build_tag_specification_with_create_token(
                    tags.as_ref(),
                    aws_sdk_ec2::types::ResourceType::RouteTable,
                    client_token,
                ))
        });
        let result = send_aws_operation_with_token(client_token, req).await;
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
}
