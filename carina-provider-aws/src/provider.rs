//! Provider trait implementation for AWS

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use carina_core::effect::PlanOp;
use carina_core::provider::{
    AppliedIdentifier, AppliedState, BoxFuture, CreateAttempt, CreateFailure, CreateOutcome,
    CreateRequest, DeleteRequest, IssuedCreate, Observation, PartialReadDiagnostic, PendingCreate,
    PendingToken, Poll, PollAttempt, Provider, ProviderError, ProviderReadyDataSource,
    ProviderResult, ReadOutcome, ReadRequest, Resume, ResumeAttempt, ResumeCreateRequest,
    ResumeFailure, ResumeOutcome, ResumedCreate, Sent, UpdateOutcome, UpdatePatch, UpdateRequest,
};
use carina_core::resource::{DataSource, Resource, ResourceId, State, Value};
use carina_provider_protocol::{
    DEFAULT_OPERATION_BUDGET, DriveError, DriveOutcome, OperationKind, OperationStep, drive_pending,
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

pub type AwsDeleteStep = OperationStep<(), ProviderError, AwsDeleteToken>;

/// A completed AWS create before it is finished through the caller's v10
/// typestate handle.
#[derive(Debug)]
pub(crate) enum CreateCompletion {
    Success {
        identifier: AppliedIdentifier,
        attributes: HashMap<String, Value>,
    },
    Partial {
        identifier: AppliedIdentifier,
        attributes: HashMap<String, Value>,
        reason: String,
        missing_attributes: Vec<String>,
    },
}

impl CreateCompletion {
    pub(crate) fn partial(
        state: AppliedState,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self {
        let (_id, identifier, attributes) = state.into_parts();
        Self::Partial {
            identifier,
            attributes,
            reason,
            missing_attributes,
        }
    }
}

/// Native providers drive pending steps themselves, while the WASM guest
/// returns one step to the host. These adapters let both paths consume the
/// same protocol-v10 typestate at the exact AWS classification boundary.
#[doc(hidden)]
pub trait AwsIssuedCreate {
    type Step;
    type Verdict;

    fn identifier(&self) -> &AppliedIdentifier;
    fn succeeded(self, attributes: HashMap<String, Value>) -> Self::Step;
    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self::Step;
    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict;
    fn discarded(self, error: ProviderError) -> Self::Verdict;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
}

#[doc(hidden)]
pub trait AwsCreateAttempt {
    type Step;
    type Verdict;
    type Issued: AwsIssuedCreate<Step = Self::Step, Verdict = Self::Verdict>;

    fn untracked(self, error: ProviderError) -> Self::Verdict;
    fn issued(self, identifier: AppliedIdentifier, sent: Sent) -> Self::Issued;
    fn pending(
        self,
        token: AwsCreateToken,
        sent: Sent,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
}

#[doc(hidden)]
pub trait AwsPollAttempt {
    type Step;
    type Verdict;
    type Issued: AwsIssuedCreate<Step = Self::Step, Verdict = Self::Verdict>;

    fn untracked(self, error: ProviderError) -> Self::Verdict;
    fn issued(self, identifier: AppliedIdentifier) -> Self::Issued;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
}

#[doc(hidden)]
pub enum AwsCreatePoll<U, I> {
    Unissued(U),
    Issued(I),
}

#[doc(hidden)]
pub trait AwsResumedCreate {
    type Step;
    type Verdict;

    fn identifier(&self) -> &AppliedIdentifier;
    fn completed(self, state: AppliedState) -> Self::Step;
    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict;
    fn discarded(self, error: ProviderError) -> Self::Verdict;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
}

#[doc(hidden)]
pub trait AwsResumeAttempt {
    type Step;
    type Verdict;
    type Resumed: AwsResumedCreate<Step = Self::Step, Verdict = Self::Verdict>;

    fn untracked(self, error: ProviderError) -> Self::Verdict;
    fn undetermined(self, error: ProviderError) -> Self::Verdict;
    fn issued(self, identifier: AppliedIdentifier) -> Self::Resumed;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
}

#[doc(hidden)]
pub enum AwsCreateResume<U, I> {
    Unissued(U),
    Issued(I),
}

#[derive(Debug)]
#[doc(hidden)]
pub enum CoreCreateStep {
    Done(CreateOutcome),
    Pending(PendingCreate),
}

#[derive(Debug)]
#[doc(hidden)]
pub enum CoreResumeStep {
    Done(ResumeOutcome),
    Pending(Box<Resume>),
}

fn encode_create_token(token: &AwsCreateToken) -> Result<PendingToken, String> {
    serde_json::to_string(token)
        .map(PendingToken::new)
        .map_err(|error| format!("failed to encode AWS create continuation: {error}"))
}

fn decode_create_token(token: &PendingToken) -> ProviderResult<AwsCreateToken> {
    serde_json::from_str(token.expose_for_provider_call()).map_err(|error| {
        ProviderError::internal_caused_by("failed to decode AWS create continuation", error)
    })
}

impl AwsCreateAttempt for CreateAttempt {
    type Step = CoreCreateStep;
    type Verdict = CreateFailure;
    type Issued = IssuedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        self.untracked(error)
    }

    fn issued(self, identifier: AppliedIdentifier, sent: Sent) -> Self::Issued {
        self.issued(identifier, sent)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        sent: Sent,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        match encode_create_token(&token) {
            Ok(token) => Ok(CoreCreateStep::Pending(self.pending(
                token,
                sent,
                poll_after_secs,
            ))),
            Err(message) => Err(self.untracked(ProviderError::internal(message))),
        }
    }
}

impl AwsPollAttempt for PollAttempt {
    type Step = CoreCreateStep;
    type Verdict = CreateFailure;
    type Issued = IssuedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        self.untracked(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Issued {
        self.issued(identifier)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        match encode_create_token(&token) {
            Ok(token) => Ok(CoreCreateStep::Pending(
                self.pending(token, poll_after_secs),
            )),
            Err(message) => Err(self.untracked(ProviderError::internal(message))),
        }
    }
}

impl AwsIssuedCreate for IssuedCreate {
    type Step = CoreCreateStep;
    type Verdict = CreateFailure;

    fn identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }

    fn succeeded(self, attributes: HashMap<String, Value>) -> Self::Step {
        CoreCreateStep::Done(self.succeeded(attributes))
    }

    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self::Step {
        match PartialReadDiagnostic::new(reason, missing_attributes) {
            Some(diagnostic) => CoreCreateStep::Done(self.partial(attributes, diagnostic)),
            None => CoreCreateStep::Done(self.succeeded(attributes)),
        }
    }

    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict {
        self.failed(observation, error)
    }

    fn discarded(self, error: ProviderError) -> Self::Verdict {
        self.discarded(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        match encode_create_token(&token) {
            Ok(token) => Ok(CoreCreateStep::Pending(
                self.pending(token, poll_after_secs),
            )),
            Err(message) => {
                let observation_error = ProviderError::internal(message.clone());
                Err(self.failed(
                    Observation::ReadFailed(observation_error),
                    ProviderError::internal(message),
                ))
            }
        }
    }
}

impl AwsResumeAttempt for ResumeAttempt {
    type Step = CoreResumeStep;
    type Verdict = ResumeFailure;
    type Resumed = ResumedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        self.untracked(error)
    }

    fn undetermined(self, error: ProviderError) -> Self::Verdict {
        self.preserve(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Resumed {
        self.issued(identifier)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        match encode_create_token(&token) {
            Ok(token) => Ok(CoreResumeStep::Pending(Box::new(self.repend(token)))),
            Err(message) => Err(self.preserve(ProviderError::internal(message))),
        }
    }
}

impl AwsResumedCreate for ResumedCreate {
    type Step = CoreResumeStep;
    type Verdict = ResumeFailure;

    fn identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }

    fn completed(self, state: AppliedState) -> Self::Step {
        CoreResumeStep::Done(self.completed(state))
    }

    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict {
        self.failed(observation, error)
    }

    fn discarded(self, error: ProviderError) -> Self::Verdict {
        self.discarded(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        match encode_create_token(&token) {
            Ok(token) => Ok(CoreResumeStep::Pending(Box::new(self.repend(token)))),
            Err(message) => Err(self.preserve(ProviderError::internal(message))),
        }
    }
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
    Tok: Clone,
    Initial: Future<Output = ProviderResult<OperationStep<T, ProviderError, Tok>>>,
    Poll: FnMut(Tok) -> PollFuture,
    PollFuture: Future<Output = ProviderResult<OperationStep<T, ProviderError, Tok>>>,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    match drive_pending(budget, initial, poll, sleep).await {
        Ok(DriveOutcome::Done(value)) => Ok(value),
        Ok(DriveOutcome::Verdict(error)) => Err(error),
        Ok(DriveOutcome::OverBudget {
            last_pending: _last_pending,
        }) => {
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
        Err(DriveError::Initial(error)) | Err(DriveError::Poll { error, .. }) => Err(error),
    }
}

impl AwsProvider {
    fn operation_budget(&self, resource_type: &str, kind: OperationKind) -> Duration {
        self.resource_schema(resource_type)
            .and_then(|schema| schema.operation_config())
            .map_or(DEFAULT_OPERATION_BUDGET, |config| config.budget(kind))
    }

    fn read_back_diagnostic(
        &self,
        id: &ResourceId,
        reason: impl Into<String>,
    ) -> (String, Vec<String>) {
        let mut missing_attributes = self
            .resource_schema(id.resource_type.as_str())
            .map(|schema| schema.attributes().keys().cloned().collect::<Vec<_>>())
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
        attributes: HashMap<String, Value>,
        outcome: ProviderResult<ReadOutcome>,
    ) -> CreateCompletion {
        let reason = match outcome {
            Ok(ReadOutcome::Found { state }) => {
                let (_id, identifier, attributes) = state.into_parts();
                return CreateCompletion::Success {
                    identifier,
                    attributes,
                };
            }
            Ok(ReadOutcome::NotFound { .. }) => {
                "post-create read-back did not find the created resource".to_string()
            }
            Err(error) => format!("post-create read-back failed: {error}"),
        };
        let (reason, missing_attributes) = self.read_back_diagnostic(id, reason);
        CreateCompletion::Partial {
            identifier,
            attributes,
            reason,
            missing_attributes,
        }
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

    pub(crate) fn sent_only(resource: &Resource, names: &[&str]) -> Sent {
        Sent::only(
            names
                .iter()
                .copied()
                .filter(|name| resource.attributes.contains_key(*name)),
        )
    }

    fn finish_create<A: AwsCreateAttempt>(
        &self,
        attempt: A,
        sent: Sent,
        result: ProviderResult<CreateCompletion>,
    ) -> Result<A::Step, A::Verdict> {
        match result {
            Ok(CreateCompletion::Success {
                identifier,
                attributes,
            }) => Ok(attempt.issued(identifier, sent).succeeded(attributes)),
            Ok(CreateCompletion::Partial {
                identifier,
                attributes,
                reason,
                missing_attributes,
            }) => {
                Ok(attempt
                    .issued(identifier, sent)
                    .partial(attributes, reason, missing_attributes))
            }
            Err(error) => Err(attempt.untracked(error)),
        }
    }

    pub(crate) fn finish_issued<I: AwsIssuedCreate>(
        &self,
        issued: I,
        completion: CreateCompletion,
    ) -> I::Step {
        match completion {
            CreateCompletion::Success {
                identifier: _identifier,
                attributes,
            } => issued.succeeded(attributes),
            CreateCompletion::Partial {
                identifier: _identifier,
                attributes,
                reason,
                missing_attributes,
            } => issued.partial(attributes, reason, missing_attributes),
        }
    }

    pub(crate) async fn failed_issued<I: AwsIssuedCreate>(
        &self,
        id: &ResourceId,
        issued: I,
        error: ProviderError,
    ) -> I::Verdict {
        let identifier = issued.identifier().clone();
        let observation = match Provider::read(self, id, &identifier, ReadRequest).await {
            Ok(outcome) => Observation::Read(outcome),
            Err(read_error) => Observation::ReadFailed(read_error),
        };
        issued.failed(observation, error)
    }

    /// Perform the initial, non-blocking create step.
    pub async fn create_step<A: AwsCreateAttempt>(
        &self,
        resource: &Resource,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        let schema = match self.resource_schema(resource.id.resource_type.as_str()) {
            Some(schema) => schema,
            None => {
                return Err(attempt.untracked(
                    ProviderError::internal(format!(
                        "Unknown resource schema: {}",
                        resource.id.resource_type
                    ))
                    .for_resource(resource.id.clone()),
                ));
            }
        };
        match resource.id.resource_type.as_str() {
            "ec2.NatGateway" => {
                let identifier = match self.create_ec2_nat_gateway_start(resource, schema).await {
                    Ok(identifier) => identifier,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                let sent = Self::sent_only(
                    resource,
                    &["subnet_id", "allocation_id", "connectivity_type"],
                );
                attempt.issued(identifier.clone(), sent).pending(
                    AwsCreateToken::NatGateway {
                        nat_gateway_id: identifier,
                    },
                    POLL_AFTER_SECS,
                )
            }
            "ec2.TransitGateway" => {
                let identifier = match self
                    .create_ec2_transit_gateway_start(resource, schema)
                    .await
                {
                    Ok(identifier) => identifier,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                attempt
                    .issued(identifier.clone(), Sent::AllRequestAttributes)
                    .pending(
                        AwsCreateToken::TransitGateway {
                            transit_gateway_id: identifier,
                        },
                        POLL_AFTER_SECS,
                    )
            }
            "ec2.TransitGatewayAttachment" => {
                let identifier = match self
                    .create_ec2_transit_gateway_attachment_start(resource, schema)
                    .await
                {
                    Ok(identifier) => identifier,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                attempt
                    .issued(identifier.clone(), Sent::AllRequestAttributes)
                    .pending(
                        AwsCreateToken::TransitGatewayAttachment {
                            attachment_id: identifier,
                        },
                        POLL_AFTER_SECS,
                    )
            }
            "organizations.Account" => {
                let request_id = match self
                    .create_organizations_account_start(resource, schema)
                    .await
                {
                    Ok(request_id) => request_id,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                let sent = Self::sent_only(
                    resource,
                    &[
                        "account_name",
                        "email",
                        "iam_user_access_to_billing",
                        "role_name",
                        "tags",
                    ],
                );
                attempt.pending(
                    AwsCreateToken::OrganizationsAccount {
                        create_account_request_id: request_id,
                    },
                    sent,
                    POLL_AFTER_SECS,
                )
            }
            "acm.Certificate" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_acm_certificate(resource, schema).await,
            ),
            "s3.Bucket" => self.create_s3_bucket(resource, attempt).await,
            "s3.BucketPolicy" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_policy(resource).await,
            ),
            "s3.BucketPublicAccessBlock" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_public_access_block(resource).await,
            ),
            "s3.BucketVersioning" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_versioning(resource, schema).await,
            ),
            "s3.BucketObjectLockConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_object_lock_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketServerSideEncryptionConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_server_side_encryption_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketAcl" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_acl(resource, schema).await,
            ),
            "s3.BucketOwnershipControls" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_ownership_controls(resource, schema)
                    .await,
            ),
            "s3.BucketReplicationConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_replication_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketLifecycleConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_lifecycle_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketWebsiteConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_website_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketCorsConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_cors_configuration(resource).await,
            ),
            "s3.BucketNotificationConfiguration" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_notification_configuration(resource)
                    .await,
            ),
            "s3.BucketLogging" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_s3_bucket_logging(resource, schema).await,
            ),
            "ec2.Eip" => self.create_ec2_eip(resource, schema, attempt).await,
            "ec2.Vpc" => self.create_ec2_vpc(resource, schema, attempt).await,
            "ec2.Subnet" => self.create_ec2_subnet(resource, schema, attempt).await,
            "ec2.InternetGateway" => self.create_ec2_internet_gateway(resource, attempt).await,
            "ec2.RouteTable" => self.create_ec2_route_table(resource, attempt).await,
            "ec2.Route" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_route(resource).await,
            ),
            "ec2.SecurityGroup" => self.create_ec2_security_group(resource, attempt).await,
            "ec2.SecurityGroupIngress" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_security_group_ingress(resource, schema)
                    .await,
            ),
            "ec2.SecurityGroupEgress" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_security_group_egress(resource, schema)
                    .await,
            ),
            "ec2.SubnetRouteTableAssociation" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_subnet_route_table_association(resource)
                    .await,
            ),
            "ec2.FlowLog" => self.create_ec2_flow_log(resource, schema, attempt).await,
            "ec2.VpcEndpoint" => {
                self.create_ec2_vpc_endpoint(resource, schema, attempt)
                    .await
            }
            "ec2.VpcGatewayAttachment" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_vpc_gateway_attachment(resource).await,
            ),
            "ec2.VpnGateway" => self.create_ec2_vpn_gateway(resource, schema, attempt).await,
            "ec2.VpcPeeringConnection" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_vpc_peering_connection(resource).await,
            ),
            "ec2.EgressOnlyInternetGateway" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_ec2_egress_only_internet_gateway(resource).await,
            ),
            "organizations.Organization" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_organizations_organization(resource, schema)
                    .await,
            ),
            "iam.Role" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_iam_role(resource).await,
            ),
            "logs.LogGroup" => self.create_logs_log_group(resource, schema, attempt).await,
            "route53.RecordSet" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_route53_record_set(resource, schema).await,
            ),
            "sqs.Queue" => self.finish_create(
                attempt,
                Sent::AllRequestAttributes,
                self.create_sqs_queue(resource, schema).await,
            ),
            unknown => Err(attempt.untracked(
                ProviderError::internal(format!("Unknown resource type: {unknown}"))
                    .for_resource(resource.id.clone()),
            )),
        }
    }

    /// Perform one describe step for a pending create operation.
    pub async fn poll_create_step<U: AwsPollAttempt>(
        &self,
        resource: &Resource,
        token: AwsCreateToken,
        poll: AwsCreatePoll<U, U::Issued>,
    ) -> Result<U::Step, U::Verdict> {
        let id = &resource.id;
        match token {
            AwsCreateToken::NatGateway { nat_gateway_id } => {
                let token = AwsCreateToken::NatGateway {
                    nat_gateway_id: nat_gateway_id.clone(),
                };
                let issued = match poll {
                    AwsCreatePoll::Unissued(attempt) => attempt.issued(nat_gateway_id.clone()),
                    AwsCreatePoll::Issued(issued) => issued,
                };
                let status = match self
                    .poll_ec2_nat_gateway_create(id, nat_gateway_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return issued.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        if let Err(error) = self
                            .apply_ec2_tags(
                                id,
                                nat_gateway_id.as_str(),
                                &resource.resolved_attributes(),
                                None,
                            )
                            .await
                        {
                            return Err(self.failed_issued(id, issued, error).await);
                        }
                        let outcome = self.read_ec2_nat_gateway(id, &nat_gateway_id).await;
                        let completion = self.create_outcome_from_read(
                            id,
                            nat_gateway_id,
                            resource.resolved_attributes(),
                            outcome,
                        );
                        Ok(self.finish_issued(issued, completion))
                    }
                    CreatePoll::Failed => Err(issued.discarded(
                        ProviderError::api_error("NAT gateway creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => issued.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::TransitGateway { transit_gateway_id } => {
                let token = AwsCreateToken::TransitGateway {
                    transit_gateway_id: transit_gateway_id.clone(),
                };
                let issued = match poll {
                    AwsCreatePoll::Unissued(attempt) => attempt.issued(transit_gateway_id.clone()),
                    AwsCreatePoll::Issued(issued) => issued,
                };
                let status = match self
                    .poll_ec2_transit_gateway_create(id, transit_gateway_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return issued.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        let outcome = self.read_ec2_transit_gateway(id, &transit_gateway_id).await;
                        let completion = self.create_outcome_from_read(
                            id,
                            transit_gateway_id,
                            resource.resolved_attributes(),
                            outcome,
                        );
                        Ok(self.finish_issued(issued, completion))
                    }
                    CreatePoll::Failed => Err(issued.discarded(
                        ProviderError::api_error("Transit gateway creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => issued.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::TransitGatewayAttachment { attachment_id } => {
                let token = AwsCreateToken::TransitGatewayAttachment {
                    attachment_id: attachment_id.clone(),
                };
                let issued = match poll {
                    AwsCreatePoll::Unissued(attempt) => attempt.issued(attachment_id.clone()),
                    AwsCreatePoll::Issued(issued) => issued,
                };
                let status = match self
                    .poll_ec2_transit_gateway_attachment_create(id, attachment_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return issued.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        let outcome = self
                            .read_ec2_transit_gateway_attachment(id, &attachment_id)
                            .await;
                        let completion = self.create_outcome_from_read(
                            id,
                            attachment_id,
                            resource.resolved_attributes(),
                            outcome,
                        );
                        Ok(self.finish_issued(issued, completion))
                    }
                    CreatePoll::Failed => Err(issued.discarded(
                        ProviderError::api_error("Transit gateway attachment creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => issued.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::OrganizationsAccount {
                create_account_request_id,
            } => {
                let token = AwsCreateToken::OrganizationsAccount {
                    create_account_request_id: create_account_request_id.clone(),
                };
                let status = match self
                    .poll_organizations_account_create(id, &create_account_request_id)
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => {
                        return match poll {
                            AwsCreatePoll::Unissued(attempt) => {
                                attempt.pending(token, POLL_AFTER_SECS)
                            }
                            AwsCreatePoll::Issued(issued) => issued.pending(token, POLL_AFTER_SECS),
                        };
                    }
                };
                match status {
                    CreatePoll::Ready(Some(account_id)) => {
                        let identifier = match crate::helpers::applied_identifier(
                            id,
                            "organizations.CreateAccount",
                            account_id,
                        ) {
                            Ok(identifier) => identifier,
                            Err(error) => {
                                return match poll {
                                    AwsCreatePoll::Unissued(attempt) => {
                                        Err(attempt.untracked(error))
                                    }
                                    AwsCreatePoll::Issued(issued) => {
                                        Err(self.failed_issued(id, issued, error).await)
                                    }
                                };
                            }
                        };
                        let issued = match poll {
                            AwsCreatePoll::Unissued(attempt) => attempt.issued(identifier),
                            AwsCreatePoll::Issued(issued) => issued,
                        };
                        match self
                            .finish_organizations_account_create(
                                resource,
                                issued.identifier().as_str(),
                            )
                            .await
                        {
                            Ok(completion) => Ok(self.finish_issued(issued, completion)),
                            Err(error) => Err(self.failed_issued(id, issued, error).await),
                        }
                    }
                    CreatePoll::Ready(None) => {
                        let error = ProviderError::api_error(
                            "CreateAccount succeeded but no account ID returned",
                        )
                        .for_resource(id.clone());
                        match poll {
                            AwsCreatePoll::Unissued(attempt) => Err(attempt.untracked(error)),
                            AwsCreatePoll::Issued(issued) => {
                                Err(self.failed_issued(id, issued, error).await)
                            }
                        }
                    }
                    CreatePoll::Failed => match poll {
                        AwsCreatePoll::Unissued(attempt) => Err(attempt.untracked(
                            ProviderError::api_error("Account creation failed")
                                .for_resource(id.clone()),
                        )),
                        AwsCreatePoll::Issued(issued) => Err(issued.discarded(
                            ProviderError::api_error("Account creation failed")
                                .for_resource(id.clone()),
                        )),
                    },
                    CreatePoll::Pending => match poll {
                        AwsCreatePoll::Unissued(attempt) => attempt.pending(token, POLL_AFTER_SECS),
                        AwsCreatePoll::Issued(issued) => issued.pending(token, POLL_AFTER_SECS),
                    },
                }
            }
        }
    }

    async fn drive_native_create(
        &self,
        resource: &Resource,
        attempt: CreateAttempt,
    ) -> Result<CreateOutcome, CreateFailure> {
        let budget =
            self.operation_budget(resource.id.resource_type.as_str(), OperationKind::Create);
        let started = std::time::Instant::now();
        let mut step = self.create_step(resource, attempt).await?;
        loop {
            let pending = match step {
                CoreCreateStep::Done(outcome) => return Ok(outcome),
                CoreCreateStep::Pending(pending) => pending,
            };
            if started.elapsed() >= budget {
                return Err(pending.preserve(
                    ProviderError::timeout(format!(
                        "create of {} did not finish within {budget:?}",
                        resource.id.resource_type
                    ))
                    .for_resource(resource.id.clone()),
                ));
            }
            let remaining = budget.saturating_sub(started.elapsed());
            let delay = Duration::from_secs(u64::from(pending.poll_after_secs().clamp(1, 300)))
                .min(remaining);
            tokio::time::sleep(delay).await;
            let token = match decode_create_token(pending.token()) {
                Ok(token) => token,
                Err(error) => return Err(pending.preserve(error)),
            };
            let poll = match pending.into_poll() {
                Poll::Unissued(attempt) => AwsCreatePoll::Unissued(attempt),
                Poll::Issued(issued) => AwsCreatePoll::Issued(issued),
            };
            step = self.poll_create_step(resource, token, poll).await?;
        }
    }

    async fn failed_resumed<R: AwsResumedCreate>(
        &self,
        id: &ResourceId,
        resumed: R,
        error: ProviderError,
    ) -> R::Verdict {
        let identifier = resumed.identifier().clone();
        let observation = match Provider::read(self, id, &identifier, ReadRequest).await {
            Ok(outcome) => Observation::Read(outcome),
            Err(read_error) => Observation::ReadFailed(read_error),
        };
        resumed.failed(observation, error)
    }

    fn finish_resumed_completion<R: AwsResumedCreate>(
        &self,
        id: &ResourceId,
        resumed: R,
        completion: CreateCompletion,
    ) -> Result<R::Step, R::Verdict> {
        match completion {
            CreateCompletion::Success {
                identifier,
                attributes,
            } => Ok(resumed.completed(AppliedState::from_identifier(
                id.clone(),
                identifier,
                attributes,
            ))),
            CreateCompletion::Partial {
                identifier: _identifier,
                attributes: _attributes,
                reason,
                missing_attributes: _missing_attributes,
            } => {
                let observation_error =
                    ProviderError::api_error(reason.clone()).for_resource(id.clone());
                Err(resumed.failed(
                    Observation::ReadFailed(observation_error),
                    ProviderError::api_error(reason).for_resource(id.clone()),
                ))
            }
        }
    }

    async fn complete_resumed_read<R: AwsResumedCreate>(
        &self,
        resumed: R,
        token: AwsCreateToken,
        outcome: ProviderResult<ReadOutcome>,
    ) -> Result<R::Step, R::Verdict> {
        // Issued EC2 creates can be briefly absent immediately after the
        // create response. The service-specific measurements at the EC2 read
        // and poll sites show that absence is reported either as an empty list
        // or a NotFound error. Keep both (and other transient read failures)
        // pending during resume; discarding the persisted record would let the
        // next apply issue a duplicate create.
        match outcome {
            Ok(ReadOutcome::Found { state }) => Ok(resumed.completed(state)),
            Ok(ReadOutcome::NotFound { id: _ }) | Err(_) => resumed.pending(token, POLL_AFTER_SECS),
        }
    }

    /// Perform one step while resuming a create loaded from state.
    pub async fn resume_create_step<U: AwsResumeAttempt>(
        &self,
        resource: &Resource,
        token: AwsCreateToken,
        resume: AwsCreateResume<U, U::Resumed>,
    ) -> Result<U::Step, U::Verdict> {
        let id = &resource.id;
        match token {
            AwsCreateToken::NatGateway { nat_gateway_id } => {
                let token = AwsCreateToken::NatGateway {
                    nat_gateway_id: nat_gateway_id.clone(),
                };
                let resumed = match resume {
                    AwsCreateResume::Unissued(attempt) => attempt.issued(nat_gateway_id.clone()),
                    AwsCreateResume::Issued(resumed) => resumed,
                };
                let status = match self
                    .poll_ec2_nat_gateway_create(id, nat_gateway_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return resumed.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        if let Err(error) = self
                            .apply_ec2_tags(
                                id,
                                nat_gateway_id.as_str(),
                                &resource.resolved_attributes(),
                                None,
                            )
                            .await
                        {
                            return Err(self.failed_resumed(id, resumed, error).await);
                        }
                        let outcome = self.read_ec2_nat_gateway(id, &nat_gateway_id).await;
                        self.complete_resumed_read(resumed, token, outcome).await
                    }
                    CreatePoll::Failed => Err(resumed.discarded(
                        ProviderError::api_error("NAT gateway creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => resumed.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::TransitGateway { transit_gateway_id } => {
                let token = AwsCreateToken::TransitGateway {
                    transit_gateway_id: transit_gateway_id.clone(),
                };
                let resumed = match resume {
                    AwsCreateResume::Unissued(attempt) => {
                        attempt.issued(transit_gateway_id.clone())
                    }
                    AwsCreateResume::Issued(resumed) => resumed,
                };
                let status = match self
                    .poll_ec2_transit_gateway_create(id, transit_gateway_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return resumed.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        let outcome = self.read_ec2_transit_gateway(id, &transit_gateway_id).await;
                        self.complete_resumed_read(resumed, token, outcome).await
                    }
                    CreatePoll::Failed => Err(resumed.discarded(
                        ProviderError::api_error("Transit gateway creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => resumed.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::TransitGatewayAttachment { attachment_id } => {
                let token = AwsCreateToken::TransitGatewayAttachment {
                    attachment_id: attachment_id.clone(),
                };
                let resumed = match resume {
                    AwsCreateResume::Unissued(attempt) => attempt.issued(attachment_id.clone()),
                    AwsCreateResume::Issued(resumed) => resumed,
                };
                let status = match self
                    .poll_ec2_transit_gateway_attachment_create(id, attachment_id.as_str())
                    .await
                {
                    Ok(status) => status,
                    Err(_error) => return resumed.pending(token, POLL_AFTER_SECS),
                };
                match status {
                    CreatePoll::Ready(()) => {
                        let outcome = self
                            .read_ec2_transit_gateway_attachment(id, &attachment_id)
                            .await;
                        self.complete_resumed_read(resumed, token, outcome).await
                    }
                    CreatePoll::Failed => Err(resumed.discarded(
                        ProviderError::api_error("Transit gateway attachment creation failed")
                            .for_resource(id.clone()),
                    )),
                    CreatePoll::Pending => resumed.pending(token, POLL_AFTER_SECS),
                }
            }
            AwsCreateToken::OrganizationsAccount {
                create_account_request_id,
            } => {
                let token = AwsCreateToken::OrganizationsAccount {
                    create_account_request_id: create_account_request_id.clone(),
                };
                let status = match self
                    .poll_organizations_account_create(id, &create_account_request_id)
                    .await
                {
                    Ok(status) => status,
                    Err(error) => {
                        return match resume {
                            AwsCreateResume::Unissued(attempt) => Err(attempt.undetermined(error)),
                            AwsCreateResume::Issued(resumed) => {
                                resumed.pending(token, POLL_AFTER_SECS)
                            }
                        };
                    }
                };
                match status {
                    CreatePoll::Ready(Some(account_id)) => {
                        let identifier = match crate::helpers::applied_identifier(
                            id,
                            "organizations.CreateAccount",
                            account_id,
                        ) {
                            Ok(identifier) => identifier,
                            Err(error) => {
                                return match resume {
                                    AwsCreateResume::Unissued(attempt) => {
                                        Err(attempt.untracked(error))
                                    }
                                    AwsCreateResume::Issued(resumed) => {
                                        Err(self.failed_resumed(id, resumed, error).await)
                                    }
                                };
                            }
                        };
                        let resumed = match resume {
                            AwsCreateResume::Unissued(attempt) => attempt.issued(identifier),
                            AwsCreateResume::Issued(resumed) => resumed,
                        };
                        match self
                            .finish_organizations_account_create(
                                resource,
                                resumed.identifier().as_str(),
                            )
                            .await
                        {
                            Ok(completion) => {
                                self.finish_resumed_completion(id, resumed, completion)
                            }
                            Err(error) => Err(self.failed_resumed(id, resumed, error).await),
                        }
                    }
                    CreatePoll::Ready(None) => {
                        let error = ProviderError::api_error(
                            "CreateAccount succeeded but no account ID returned",
                        )
                        .for_resource(id.clone());
                        match resume {
                            AwsCreateResume::Unissued(attempt) => Err(attempt.untracked(error)),
                            AwsCreateResume::Issued(resumed) => {
                                Err(self.failed_resumed(id, resumed, error).await)
                            }
                        }
                    }
                    CreatePoll::Failed => match resume {
                        AwsCreateResume::Unissued(attempt) => Err(attempt.untracked(
                            ProviderError::api_error("Account creation failed")
                                .for_resource(id.clone()),
                        )),
                        AwsCreateResume::Issued(resumed) => Err(resumed.discarded(
                            ProviderError::api_error("Account creation failed")
                                .for_resource(id.clone()),
                        )),
                    },
                    CreatePoll::Pending => match resume {
                        AwsCreateResume::Unissued(attempt) => {
                            attempt.pending(token, POLL_AFTER_SECS)
                        }
                        AwsCreateResume::Issued(resumed) => resumed.pending(token, POLL_AFTER_SECS),
                    },
                }
            }
        }
    }

    async fn drive_native_resume(
        &self,
        request: ResumeCreateRequest,
    ) -> Result<ResumeOutcome, ResumeFailure> {
        let budget =
            self.operation_budget(request.id().resource_type.as_str(), OperationKind::Create);
        let started = std::time::Instant::now();
        let (resume, create_request) = request.into_parts();
        let resource = create_request.resource().as_resource().clone();
        let token = match decode_create_token(resume.token()) {
            Ok(token) => token,
            Err(error) => return Err(resume.preserve(error)),
        };
        let resume = match resume {
            Resume::Unissued(attempt) => AwsCreateResume::Unissued(attempt),
            Resume::Issued(resumed) => AwsCreateResume::Issued(resumed),
        };
        let mut step = self.resume_create_step(&resource, token, resume).await?;
        loop {
            let next = match step {
                CoreResumeStep::Done(outcome) => return Ok(outcome),
                CoreResumeStep::Pending(resume) => *resume,
            };
            if started.elapsed() >= budget {
                return Err(next.preserve(
                    ProviderError::timeout(format!(
                        "create of {} did not finish within {budget:?}",
                        resource.id.resource_type
                    ))
                    .for_resource(resource.id.clone()),
                ));
            }
            let remaining = budget.saturating_sub(started.elapsed());
            let delay = Duration::from_secs(u64::from(POLL_AFTER_SECS)).min(remaining);
            tokio::time::sleep(delay).await;
            let token = match decode_create_token(next.token()) {
                Ok(token) => token,
                Err(error) => return Err(next.preserve(error)),
            };
            let resume = match next {
                Resume::Unissued(attempt) => AwsCreateResume::Unissued(attempt),
                Resume::Issued(resumed) => AwsCreateResume::Issued(resumed),
            };
            step = self.resume_create_step(&resource, token, resume).await?;
        }
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
        attempt: CreateAttempt,
    ) -> BoxFuture<'_, Result<CreateOutcome, CreateFailure>> {
        Box::pin(async move {
            self.drive_native_create(request.resource().as_resource(), attempt)
                .await
        })
    }

    fn resume_create(
        &self,
        request: ResumeCreateRequest,
    ) -> BoxFuture<'_, Result<ResumeOutcome, ResumeFailure>> {
        Box::pin(async move { self.drive_native_resume(request).await })
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
        let mut done = Some(expected.clone());

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

        assert_eq!(outcome, expected);
    }

    #[tokio::test]
    async fn native_create_driver_maps_over_budget_to_resource_timeout() {
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "example", None);
        let result: ProviderResult<()> = drive_operation(
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
