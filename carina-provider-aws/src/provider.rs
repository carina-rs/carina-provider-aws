//! Provider trait implementation for AWS

use std::future::Future;
use std::time::Duration;

use carina_core::effect::PlanOp;
use carina_core::provider::{
    AppliedIdentifier, AppliedState, BoxFuture, CreateOutcome, CreateRequest, DeleteRequest,
    Provider, ProviderError, ProviderReadyDataSource, ProviderResult, ReadOutcome, ReadRequest,
    UpdateOutcome, UpdatePatch, UpdateRequest,
};
use carina_core::resource::{DataSource, Resource, ResourceId, State};
use carina_provider_protocol::{
    DEFAULT_OPERATION_BUDGET, DriveOutcome, OperationKind, OperationStep, drive_pending,
};
use serde::{Deserialize, Serialize};

use crate::AwsProvider;
use crate::helpers::{CreatePoll, DeletePoll, apply_patch_to_state};

const POLL_AFTER_SECS: u32 = 5;

/// Continuation phases for AWS create operations that wait on cloud state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AwsCreateToken {
    NatGateway {
        nat_gateway_id: AppliedIdentifier,
    },
    TransitGateway {
        transit_gateway_id: AppliedIdentifier,
    },
    TransitGatewayAttachment {
        attachment_id: AppliedIdentifier,
    },
    OrganizationsAccount {
        create_account_request_id: String,
    },
}

/// Continuation phases for AWS delete operations that wait on cloud state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AwsDeleteToken {
    NatGateway {
        nat_gateway_id: AppliedIdentifier,
    },
    TransitGateway {
        transit_gateway_id: AppliedIdentifier,
    },
    TransitGatewayAttachment {
        attachment_id: AppliedIdentifier,
    },
}

pub type AwsCreateStep = OperationStep<CreateOutcome, AwsCreateToken>;
pub type AwsDeleteStep = OperationStep<(), AwsDeleteToken>;

fn create_pending(token: AwsCreateToken) -> AwsCreateStep {
    OperationStep::Pending {
        pending: token,
        poll_after_secs: POLL_AFTER_SECS,
    }
}

fn create_done(outcome: CreateOutcome) -> AwsCreateStep {
    OperationStep::Done(outcome)
}

fn delete_pending(token: AwsDeleteToken) -> AwsDeleteStep {
    OperationStep::Pending {
        pending: token,
        poll_after_secs: POLL_AFTER_SECS,
    }
}

fn delete_done(_: ()) -> AwsDeleteStep {
    OperationStep::Done(())
}

async fn drive_operation<T, Tok, Initial, Poll, PollFuture, Sleep, SleepFuture>(
    id: &ResourceId,
    kind: OperationKind,
    budget: Duration,
    initial: Initial,
    poll: Poll,
    sleep: Sleep,
) -> ProviderResult<T>
where
    Initial: Future<Output = ProviderResult<OperationStep<T, Tok>>>,
    Poll: FnMut(Tok) -> PollFuture,
    PollFuture: Future<Output = ProviderResult<OperationStep<T, Tok>>>,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    match drive_pending(budget, initial, poll, sleep).await? {
        DriveOutcome::Done(value) => Ok(value),
        DriveOutcome::OverBudget => {
            let kind = match kind {
                OperationKind::Create => "create",
                OperationKind::Update => "update",
                OperationKind::Delete => "delete",
            };
            Err(ProviderError::timeout(format!(
                "{kind} of {} did not finish within {budget:?}",
                id.resource_type
            ))
            .for_resource(id.clone()))
        }
    }
}

impl AwsProvider {
    fn operation_budget(&self, resource_type: &str, kind: OperationKind) -> Duration {
        self.resource_schema(resource_type)
            .and_then(|schema| schema.operation_config.as_ref())
            .map_or(DEFAULT_OPERATION_BUDGET, |config| config.budget(kind))
    }

    fn read_back_diagnostic(
        &self,
        id: &ResourceId,
        reason: impl Into<String>,
    ) -> (String, Vec<String>) {
        let mut missing_attributes = self
            .resource_schema(id.resource_type.as_str())
            .map(|schema| schema.attributes.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        missing_attributes.sort();
        if missing_attributes.is_empty() {
            missing_attributes.push("state".to_string());
        }
        (reason.into(), missing_attributes)
    }

    pub(crate) fn create_outcome_from_read(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        attributes: std::collections::HashMap<String, carina_core::resource::Value>,
        outcome: ProviderResult<ReadOutcome>,
    ) -> CreateOutcome {
        let reason = match outcome {
            Ok(ReadOutcome::Found { state }) => return CreateOutcome::Success { state },
            Ok(ReadOutcome::NotFound { .. }) => {
                "post-create read-back did not find the created resource".to_string()
            }
            Err(error) => format!("post-create read-back failed: {error}"),
        };
        let state = AppliedState::from_identifier(id.clone(), identifier, attributes);
        let (reason, missing_attributes) = self.read_back_diagnostic(id, reason);
        CreateOutcome::partial_success(state, reason, missing_attributes)
    }

    fn update_outcome_from_read(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        attributes: std::collections::HashMap<String, carina_core::resource::Value>,
        outcome: ProviderResult<ReadOutcome>,
    ) -> UpdateOutcome {
        let reason = match outcome {
            Ok(ReadOutcome::Found { state }) => return UpdateOutcome::Success { state },
            Ok(ReadOutcome::NotFound { .. }) => {
                "post-update read-back did not find the updated resource".to_string()
            }
            Err(error) => format!("post-update read-back failed: {error}"),
        };
        let state = AppliedState::from_identifier(id.clone(), identifier, attributes);
        let (reason, missing_attributes) = self.read_back_diagnostic(id, reason);
        UpdateOutcome::partial_success(state, reason, missing_attributes)
    }

    /// Perform the initial, non-blocking create step.
    pub async fn create_step(&self, resource: &Resource) -> ProviderResult<AwsCreateStep> {
        let schema = self
            .resource_schema(resource.id.resource_type.as_str())
            .ok_or_else(|| {
                ProviderError::internal(format!(
                    "Unknown resource schema: {}",
                    resource.id.resource_type
                ))
                .for_resource(resource.id.clone())
            })?;
        match resource.id.resource_type.as_str() {
            "ec2.NatGateway" => self
                .create_ec2_nat_gateway_start(resource, schema)
                .await
                .map(|nat_gateway_id| {
                    create_pending(AwsCreateToken::NatGateway { nat_gateway_id })
                }),
            "ec2.TransitGateway" => self
                .create_ec2_transit_gateway_start(resource, schema)
                .await
                .map(|transit_gateway_id| {
                    create_pending(AwsCreateToken::TransitGateway { transit_gateway_id })
                }),
            "ec2.TransitGatewayAttachment" => self
                .create_ec2_transit_gateway_attachment_start(resource, schema)
                .await
                .map(|attachment_id| {
                    create_pending(AwsCreateToken::TransitGatewayAttachment { attachment_id })
                }),
            "organizations.Account" => self
                .create_organizations_account_start(resource, schema)
                .await
                .map(|create_account_request_id| {
                    create_pending(AwsCreateToken::OrganizationsAccount {
                        create_account_request_id,
                    })
                }),
            "acm.Certificate" => self
                .create_acm_certificate(resource, schema)
                .await
                .map(OperationStep::Done),
            "s3.Bucket" => self.create_s3_bucket(resource).await.map(create_done),
            "s3.BucketPolicy" => self
                .create_s3_bucket_policy(resource)
                .await
                .map(create_done),
            "s3.BucketPublicAccessBlock" => self
                .create_s3_bucket_public_access_block(resource)
                .await
                .map(create_done),
            "s3.BucketVersioning" => self
                .create_s3_bucket_versioning(resource, schema)
                .await
                .map(create_done),
            "s3.BucketObjectLockConfiguration" => self
                .create_s3_bucket_object_lock_configuration(resource, schema)
                .await
                .map(create_done),
            "s3.BucketServerSideEncryptionConfiguration" => self
                .create_s3_bucket_server_side_encryption_configuration(resource, schema)
                .await
                .map(create_done),
            "s3.BucketAcl" => self
                .create_s3_bucket_acl(resource, schema)
                .await
                .map(create_done),
            "s3.BucketOwnershipControls" => self
                .create_s3_bucket_ownership_controls(resource, schema)
                .await
                .map(create_done),
            "s3.BucketReplicationConfiguration" => self
                .create_s3_bucket_replication_configuration(resource, schema)
                .await
                .map(create_done),
            "s3.BucketLifecycleConfiguration" => self
                .create_s3_bucket_lifecycle_configuration(resource, schema)
                .await
                .map(create_done),
            "s3.BucketWebsiteConfiguration" => self
                .create_s3_bucket_website_configuration(resource, schema)
                .await
                .map(create_done),
            "s3.BucketCorsConfiguration" => self
                .create_s3_bucket_cors_configuration(resource)
                .await
                .map(create_done),
            "s3.BucketNotificationConfiguration" => self
                .create_s3_bucket_notification_configuration(resource)
                .await
                .map(create_done),
            "s3.BucketLogging" => self
                .create_s3_bucket_logging(resource, schema)
                .await
                .map(create_done),
            "ec2.Eip" => self.create_ec2_eip(resource, schema).await.map(create_done),
            "ec2.Vpc" => self.create_ec2_vpc(resource, schema).await.map(create_done),
            "ec2.Subnet" => self
                .create_ec2_subnet(resource, schema)
                .await
                .map(create_done),
            "ec2.InternetGateway" => self
                .create_ec2_internet_gateway(resource)
                .await
                .map(create_done),
            "ec2.RouteTable" => self.create_ec2_route_table(resource).await.map(create_done),
            "ec2.Route" => self.create_ec2_route(resource).await.map(create_done),
            "ec2.SecurityGroup" => self
                .create_ec2_security_group(resource)
                .await
                .map(create_done),
            "ec2.SecurityGroupIngress" => self
                .create_ec2_security_group_ingress(resource, schema)
                .await
                .map(create_done),
            "ec2.SecurityGroupEgress" => self
                .create_ec2_security_group_egress(resource, schema)
                .await
                .map(create_done),
            "ec2.SubnetRouteTableAssociation" => self
                .create_ec2_subnet_route_table_association(resource)
                .await
                .map(create_done),
            "ec2.FlowLog" => self
                .create_ec2_flow_log(resource, schema)
                .await
                .map(create_done),
            "ec2.VpcEndpoint" => self
                .create_ec2_vpc_endpoint(resource, schema)
                .await
                .map(create_done),
            "ec2.VpcGatewayAttachment" => self
                .create_ec2_vpc_gateway_attachment(resource)
                .await
                .map(create_done),
            "ec2.VpnGateway" => self
                .create_ec2_vpn_gateway(resource, schema)
                .await
                .map(create_done),
            "ec2.VpcPeeringConnection" => self
                .create_ec2_vpc_peering_connection(resource)
                .await
                .map(create_done),
            "ec2.EgressOnlyInternetGateway" => self
                .create_ec2_egress_only_internet_gateway(resource)
                .await
                .map(create_done),
            "organizations.Organization" => self
                .create_organizations_organization(resource, schema)
                .await
                .map(create_done),
            "iam.Role" => self.create_iam_role(resource).await.map(create_done),
            "logs.LogGroup" => self
                .create_logs_log_group(resource, schema)
                .await
                .map(create_done),
            "route53.RecordSet" => self
                .create_route53_record_set(resource, schema)
                .await
                .map(create_done),
            "sqs.Queue" => self
                .create_sqs_queue(resource, schema)
                .await
                .map(create_done),
            _ => Err(ProviderError::internal(format!(
                "Unknown resource type: {}",
                resource.id.resource_type
            ))
            .for_resource(resource.id.clone())),
        }
    }

    /// Perform one describe step for a pending create operation.
    pub async fn poll_create_step(
        &self,
        resource: &Resource,
        token: AwsCreateToken,
    ) -> ProviderResult<AwsCreateStep> {
        let id = &resource.id;
        match token {
            AwsCreateToken::NatGateway { nat_gateway_id } => {
                match self
                    .poll_ec2_nat_gateway_create(id, nat_gateway_id.as_str())
                    .await?
                {
                    CreatePoll::Ready(()) => {
                        let outcome = self.read_ec2_nat_gateway(id, &nat_gateway_id).await;
                        Ok(create_done(self.create_outcome_from_read(
                            id,
                            nat_gateway_id,
                            resource.resolved_attributes(),
                            outcome,
                        )))
                    }
                    CreatePoll::Failed => {
                        Err(ProviderError::api_error("NAT gateway creation failed")
                            .for_resource(id.clone()))
                    }
                    CreatePoll::Pending => Ok(create_pending(AwsCreateToken::NatGateway {
                        nat_gateway_id,
                    })),
                }
            }
            AwsCreateToken::TransitGateway { transit_gateway_id } => {
                match self
                    .poll_ec2_transit_gateway_create(id, transit_gateway_id.as_str())
                    .await?
                {
                    CreatePoll::Ready(()) => {
                        let outcome = self.read_ec2_transit_gateway(id, &transit_gateway_id).await;
                        Ok(create_done(self.create_outcome_from_read(
                            id,
                            transit_gateway_id,
                            resource.resolved_attributes(),
                            outcome,
                        )))
                    }
                    CreatePoll::Failed => {
                        Err(ProviderError::api_error("Transit gateway creation failed")
                            .for_resource(id.clone()))
                    }
                    CreatePoll::Pending => Ok(create_pending(AwsCreateToken::TransitGateway {
                        transit_gateway_id,
                    })),
                }
            }
            AwsCreateToken::TransitGatewayAttachment { attachment_id } => {
                match self
                    .poll_ec2_transit_gateway_attachment_create(id, attachment_id.as_str())
                    .await?
                {
                    CreatePoll::Ready(()) => {
                        let outcome = self
                            .read_ec2_transit_gateway_attachment(id, &attachment_id)
                            .await;
                        Ok(create_done(self.create_outcome_from_read(
                            id,
                            attachment_id,
                            resource.resolved_attributes(),
                            outcome,
                        )))
                    }
                    CreatePoll::Failed => Err(ProviderError::api_error(
                        "Transit gateway attachment creation failed",
                    )
                    .for_resource(id.clone())),
                    CreatePoll::Pending => {
                        Ok(create_pending(AwsCreateToken::TransitGatewayAttachment {
                            attachment_id,
                        }))
                    }
                }
            }
            AwsCreateToken::OrganizationsAccount {
                create_account_request_id,
            } => {
                match self
                    .poll_organizations_account_create(id, &create_account_request_id)
                    .await?
                {
                    CreatePoll::Ready(account_id) => self
                        .finish_organizations_account_create(resource, &account_id)
                        .await
                        .map(create_done),
                    CreatePoll::Failed => Err(ProviderError::api_error("Account creation failed")
                        .for_resource(id.clone())),
                    CreatePoll::Pending => {
                        Ok(create_pending(AwsCreateToken::OrganizationsAccount {
                            create_account_request_id,
                        }))
                    }
                }
            }
        }
    }

    /// Drive create steps to a settled result for native callers.
    pub async fn create_resource(&self, resource: &Resource) -> ProviderResult<CreateOutcome> {
        let budget =
            self.operation_budget(resource.id.resource_type.as_str(), OperationKind::Create);
        drive_operation(
            &resource.id,
            OperationKind::Create,
            budget,
            self.create_step(resource),
            |token| self.poll_create_step(resource, token),
            tokio::time::sleep,
        )
        .await
    }

    /// Dispatch a checked data source's provider-facing payload.
    ///
    /// The WASM guest bridge already receives a protocol-validated request and
    /// cannot forge carina-core's sealed provider-readiness witness, so it
    /// shares this body with the core [`Provider`] implementation.
    pub async fn read_data_source_resource(&self, resource: &DataSource) -> ProviderResult<State> {
        crate::provider_generated::dispatch_read_data_source(self, resource).await
    }

    /// Apply a checked update patch without constructing a sealed core request.
    ///
    /// This is also the WASM guest bridge entry point. The bridge receives the
    /// protocol request's current state and patch separately, while native core
    /// calls reach the same body through [`Provider::update`].
    pub async fn update_with_patch(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        patch: &UpdatePatch,
    ) -> ProviderResult<UpdateOutcome> {
        let id = id.clone();
        // The aws provider's per-resource `update_*` methods predate the
        // Level 3 patch contract and accept a full `to: Resource`.
        // Reconstruct that from `(from, patch)` here so each method's
        // existing logic continues to work without per-resource churn.
        // Future per-resource updates can consume `patch` directly for
        // partial-update API paths once those are wired in.
        let to = apply_patch_to_state(from, patch);
        let partial_attributes = to.resolved_attributes();
        let outcome_id = id.clone();
        let schema = self
            .resource_schema(id.resource_type.as_str())
            .ok_or_else(|| {
                ProviderError::internal(format!("Unknown resource schema: {}", id.resource_type))
                    .for_resource(id.clone())
            })?;
        let read_back = match id.resource_type.as_str() {
            "s3.Bucket" => self.update_s3_bucket(id, identifier, from, to).await,
            "s3.BucketPolicy" => self.update_s3_bucket_policy(id, identifier, from, to).await,
            "s3.BucketPublicAccessBlock" => {
                self.update_s3_bucket_public_access_block(id, identifier, from, to)
                    .await
            }
            "s3.BucketVersioning" => {
                self.update_s3_bucket_versioning(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketObjectLockConfiguration" => {
                self.update_s3_bucket_object_lock_configuration(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketServerSideEncryptionConfiguration" => {
                self.update_s3_bucket_server_side_encryption_configuration(
                    id, identifier, from, to, schema,
                )
                .await
            }
            "s3.BucketAcl" => {
                self.update_s3_bucket_acl(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketOwnershipControls" => {
                self.update_s3_bucket_ownership_controls(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketReplicationConfiguration" => {
                self.update_s3_bucket_replication_configuration(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketLifecycleConfiguration" => {
                self.update_s3_bucket_lifecycle_configuration(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketWebsiteConfiguration" => {
                self.update_s3_bucket_website_configuration(id, identifier, from, to, schema)
                    .await
            }
            "s3.BucketCorsConfiguration" => {
                self.update_s3_bucket_cors_configuration(id, identifier, from, to)
                    .await
            }
            "s3.BucketNotificationConfiguration" => {
                self.update_s3_bucket_notification_configuration(id, identifier, from, to)
                    .await
            }
            "s3.BucketLogging" => {
                self.update_s3_bucket_logging(id, identifier, from, to, schema)
                    .await
            }
            "ec2.Eip" => self.update_ec2_eip(id, identifier, from, to, schema).await,
            "ec2.Vpc" => self.update_ec2_vpc(id, identifier, from, to, schema).await,
            "ec2.Subnet" => {
                self.update_ec2_subnet(id, identifier, from, to, schema)
                    .await
            }
            "ec2.InternetGateway" => {
                self.update_ec2_internet_gateway(id, identifier, from, to)
                    .await
            }
            "ec2.NatGateway" => {
                self.update_ec2_nat_gateway(id, identifier, from, to, schema)
                    .await
            }
            "ec2.RouteTable" => self.update_ec2_route_table(id, identifier, from, to).await,
            "ec2.Route" => self.update_ec2_route(id, identifier, to).await,
            "ec2.SecurityGroup" => {
                self.update_ec2_security_group(id, identifier, from, to)
                    .await
            }
            "ec2.SecurityGroupIngress" => {
                self.update_ec2_security_group_ingress(id, identifier, to, schema)
                    .await
            }
            "ec2.SecurityGroupEgress" => {
                self.update_ec2_security_group_egress(id, identifier, to, schema)
                    .await
            }
            "ec2.SubnetRouteTableAssociation" => {
                self.update_ec2_subnet_route_table_association(id, identifier, to)
                    .await
            }
            "ec2.FlowLog" => {
                self.update_ec2_flow_log(id, identifier, from, to, schema)
                    .await
            }
            "ec2.VpcEndpoint" => {
                self.update_ec2_vpc_endpoint(id, identifier, from, to, schema)
                    .await
            }
            "ec2.VpcGatewayAttachment" => {
                self.update_ec2_vpc_gateway_attachment(id, identifier).await
            }
            "ec2.VpnGateway" => {
                self.update_ec2_vpn_gateway(id, identifier, from, to, schema)
                    .await
            }
            "ec2.TransitGateway" => {
                self.update_ec2_transit_gateway(id, identifier, from, to, schema)
                    .await
            }
            "ec2.TransitGatewayAttachment" => {
                self.update_ec2_transit_gateway_attachment(id, identifier, from, to, schema)
                    .await
            }
            "ec2.VpcPeeringConnection" => {
                self.update_ec2_vpc_peering_connection(id, identifier, from, to)
                    .await
            }
            "ec2.EgressOnlyInternetGateway" => {
                self.update_ec2_egress_only_internet_gateway(id, identifier, from, to)
                    .await
            }
            "organizations.Account" => {
                self.update_organizations_account(id, identifier, from, to, schema)
                    .await
            }
            "organizations.Organization" => {
                // All attributes are read-only or create-only; read back current state.
                Ok(self.read_organizations_organization(&id, identifier).await)
            }
            "iam.Role" => self.update_iam_role(id, identifier, from, to).await,
            "logs.LogGroup" => {
                self.update_logs_log_group(id, identifier, from, to, schema)
                    .await
            }
            "route53.RecordSet" => {
                self.update_route53_record_set(id, identifier, to, schema)
                    .await
            }
            "acm.Certificate" => {
                self.update_acm_certificate(id, identifier, from, to, schema)
                    .await
            }
            "sqs.Queue" => {
                self.update_sqs_queue(id, identifier, from, to, schema)
                    .await
            }
            _ => Err(ProviderError::internal(format!(
                "Unknown resource type: {}",
                id.resource_type
            ))
            .for_resource(id.clone())),
        }?;
        Ok(self.update_outcome_from_read(
            &outcome_id,
            identifier.clone(),
            partial_attributes,
            read_back,
        ))
    }

    /// Perform the initial, non-blocking delete step.
    pub async fn delete_step(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        request: &DeleteRequest,
    ) -> ProviderResult<AwsDeleteStep> {
        let id = id.clone();
        match id.resource_type.as_str() {
            "ec2.NatGateway" => self.delete_ec2_nat_gateway(id, identifier).await.map(|()| {
                delete_pending(AwsDeleteToken::NatGateway {
                    nat_gateway_id: identifier.clone(),
                })
            }),
            "ec2.TransitGateway" => {
                self.delete_ec2_transit_gateway(id, identifier)
                    .await
                    .map(|()| {
                        delete_pending(AwsDeleteToken::TransitGateway {
                            transit_gateway_id: identifier.clone(),
                        })
                    })
            }
            "ec2.TransitGatewayAttachment" => self
                .delete_ec2_transit_gateway_attachment(id, identifier)
                .await
                .map(|()| {
                    delete_pending(AwsDeleteToken::TransitGatewayAttachment {
                        attachment_id: identifier.clone(),
                    })
                }),
            "s3.Bucket" => self
                .delete_s3_bucket(id, identifier, &request.directives)
                .await
                .map(delete_done),
            "s3.BucketPolicy" => self
                .delete_s3_bucket_policy_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketPublicAccessBlock" => self
                .delete_s3_bucket_public_access_block_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketVersioning" => self
                .delete_s3_bucket_versioning_suspend(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketObjectLockConfiguration" => self
                .delete_s3_bucket_object_lock_configuration_noop(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketServerSideEncryptionConfiguration" => self
                .delete_s3_bucket_server_side_encryption_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketAcl" => self
                .delete_s3_bucket_acl_reset(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketOwnershipControls" => self
                .delete_s3_bucket_ownership_controls_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketReplicationConfiguration" => self
                .delete_s3_bucket_replication_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketLifecycleConfiguration" => self
                .delete_s3_bucket_lifecycle_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketWebsiteConfiguration" => self
                .delete_s3_bucket_website_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketCorsConfiguration" => self
                .delete_s3_bucket_cors_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketNotificationConfiguration" => self
                .delete_s3_bucket_notification_configuration_idempotent(id, identifier)
                .await
                .map(delete_done),
            "s3.BucketLogging" => self
                .delete_s3_bucket_logging_idempotent(id, identifier)
                .await
                .map(delete_done),
            "ec2.Eip" => self.delete_ec2_eip(id, identifier).await.map(delete_done),
            "ec2.Vpc" => self.delete_ec2_vpc(id, identifier).await.map(delete_done),
            "ec2.Subnet" => self
                .delete_ec2_subnet(id, identifier)
                .await
                .map(delete_done),
            "ec2.InternetGateway" => self
                .delete_ec2_internet_gateway(id, identifier)
                .await
                .map(delete_done),
            "ec2.RouteTable" => self
                .delete_ec2_route_table(id, identifier)
                .await
                .map(delete_done),
            "ec2.Route" => self.delete_ec2_route(id, identifier).await.map(delete_done),
            "ec2.SecurityGroup" => self
                .delete_ec2_security_group(id, identifier)
                .await
                .map(delete_done),
            "ec2.SecurityGroupIngress" => self
                .delete_ec2_security_group_ingress(id, identifier)
                .await
                .map(delete_done),
            "ec2.SecurityGroupEgress" => self
                .delete_ec2_security_group_egress(id, identifier)
                .await
                .map(delete_done),
            "ec2.SubnetRouteTableAssociation" => self
                .delete_ec2_subnet_route_table_association(id, identifier)
                .await
                .map(delete_done),
            "ec2.FlowLog" => self
                .delete_ec2_flow_log(id, identifier)
                .await
                .map(delete_done),
            "ec2.VpcEndpoint" => self
                .delete_ec2_vpc_endpoint(id, identifier)
                .await
                .map(delete_done),
            "ec2.VpcGatewayAttachment" => self
                .delete_ec2_vpc_gateway_attachment(id, identifier)
                .await
                .map(delete_done),
            "ec2.VpnGateway" => self
                .delete_ec2_vpn_gateway(id, identifier)
                .await
                .map(delete_done),
            "ec2.VpcPeeringConnection" => self
                .delete_ec2_vpc_peering_connection(id, identifier)
                .await
                .map(delete_done),
            "ec2.EgressOnlyInternetGateway" => self
                .delete_ec2_egress_only_internet_gateway(id, identifier)
                .await
                .map(delete_done),
            "organizations.Account" => self
                .delete_organizations_account(id, identifier)
                .await
                .map(delete_done),
            "organizations.Organization" => self
                .delete_organizations_organization(id, identifier)
                .await
                .map(delete_done),
            "iam.Role" => self.delete_iam_role(id, identifier).await.map(delete_done),
            "logs.LogGroup" => self
                .delete_logs_log_group(id, identifier)
                .await
                .map(delete_done),
            "route53.RecordSet" => self
                .delete_route53_record_set(id, identifier)
                .await
                .map(delete_done),
            "acm.Certificate" => self
                .delete_acm_certificate(id, identifier)
                .await
                .map(delete_done),
            "sqs.Queue" => self.delete_sqs_queue(id, identifier).await.map(delete_done),
            _ => Err(ProviderError::internal(format!(
                "Unknown resource type: {}",
                id.resource_type
            ))
            .for_resource(id.clone())),
        }
    }

    /// Perform one describe step for a pending delete operation.
    pub async fn poll_delete_step(
        &self,
        id: &ResourceId,
        token: AwsDeleteToken,
    ) -> ProviderResult<AwsDeleteStep> {
        match token {
            AwsDeleteToken::NatGateway { nat_gateway_id } => {
                match self
                    .poll_ec2_nat_gateway_delete(id, nat_gateway_id.as_str())
                    .await?
                {
                    DeletePoll::Gone => Ok(OperationStep::Done(())),
                    DeletePoll::Pending => Ok(delete_pending(AwsDeleteToken::NatGateway {
                        nat_gateway_id,
                    })),
                }
            }
            AwsDeleteToken::TransitGateway { transit_gateway_id } => {
                match self
                    .poll_ec2_transit_gateway_delete(id, transit_gateway_id.as_str())
                    .await?
                {
                    DeletePoll::Gone => Ok(OperationStep::Done(())),
                    DeletePoll::Pending => Ok(delete_pending(AwsDeleteToken::TransitGateway {
                        transit_gateway_id,
                    })),
                }
            }
            AwsDeleteToken::TransitGatewayAttachment { attachment_id } => {
                match self
                    .poll_ec2_transit_gateway_attachment_delete(id, attachment_id.as_str())
                    .await?
                {
                    DeletePoll::Gone => Ok(OperationStep::Done(())),
                    DeletePoll::Pending => {
                        Ok(delete_pending(AwsDeleteToken::TransitGatewayAttachment {
                            attachment_id,
                        }))
                    }
                }
            }
        }
    }
}

impl Provider for AwsProvider {
    fn name(&self) -> &str {
        "aws"
    }

    fn required_permissions(&self, _id: &ResourceId, _op: PlanOp) -> ProviderResult<Vec<String>> {
        Ok(Vec::new())
    }

    fn read(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        _request: ReadRequest,
    ) -> BoxFuture<'_, ProviderResult<ReadOutcome>> {
        let id = id.clone();
        let identifier = identifier.clone();
        Box::pin(async move {
            let state = match id.resource_type.as_str() {
                "s3.Bucket" => self.read_s3_bucket(&id, &identifier).await,
                "s3.BucketPolicy" => self.read_s3_bucket_policy(&id, &identifier).await,
                "s3.BucketPublicAccessBlock" => {
                    self.read_s3_bucket_public_access_block(&id, &identifier)
                        .await
                }
                "s3.BucketVersioning" => self.read_s3_bucket_versioning(&id, &identifier).await,
                "s3.BucketObjectLockConfiguration" => {
                    self.read_s3_bucket_object_lock_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketServerSideEncryptionConfiguration" => {
                    self.read_s3_bucket_server_side_encryption_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketAcl" => self.read_s3_bucket_acl(&id, &identifier).await,
                "s3.BucketOwnershipControls" => {
                    self.read_s3_bucket_ownership_controls(&id, &identifier)
                        .await
                }
                "s3.BucketReplicationConfiguration" => {
                    self.read_s3_bucket_replication_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketLifecycleConfiguration" => {
                    self.read_s3_bucket_lifecycle_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketWebsiteConfiguration" => {
                    self.read_s3_bucket_website_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketCorsConfiguration" => {
                    self.read_s3_bucket_cors_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketNotificationConfiguration" => {
                    self.read_s3_bucket_notification_configuration(&id, &identifier)
                        .await
                }
                "s3.BucketLogging" => self.read_s3_bucket_logging(&id, &identifier).await,
                "ec2.Eip" => self.read_ec2_eip(&id, &identifier).await,
                "ec2.Vpc" => self.read_ec2_vpc(&id, &identifier).await,
                "ec2.Subnet" => self.read_ec2_subnet(&id, &identifier).await,
                "ec2.InternetGateway" => self.read_ec2_internet_gateway(&id, &identifier).await,
                "ec2.NatGateway" => self.read_ec2_nat_gateway(&id, &identifier).await,
                "ec2.RouteTable" => self.read_ec2_route_table(&id, &identifier).await,
                "ec2.Route" => self.read_ec2_route(&id, &identifier).await,
                "ec2.SecurityGroup" => self.read_ec2_security_group(&id, &identifier).await,
                "ec2.SecurityGroupIngress" => {
                    self.read_ec2_security_group_ingress(&id, &identifier).await
                }
                "ec2.SecurityGroupEgress" => {
                    self.read_ec2_security_group_egress(&id, &identifier).await
                }
                "ec2.SubnetRouteTableAssociation" => {
                    self.read_ec2_subnet_route_table_association(&id, &identifier)
                        .await
                }
                "ec2.FlowLog" => self.read_ec2_flow_log(&id, &identifier).await,
                "ec2.VpcEndpoint" => self.read_ec2_vpc_endpoint(&id, &identifier).await,
                "ec2.VpcGatewayAttachment" => {
                    self.read_ec2_vpc_gateway_attachment(&id, &identifier).await
                }
                "ec2.VpnGateway" => self.read_ec2_vpn_gateway(&id, &identifier).await,
                "ec2.TransitGateway" => self.read_ec2_transit_gateway(&id, &identifier).await,
                "ec2.TransitGatewayAttachment" => {
                    self.read_ec2_transit_gateway_attachment(&id, &identifier)
                        .await
                }
                "ec2.VpcPeeringConnection" => {
                    self.read_ec2_vpc_peering_connection(&id, &identifier).await
                }
                "ec2.EgressOnlyInternetGateway" => {
                    self.read_ec2_egress_only_internet_gateway(&id, &identifier)
                        .await
                }
                "organizations.Account" => self.read_organizations_account(&id, &identifier).await,
                "organizations.Organization" => {
                    self.read_organizations_organization(&id, &identifier).await
                }
                "iam.Role" => self.read_iam_role(&id, &identifier).await,
                "logs.LogGroup" => self.read_logs_log_group(&id, &identifier).await,
                "route53.RecordSet" => self.read_route53_record_set(&id, &identifier).await,
                "acm.Certificate" => self.read_acm_certificate(&id, &identifier).await,
                "sqs.Queue" => self.read_sqs_queue(&id, &identifier).await,
                _ => Err(ProviderError::internal(format!(
                    "Unknown resource type: {}",
                    id.resource_type
                ))
                .for_resource(id.clone())),
            }?;

            Ok(state)
        })
    }

    fn read_data_source(
        &self,
        resource: &ProviderReadyDataSource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.as_data_source().clone();
        Box::pin(async move { self.read_data_source_resource(&resource).await })
    }

    fn create(
        &self,
        _id: &ResourceId,
        request: CreateRequest,
    ) -> BoxFuture<'_, ProviderResult<CreateOutcome>> {
        Box::pin(async move { self.create_resource(request.resource().as_resource()).await })
    }

    fn update(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        request: UpdateRequest,
    ) -> BoxFuture<'_, ProviderResult<UpdateOutcome>> {
        let id = id.clone();
        let identifier = identifier.clone();
        Box::pin(async move {
            self.update_with_patch(&id, &identifier, request.from(), request.patch())
                .await
        })
    }

    fn delete(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        request: DeleteRequest,
    ) -> BoxFuture<'_, ProviderResult<()>> {
        let id = id.clone();
        let identifier = identifier.clone();
        Box::pin(async move {
            let budget = self.operation_budget(id.resource_type.as_str(), OperationKind::Delete);
            drive_operation(
                &id,
                OperationKind::Delete,
                budget,
                self.delete_step(&id, &identifier, &request),
                |token| self.poll_delete_step(&id, token),
                tokio::time::sleep,
            )
            .await
        })
    }
}

#[cfg(test)]
mod pending_operation_tests {
    use std::collections::HashMap;
    use std::future::ready;
    use std::time::Duration;

    use carina_provider_protocol::OperationStep;

    use super::*;

    fn identifier(value: &str) -> AppliedIdentifier {
        AppliedIdentifier::new(value).expect("test identifier must be non-empty")
    }

    #[test]
    fn pending_tokens_round_trip_without_resource_attributes() {
        let create_tokens = [
            AwsCreateToken::NatGateway {
                nat_gateway_id: identifier("nat-0123456789"),
            },
            AwsCreateToken::TransitGateway {
                transit_gateway_id: identifier("tgw-0123456789"),
            },
            AwsCreateToken::TransitGatewayAttachment {
                attachment_id: identifier("tgw-attach-0123456789"),
            },
            AwsCreateToken::OrganizationsAccount {
                create_account_request_id: "car-0123456789".to_string(),
            },
        ];
        for token in create_tokens {
            let json = serde_json::to_string(&token).expect("create token should serialize");
            assert_eq!(
                serde_json::from_str::<AwsCreateToken>(&json)
                    .expect("create token should deserialize"),
                token
            );
            let object = serde_json::from_str::<serde_json::Value>(&json)
                .expect("create token JSON")
                .as_object()
                .expect("create token should be an object")
                .clone();
            assert_eq!(object.len(), 2, "token must contain only phase and one id");
            assert!(object.contains_key("phase"));
        }

        let delete_tokens = [
            AwsDeleteToken::NatGateway {
                nat_gateway_id: identifier("nat-0123456789"),
            },
            AwsDeleteToken::TransitGateway {
                transit_gateway_id: identifier("tgw-0123456789"),
            },
            AwsDeleteToken::TransitGatewayAttachment {
                attachment_id: identifier("tgw-attach-0123456789"),
            },
        ];
        for token in delete_tokens {
            let json = serde_json::to_string(&token).expect("delete token should serialize");
            assert_eq!(
                serde_json::from_str::<AwsDeleteToken>(&json)
                    .expect("delete token should deserialize"),
                token
            );
            let object = serde_json::from_str::<serde_json::Value>(&json)
                .expect("delete token JSON")
                .as_object()
                .expect("delete token should be an object")
                .clone();
            assert_eq!(object.len(), 2, "token must contain only phase and one id");
            assert!(object.contains_key("phase"));
        }
    }

    #[tokio::test]
    async fn drive_operation_returns_done_value_from_poll() {
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "example", None);
        let expected =
            AppliedState::from_identifier(id.clone(), identifier("nat-read-back"), HashMap::new());
        let mut done = Some(CreateOutcome::Success {
            state: expected.clone(),
        });

        let outcome = drive_operation(
            &id,
            OperationKind::Create,
            Duration::from_secs(30),
            ready(Ok(OperationStep::Pending {
                pending: AwsCreateToken::NatGateway {
                    nat_gateway_id: identifier("nat-123"),
                },
                poll_after_secs: 5,
            })),
            |_| ready(Ok(OperationStep::Done(done.take().expect("one poll")))),
            |_| ready(()),
        )
        .await
        .expect("pending create should settle");

        match outcome {
            CreateOutcome::Success { state } => assert_eq!(state, expected),
            other => panic!("unexpected create outcome: {other:?}"),
        }
    }

    #[tokio::test]
    async fn native_create_driver_maps_over_budget_to_resource_timeout() {
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "example", None);
        let result: ProviderResult<CreateOutcome> = drive_operation(
            &id,
            OperationKind::Create,
            Duration::ZERO,
            ready(Ok(OperationStep::Pending {
                pending: AwsCreateToken::NatGateway {
                    nat_gateway_id: identifier("nat-123"),
                },
                poll_after_secs: 5,
            })),
            |token| {
                ready(Ok(OperationStep::Pending {
                    pending: token,
                    poll_after_secs: 5,
                }))
            },
            |_| ready(()),
        )
        .await;

        let error = result.expect_err("zero budget should time out");
        assert_eq!(error.variant_name(), "timeout");
        assert_eq!(
            error.to_wire().message,
            "create of ec2.NatGateway did not finish within 0ns"
        );
        assert_eq!(
            error.to_wire().resource_id,
            Some(carina_provider_protocol::types::ResourceId {
                provider: "aws".to_string(),
                resource_type: "ec2.NatGateway".to_string(),
                identity: "example".to_string(),
            })
        );
    }
}
