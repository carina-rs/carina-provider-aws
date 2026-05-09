//! Provider trait implementation for AWS

use carina_core::provider::{
    BoxFuture, CreateRequest, DeleteRequest, Provider, ProviderError, ProviderResult, ReadRequest,
    UpdateRequest,
};
use carina_core::resource::{Resource, ResourceId, State};

use crate::AwsProvider;
use crate::helpers::apply_patch_to_state;
use crate::normalizer::normalize_state_enums;

impl Provider for AwsProvider {
    fn name(&self) -> &str {
        "aws"
    }

    fn read(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
        _request: ReadRequest,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let id = id.clone();
        let identifier = identifier.map(|s| s.to_string());
        Box::pin(async move {
            let mut state = match id.resource_type.as_str() {
                "s3.Bucket" => self.read_s3_bucket(&id, identifier.as_deref()).await,
                "s3.BucketPolicy" => self.read_s3_bucket_policy(&id, identifier.as_deref()).await,
                "s3.BucketPublicAccessBlock" => {
                    self.read_s3_bucket_public_access_block(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketVersioning" => {
                    self.read_s3_bucket_versioning(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketServerSideEncryptionConfiguration" => {
                    self.read_s3_bucket_server_side_encryption_configuration(
                        &id,
                        identifier.as_deref(),
                    )
                    .await
                }
                "s3.BucketAcl" => self.read_s3_bucket_acl(&id, identifier.as_deref()).await,
                "s3.BucketOwnershipControls" => {
                    self.read_s3_bucket_ownership_controls(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketReplicationConfiguration" => {
                    self.read_s3_bucket_replication_configuration(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketLifecycleConfiguration" => {
                    self.read_s3_bucket_lifecycle_configuration(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketWebsiteConfiguration" => {
                    self.read_s3_bucket_website_configuration(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketCorsConfiguration" => {
                    self.read_s3_bucket_cors_configuration(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketNotificationConfiguration" => {
                    self.read_s3_bucket_notification_configuration(&id, identifier.as_deref())
                        .await
                }
                "s3.BucketLogging" => {
                    self.read_s3_bucket_logging(&id, identifier.as_deref())
                        .await
                }
                "ec2.Eip" => self.read_ec2_eip(&id, identifier.as_deref()).await,
                "ec2.Vpc" => self.read_ec2_vpc(&id, identifier.as_deref()).await,
                "ec2.Subnet" => self.read_ec2_subnet(&id, identifier.as_deref()).await,
                "ec2.InternetGateway" => {
                    self.read_ec2_internet_gateway(&id, identifier.as_deref())
                        .await
                }
                "ec2.NatGateway" => self.read_ec2_nat_gateway(&id, identifier.as_deref()).await,
                "ec2.RouteTable" => self.read_ec2_route_table(&id, identifier.as_deref()).await,
                "ec2.Route" => self.read_ec2_route(&id, identifier.as_deref()).await,
                "ec2.SecurityGroup" => {
                    self.read_ec2_security_group(&id, identifier.as_deref())
                        .await
                }
                "ec2.SecurityGroupIngress" => {
                    self.read_ec2_security_group_ingress(&id, identifier.as_deref())
                        .await
                }
                "ec2.SecurityGroupEgress" => {
                    self.read_ec2_security_group_egress(&id, identifier.as_deref())
                        .await
                }
                "ec2.SubnetRouteTableAssociation" => {
                    self.read_ec2_subnet_route_table_association(&id, identifier.as_deref())
                        .await
                }
                "ec2.FlowLog" => self.read_ec2_flow_log(&id, identifier.as_deref()).await,
                "ec2.VpcEndpoint" => self.read_ec2_vpc_endpoint(&id, identifier.as_deref()).await,
                "ec2.VpcGatewayAttachment" => {
                    self.read_ec2_vpc_gateway_attachment(&id, identifier.as_deref())
                        .await
                }
                "ec2.VpnGateway" => self.read_ec2_vpn_gateway(&id, identifier.as_deref()).await,
                "ec2.TransitGateway" => {
                    self.read_ec2_transit_gateway(&id, identifier.as_deref())
                        .await
                }
                "ec2.TransitGatewayAttachment" => {
                    self.read_ec2_transit_gateway_attachment(&id, identifier.as_deref())
                        .await
                }
                "ec2.VpcPeeringConnection" => {
                    self.read_ec2_vpc_peering_connection(&id, identifier.as_deref())
                        .await
                }
                "ec2.EgressOnlyInternetGateway" => {
                    self.read_ec2_egress_only_internet_gateway(&id, identifier.as_deref())
                        .await
                }
                "organizations.Account" => {
                    self.read_organizations_account(&id, identifier.as_deref())
                        .await
                }
                "organizations.Organization" => {
                    self.read_organizations_organization(&id, identifier.as_deref())
                        .await
                }
                "iam.Role" => self.read_iam_role(&id, identifier.as_deref()).await,
                "logs.LogGroup" => self.read_logs_log_group(&id, identifier.as_deref()).await,
                "route53.RecordSet" => {
                    self.read_route53_record_set(&id, identifier.as_deref())
                        .await
                }
                _ => Err(ProviderError::internal(format!(
                    "Unknown resource type: {}",
                    id.resource_type
                ))
                .for_resource(id.clone())),
            }?;

            // Normalize enum values in read state to namespaced DSL format
            if state.exists {
                normalize_state_enums(&id.resource_type, &mut state.attributes);
            }

            Ok(state)
        })
    }

    fn read_data_source(&self, resource: &Resource) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.clone();
        Box::pin(async move {
            let mut state =
                crate::provider_generated::dispatch_read_data_source(self, &resource).await?;
            if state.exists {
                normalize_state_enums(&resource.id.resource_type, &mut state.attributes);
            }
            Ok(state)
        })
    }

    fn create(
        &self,
        _id: &ResourceId,
        request: CreateRequest,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = request.resource;
        Box::pin(async move {
            match resource.id.resource_type.as_str() {
                "s3.Bucket" => self.create_s3_bucket(resource).await,
                "s3.BucketPolicy" => self.create_s3_bucket_policy(resource).await,
                "s3.BucketPublicAccessBlock" => {
                    self.create_s3_bucket_public_access_block(resource).await
                }
                "s3.BucketVersioning" => self.create_s3_bucket_versioning(resource).await,
                "s3.BucketServerSideEncryptionConfiguration" => {
                    self.create_s3_bucket_server_side_encryption_configuration(resource)
                        .await
                }
                "s3.BucketAcl" => self.create_s3_bucket_acl(resource).await,
                "s3.BucketOwnershipControls" => {
                    self.create_s3_bucket_ownership_controls(resource).await
                }
                "s3.BucketReplicationConfiguration" => {
                    self.create_s3_bucket_replication_configuration(resource)
                        .await
                }
                "s3.BucketLifecycleConfiguration" => {
                    self.create_s3_bucket_lifecycle_configuration(resource)
                        .await
                }
                "s3.BucketWebsiteConfiguration" => {
                    self.create_s3_bucket_website_configuration(resource).await
                }
                "s3.BucketCorsConfiguration" => {
                    self.create_s3_bucket_cors_configuration(resource).await
                }
                "s3.BucketNotificationConfiguration" => {
                    self.create_s3_bucket_notification_configuration(resource)
                        .await
                }
                "s3.BucketLogging" => self.create_s3_bucket_logging(resource).await,
                "ec2.Eip" => self.create_ec2_eip(resource).await,
                "ec2.Vpc" => self.create_ec2_vpc(resource).await,
                "ec2.Subnet" => self.create_ec2_subnet(resource).await,
                "ec2.InternetGateway" => self.create_ec2_internet_gateway(resource).await,
                "ec2.NatGateway" => self.create_ec2_nat_gateway(resource).await,
                "ec2.RouteTable" => self.create_ec2_route_table(resource).await,
                "ec2.Route" => self.create_ec2_route(resource).await,
                "ec2.SecurityGroup" => self.create_ec2_security_group(resource).await,
                "ec2.SecurityGroupIngress" => {
                    self.create_ec2_security_group_ingress(resource).await
                }
                "ec2.SecurityGroupEgress" => self.create_ec2_security_group_egress(resource).await,
                "ec2.SubnetRouteTableAssociation" => {
                    self.create_ec2_subnet_route_table_association(resource)
                        .await
                }
                "ec2.FlowLog" => self.create_ec2_flow_log(resource).await,
                "ec2.VpcEndpoint" => self.create_ec2_vpc_endpoint(resource).await,
                "ec2.VpcGatewayAttachment" => {
                    self.create_ec2_vpc_gateway_attachment(resource).await
                }
                "ec2.VpnGateway" => self.create_ec2_vpn_gateway(resource).await,
                "ec2.TransitGateway" => self.create_ec2_transit_gateway(resource).await,
                "ec2.TransitGatewayAttachment" => {
                    self.create_ec2_transit_gateway_attachment(resource).await
                }
                "ec2.VpcPeeringConnection" => {
                    self.create_ec2_vpc_peering_connection(resource).await
                }
                "ec2.EgressOnlyInternetGateway" => {
                    self.create_ec2_egress_only_internet_gateway(resource).await
                }
                "organizations.Account" => self.create_organizations_account(resource).await,
                "organizations.Organization" => {
                    self.create_organizations_organization(resource).await
                }
                "iam.Role" => self.create_iam_role(resource).await,
                "logs.LogGroup" => self.create_logs_log_group(resource).await,
                "route53.RecordSet" => self.create_route53_record_set(resource).await,
                _ => Err(ProviderError::internal(format!(
                    "Unknown resource type: {}",
                    resource.id.resource_type
                ))
                .for_resource(resource.id.clone())),
            }
        })
    }

    fn update(
        &self,
        id: &ResourceId,
        identifier: &str,
        request: UpdateRequest,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let id = id.clone();
        let identifier = identifier.to_string();
        let from = request.from.clone();
        // The aws provider's per-resource `update_*` methods predate the
        // Level 3 patch contract and accept a full `to: Resource`.
        // Reconstruct that from `(from, patch)` here so each method's
        // existing logic continues to work without per-resource churn.
        // Future per-resource updates can read `request.patch` directly
        // for partial-update API paths once those are wired in.
        let to = apply_patch_to_state(&request.from, &request.patch);
        Box::pin(async move {
            match id.resource_type.as_str() {
                "s3.Bucket" => self.update_s3_bucket(id, &identifier, &from, to).await,
                "s3.BucketPolicy" => {
                    self.update_s3_bucket_policy(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketPublicAccessBlock" => {
                    self.update_s3_bucket_public_access_block(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketVersioning" => {
                    self.update_s3_bucket_versioning(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketServerSideEncryptionConfiguration" => {
                    self.update_s3_bucket_server_side_encryption_configuration(
                        id,
                        &identifier,
                        &from,
                        to,
                    )
                    .await
                }
                "s3.BucketAcl" => self.update_s3_bucket_acl(id, &identifier, &from, to).await,
                "s3.BucketOwnershipControls" => {
                    self.update_s3_bucket_ownership_controls(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketReplicationConfiguration" => {
                    self.update_s3_bucket_replication_configuration(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketLifecycleConfiguration" => {
                    self.update_s3_bucket_lifecycle_configuration(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketWebsiteConfiguration" => {
                    self.update_s3_bucket_website_configuration(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketCorsConfiguration" => {
                    self.update_s3_bucket_cors_configuration(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketNotificationConfiguration" => {
                    self.update_s3_bucket_notification_configuration(id, &identifier, &from, to)
                        .await
                }
                "s3.BucketLogging" => {
                    self.update_s3_bucket_logging(id, &identifier, &from, to)
                        .await
                }
                "ec2.Eip" => self.update_ec2_eip(id, &identifier, &from, to).await,
                "ec2.Vpc" => self.update_ec2_vpc(id, &identifier, &from, to).await,
                "ec2.Subnet" => self.update_ec2_subnet(id, &identifier, &from, to).await,
                "ec2.InternetGateway" => {
                    self.update_ec2_internet_gateway(id, &identifier, &from, to)
                        .await
                }
                "ec2.NatGateway" => {
                    self.update_ec2_nat_gateway(id, &identifier, &from, to)
                        .await
                }
                "ec2.RouteTable" => {
                    self.update_ec2_route_table(id, &identifier, &from, to)
                        .await
                }
                "ec2.Route" => self.update_ec2_route(id, &identifier, to).await,
                "ec2.SecurityGroup" => {
                    self.update_ec2_security_group(id, &identifier, &from, to)
                        .await
                }
                "ec2.SecurityGroupIngress" => {
                    self.update_ec2_security_group_ingress(id, &identifier, to)
                        .await
                }
                "ec2.SecurityGroupEgress" => {
                    self.update_ec2_security_group_egress(id, &identifier, to)
                        .await
                }
                "ec2.SubnetRouteTableAssociation" => {
                    self.update_ec2_subnet_route_table_association(id, &identifier, to)
                        .await
                }
                "ec2.FlowLog" => self.update_ec2_flow_log(id, &identifier, &from, to).await,
                "ec2.VpcEndpoint" => {
                    self.update_ec2_vpc_endpoint(id, &identifier, &from, to)
                        .await
                }
                "ec2.VpcGatewayAttachment" => {
                    self.update_ec2_vpc_gateway_attachment(id, &identifier)
                        .await
                }
                "ec2.VpnGateway" => {
                    self.update_ec2_vpn_gateway(id, &identifier, &from, to)
                        .await
                }
                "ec2.TransitGateway" => {
                    self.update_ec2_transit_gateway(id, &identifier, &from, to)
                        .await
                }
                "ec2.TransitGatewayAttachment" => {
                    self.update_ec2_transit_gateway_attachment(id, &identifier, &from, to)
                        .await
                }
                "ec2.VpcPeeringConnection" => {
                    self.update_ec2_vpc_peering_connection(id, &identifier, &from, to)
                        .await
                }
                "ec2.EgressOnlyInternetGateway" => {
                    self.update_ec2_egress_only_internet_gateway(id, &identifier, &from, to)
                        .await
                }
                "organizations.Account" => {
                    self.update_organizations_account(id, &identifier, &from, to)
                        .await
                }
                "organizations.Organization" => {
                    // All attributes are read-only or create-only; read back current state
                    self.read_organizations_organization(&id, Some(&identifier))
                        .await
                }
                "iam.Role" => self.update_iam_role(id, &identifier, &from, to).await,
                "logs.LogGroup" => self.update_logs_log_group(id, &identifier, &from, to).await,
                "route53.RecordSet" => self.update_route53_record_set(id, &identifier, to).await,
                _ => Err(ProviderError::internal(format!(
                    "Unknown resource type: {}",
                    id.resource_type
                ))
                .for_resource(id.clone())),
            }
        })
    }

    fn delete(
        &self,
        id: &ResourceId,
        identifier: &str,
        request: DeleteRequest,
    ) -> BoxFuture<'_, ProviderResult<()>> {
        let id = id.clone();
        let identifier = identifier.to_string();
        let directives = request.directives;
        Box::pin(async move {
            match id.resource_type.as_str() {
                "s3.Bucket" => self.delete_s3_bucket(id, &identifier, &directives).await,
                "s3.BucketPolicy" => {
                    self.delete_s3_bucket_policy_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketPublicAccessBlock" => {
                    self.delete_s3_bucket_public_access_block_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketVersioning" => {
                    self.delete_s3_bucket_versioning_suspend(id, &identifier)
                        .await
                }
                "s3.BucketServerSideEncryptionConfiguration" => {
                    self.delete_s3_bucket_server_side_encryption_configuration_idempotent(
                        id,
                        &identifier,
                    )
                    .await
                }
                "s3.BucketAcl" => self.delete_s3_bucket_acl_reset(id, &identifier).await,
                "s3.BucketOwnershipControls" => {
                    self.delete_s3_bucket_ownership_controls_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketReplicationConfiguration" => {
                    self.delete_s3_bucket_replication_configuration_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketLifecycleConfiguration" => {
                    self.delete_s3_bucket_lifecycle_configuration_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketWebsiteConfiguration" => {
                    self.delete_s3_bucket_website_configuration_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketCorsConfiguration" => {
                    self.delete_s3_bucket_cors_configuration_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketNotificationConfiguration" => {
                    self.delete_s3_bucket_notification_configuration_idempotent(id, &identifier)
                        .await
                }
                "s3.BucketLogging" => {
                    self.delete_s3_bucket_logging_idempotent(id, &identifier)
                        .await
                }
                "ec2.Eip" => self.delete_ec2_eip(id, &identifier).await,
                "ec2.Vpc" => self.delete_ec2_vpc(id, &identifier).await,
                "ec2.Subnet" => self.delete_ec2_subnet(id, &identifier).await,
                "ec2.InternetGateway" => self.delete_ec2_internet_gateway(id, &identifier).await,
                "ec2.NatGateway" => self.delete_ec2_nat_gateway(id, &identifier).await,
                "ec2.RouteTable" => self.delete_ec2_route_table(id, &identifier).await,
                "ec2.Route" => self.delete_ec2_route(id, &identifier).await,
                "ec2.SecurityGroup" => self.delete_ec2_security_group(id, &identifier).await,
                "ec2.SecurityGroupIngress" => {
                    self.delete_ec2_security_group_ingress(id, &identifier)
                        .await
                }
                "ec2.SecurityGroupEgress" => {
                    self.delete_ec2_security_group_egress(id, &identifier).await
                }
                "ec2.SubnetRouteTableAssociation" => {
                    self.delete_ec2_subnet_route_table_association(id, &identifier)
                        .await
                }
                "ec2.FlowLog" => self.delete_ec2_flow_log(id, &identifier).await,
                "ec2.VpcEndpoint" => self.delete_ec2_vpc_endpoint(id, &identifier).await,
                "ec2.VpcGatewayAttachment" => {
                    self.delete_ec2_vpc_gateway_attachment(id, &identifier)
                        .await
                }
                "ec2.VpnGateway" => self.delete_ec2_vpn_gateway(id, &identifier).await,
                "ec2.TransitGateway" => self.delete_ec2_transit_gateway(id, &identifier).await,
                "ec2.TransitGatewayAttachment" => {
                    self.delete_ec2_transit_gateway_attachment(id, &identifier)
                        .await
                }
                "ec2.VpcPeeringConnection" => {
                    self.delete_ec2_vpc_peering_connection(id, &identifier)
                        .await
                }
                "ec2.EgressOnlyInternetGateway" => {
                    self.delete_ec2_egress_only_internet_gateway(id, &identifier)
                        .await
                }
                "organizations.Account" => self.delete_organizations_account(id, &identifier).await,
                "organizations.Organization" => {
                    self.delete_organizations_organization(id, &identifier)
                        .await
                }
                "iam.Role" => self.delete_iam_role(id, &identifier).await,
                "logs.LogGroup" => self.delete_logs_log_group(id, &identifier).await,
                "route53.RecordSet" => self.delete_route53_record_set(id, &identifier).await,
                _ => Err(ProviderError::internal(format!(
                    "Unknown resource type: {}",
                    id.resource_type
                ))
                .for_resource(id.clone())),
            }
        })
    }
}
