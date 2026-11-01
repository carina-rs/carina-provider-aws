//! Provider trait implementation for AWS

use std::collections::HashMap;
use std::future::Future;
use std::num::NonZeroU8;
use std::time::Duration;

use carina_aws_types::SendClassification;
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
    CompletedOperation, DEFAULT_OPERATION_BUDGET, DEFAULT_READ_BACK_BUDGET, DriveError,
    DriveOutcome, OperationKind, OperationStep, PendingPhase, PollFinality, PollRequest,
    drive_pending, drive_pending_infallible,
};
use serde::{Deserialize, Serialize};

use crate::AwsProvider;
use crate::aws_operation::{ClientToken, DecidedOperationError, EC2_CREATE_TOKEN_TAG};
use crate::error_helpers::{IntoSendFailure, SendFailure};
use crate::helpers::{CreatePoll, DeletePoll, apply_patch_to_state, provider_retry_allowed};
use crate::read_back::{
    NonEmptyAttributeNames, ReadBackCompletion, ReadBackContinuation, ReadBackKind,
    finish_final_read_back, invalid_read_back,
};

pub(crate) const POLL_AFTER_SECS: u32 = 5;
/// Four describe attempts after delays totaling 65 seconds. Each paginated
/// describe is capped at two pages, because in production WASM an individual
/// synchronous HTTP call can take about 90 seconds before the host observes a
/// timeout. The initial create plus eight describe-page calls therefore cost
/// at most 9 * 90 = 810 seconds; adding 65 seconds of sleeps gives 875 seconds,
/// below the host's 900-second default operation budget. Once discovery finds
/// an identifier, continuation work is returned as another pending step so a
/// later WASM poll receives a fresh operation budget.
pub(crate) const TAGGED_EC2_DISCOVERY_ATTEMPTS: u8 = 4;
pub(crate) const TAGGED_EC2_DISCOVERY_MAX_PAGES: u8 = 2;
pub(crate) const TAGGED_EC2_INITIAL_DELAY_SECS: u32 = 5;
const ACM_REQUEST_CERTIFICATE_POLL_ATTEMPTS: NonZeroU8 = NonZeroU8::new(3).unwrap();
const ACM_IDEMPOTENCY_WINDOW_SECS: u64 = 60 * 60;
// This margin is the cross-host clock-skew budget between the process that
// first sends the token and a later process that polls or resumes it.
const ACM_IDEMPOTENCY_SAFETY_MARGIN_SECS: u64 = 15 * 60;
const ACM_REQUEST_CERTIFICATE_OUTCOME_UNKNOWN: &str =
    "acm.Certificate may have been created; check before re-applying";
const ACM_REQUEST_CERTIFICATE_ISSUED_HANDLE_ERROR: &str =
    "ACM request-certificate token cannot be paired with an issued create handle";

fn next_tagged_ec2_delay(current: u32) -> u32 {
    match current {
        0..=4 => 5,
        5..=9 => 10,
        10..=19 => 20,
        20..=29 => 30,
        _ => 60,
    }
}

fn describe_ids_from_pages<'a, Page: 'a, Item: 'a>(
    pages: &'a [Page],
    items: impl Fn(&'a Page) -> &'a [Item],
    identifier: impl Fn(&'a Item) -> Option<&'a str>,
) -> Vec<String> {
    let mut identifiers = Vec::new();
    for page in pages {
        identifiers.extend(
            items(page)
                .iter()
                .filter_map(|resource| identifier(resource).map(str::to_owned)),
        );
    }
    identifiers
}

fn unix_timestamp_now() -> ProviderResult<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| {
            ProviderError::internal_caused_by("system clock is before Unix epoch", error)
        })
}

fn acm_client_token_can_be_resent(first_used_at_unix_secs: u64, now_unix_secs: u64) -> bool {
    now_unix_secs
        .checked_sub(first_used_at_unix_secs)
        .is_some_and(|elapsed| {
            elapsed < ACM_IDEMPOTENCY_WINDOW_SECS - ACM_IDEMPOTENCY_SAFETY_MARGIN_SECS
        })
}

/// Proof that the ACM idempotency token is still inside the conservative
/// resend window at the clock reading used by this transition.
struct FreshAcmClientToken {
    client_token: ClientToken,
    first_used_at_unix_secs: u64,
}

fn fresh_acm_client_token(
    client_token: ClientToken,
    first_used_at_unix_secs: u64,
    now_unix_secs: u64,
) -> Option<FreshAcmClientToken> {
    acm_client_token_can_be_resent(first_used_at_unix_secs, now_unix_secs).then_some(
        FreshAcmClientToken {
            client_token,
            first_used_at_unix_secs,
        },
    )
}

fn acm_request_certificate_outcome_unknown(id: &ResourceId) -> ProviderError {
    ProviderError::api_error(ACM_REQUEST_CERTIFICATE_OUTCOME_UNKNOWN).for_resource(id.clone())
}

fn acm_request_certificate_issued_handle_error(id: &ResourceId) -> ProviderError {
    ProviderError::internal(ACM_REQUEST_CERTIFICATE_ISSUED_HANDLE_ERROR).for_resource(id.clone())
}

/// Secret-free AWS metadata retained when a create's send outcome is ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwsFailureSummary {
    operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

impl AwsFailureSummary {
    fn from_send_failure<E>(failure: &SendFailure<E>) -> Self
    where
        E: DecidedOperationError
            + aws_smithy_types::error::metadata::ProvideErrorMetadata
            + std::error::Error
            + 'static,
    {
        use aws_smithy_types::error::metadata::ProvideErrorMetadata;
        use aws_types::request_id::RequestId;

        Self {
            operation: E::NAME.to_string(),
            status: failure
                .raw_response()
                .map(|response| response.status().as_u16()),
            code: failure.code().map(str::to_owned),
            request_id: failure.request_id().map(str::to_owned),
        }
    }
}

impl std::fmt::Display for AwsFailureSummary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "operation={}", self.operation)?;
        if let Some(status) = self.status {
            write!(formatter, ", status={status}")?;
        }
        if let Some(code) = &self.code {
            write!(formatter, ", code={code}")?;
        }
        if let Some(request_id) = &self.request_id {
            write!(formatter, ", request_id={request_id}")?;
        }
        Ok(())
    }
}

/// Continuation phases for AWS create operations that wait on cloud state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AwsCreateToken {
    ReadBack(ReadBackContinuation),
    AcmRequestCertificate {
        client_token: ClientToken,
        first_used_at_unix_secs: u64,
        attempts_remaining: u8,
    },
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
    TaggedEc2 {
        kind: Ec2TaggedResourceKind,
        create_token: ClientToken,
        attempts_remaining: u8,
        next_delay_secs: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_failure: Option<AwsFailureSummary>,
    },
    TaggedEc2Found {
        kind: Ec2TaggedResourceKind,
        identifier: AppliedIdentifier,
    },
}

impl AwsCreateToken {
    pub fn phase(&self) -> PendingPhase {
        match self {
            Self::ReadBack(_) => PendingPhase::ReadBack,
            Self::AcmRequestCertificate { .. }
            | Self::NatGateway { .. }
            | Self::TransitGateway { .. }
            | Self::TransitGatewayAttachment { .. }
            | Self::OrganizationsAccount { .. }
            | Self::TaggedEc2 { .. }
            | Self::TaggedEc2Found { .. } => PendingPhase::Operation,
        }
    }

    /// Poll cadence that belongs to this exact continuation.
    pub fn poll_after_secs(&self) -> u32 {
        match self {
            Self::ReadBack(_)
            | Self::AcmRequestCertificate { .. }
            | Self::TaggedEc2Found { .. }
            | Self::NatGateway { .. }
            | Self::TransitGateway { .. }
            | Self::TransitGatewayAttachment { .. }
            | Self::OrganizationsAccount { .. } => POLL_AFTER_SECS,
            Self::TaggedEc2 {
                next_delay_secs, ..
            } => *next_delay_secs,
        }
    }
}

/// Update continuations distinguish unsettled mutation work from read-back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AwsUpdateToken {
    ReadBack(ReadBackContinuation),
}

pub type AwsUpdateStep = OperationStep<UpdateOutcome, ProviderError, AwsUpdateToken>;

/// EC2 resource kinds discoverable by the provider-managed create token tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ec2TaggedResourceKind {
    Vpc,
    Subnet,
    RouteTable,
    SecurityGroup,
    InternetGateway,
    EgressOnlyInternetGateway,
    VpnGateway,
    VpcPeeringConnection,
    Address,
    NatGateway,
    VpcEndpoint,
    FlowLog,
    TransitGateway,
    TransitGatewayVpcAttachment,
}

/// Inputs shared by every tag-discoverable EC2 create result handler.
pub(crate) struct TaggedEc2CreateContext<'a> {
    create: TaggedEc2Create,
    create_token: &'a ClientToken,
    sent: Sent,
}

/// A tagged EC2 create kind together with any values needed after the create
/// request is accepted. Variant construction makes kind/continuation mismatch
/// states unrepresentable.
pub(crate) enum TaggedEc2Create {
    Vpc {
        enable_dns_hostnames: Option<bool>,
        enable_dns_support: Option<bool>,
    },
    Subnet {
        assign_ipv6_address_on_creation: Option<bool>,
        enable_dns64: Option<bool>,
        enable_lni_at_device_index: Option<i64>,
        map_public_ip_on_launch: Option<bool>,
        private_dns_name_options_on_launch: Option<Value>,
    },
    InternetGateway {
        vpc_id: Option<String>,
    },
    RouteTable,
    SecurityGroup,
    EgressOnlyInternetGateway,
    VpnGateway,
    VpcPeeringConnection,
    Address,
    NatGateway,
    VpcEndpoint,
    FlowLog,
    TransitGateway,
    TransitGatewayVpcAttachment,
}

/// A resumed EC2 create only needs follow-up fields, but every generated
/// create part still has to cross the `Sent<T>` consumption boundary. Keeping
/// this as a generic helper avoids `drop(copy_value)` warnings while making
/// the intentional projection explicit at each named field.
fn consume_create_request_part_on_resume<T>(part: crate::send_model::Sent<T>) {
    drop(part.into_inner());
}

impl TaggedEc2Create {
    pub(crate) fn create_context(
        self,
        create_token: &ClientToken,
        sent: Sent,
    ) -> TaggedEc2CreateContext<'_> {
        TaggedEc2CreateContext {
            create: self,
            create_token,
            sent,
        }
    }

    fn kind(&self) -> Ec2TaggedResourceKind {
        match self {
            Self::Vpc { .. } => Ec2TaggedResourceKind::Vpc,
            Self::Subnet { .. } => Ec2TaggedResourceKind::Subnet,
            Self::InternetGateway { .. } => Ec2TaggedResourceKind::InternetGateway,
            Self::RouteTable => Ec2TaggedResourceKind::RouteTable,
            Self::SecurityGroup => Ec2TaggedResourceKind::SecurityGroup,
            Self::EgressOnlyInternetGateway => Ec2TaggedResourceKind::EgressOnlyInternetGateway,
            Self::VpnGateway => Ec2TaggedResourceKind::VpnGateway,
            Self::VpcPeeringConnection => Ec2TaggedResourceKind::VpcPeeringConnection,
            Self::Address => Ec2TaggedResourceKind::Address,
            Self::NatGateway => Ec2TaggedResourceKind::NatGateway,
            Self::VpcEndpoint => Ec2TaggedResourceKind::VpcEndpoint,
            Self::FlowLog => Ec2TaggedResourceKind::FlowLog,
            Self::TransitGateway => Ec2TaggedResourceKind::TransitGateway,
            Self::TransitGatewayVpcAttachment => Ec2TaggedResourceKind::TransitGatewayVpcAttachment,
        }
    }

    fn decode_resumed(
        kind: Ec2TaggedResourceKind,
        resource: &Resource<impl Sized>,
        registry: &carina_core::schema::SchemaRegistry,
    ) -> ProviderResult<Self> {
        let schema = |resource_type: &str| {
            registry
                .get(
                    "aws",
                    resource_type,
                    carina_core::schema::SchemaKind::Resource,
                )
                .map(|schema| schema.schema().as_schema())
                .ok_or_else(|| {
                    ProviderError::internal(format!("Unknown resource schema: {resource_type}"))
                        .for_resource(resource.id.clone())
                })
        };
        match kind {
            Ec2TaggedResourceKind::Vpc => {
                let input = crate::send_models_generated::Ec2VpcCreateInput::decode(
                    resource,
                    schema("ec2.Vpc")?,
                )?;
                let crate::send_models_generated::Ec2VpcCreateParts {
                    cidr_block,
                    enable_dns_hostnames,
                    enable_dns_support,
                    instance_tenancy,
                    ipv4_ipam_pool_id,
                    ipv4_netmask_length,
                    tags,
                } = input.into_parts();
                // Resume only needs the follow-up values. Explicitly consume
                // the create-request values so the Sent<T> guard still proves
                // that this projection was updated when the generated input grows.
                consume_create_request_part_on_resume(cidr_block);
                let enable_dns_hostnames = enable_dns_hostnames.into_inner();
                let enable_dns_support = enable_dns_support.into_inner();
                consume_create_request_part_on_resume(instance_tenancy);
                consume_create_request_part_on_resume(ipv4_ipam_pool_id);
                consume_create_request_part_on_resume(ipv4_netmask_length);
                consume_create_request_part_on_resume(tags);
                Ok(Self::Vpc {
                    enable_dns_hostnames,
                    enable_dns_support,
                })
            }
            Ec2TaggedResourceKind::Subnet => {
                let input = crate::send_models_generated::Ec2SubnetCreateInput::decode(
                    resource,
                    schema("ec2.Subnet")?,
                )?;
                let crate::send_models_generated::Ec2SubnetCreateParts {
                    assign_ipv6_address_on_creation,
                    availability_zone,
                    availability_zone_id,
                    cidr_block,
                    enable_dns64,
                    enable_lni_at_device_index,
                    ipv4_ipam_pool_id,
                    ipv4_netmask_length,
                    ipv6_cidr_block,
                    ipv6_ipam_pool_id,
                    ipv6_native,
                    ipv6_netmask_length,
                    map_public_ip_on_launch,
                    outpost_arn,
                    private_dns_name_options_on_launch,
                    tags,
                    vpc_id,
                } = input.into_parts();
                let assign_ipv6_address_on_creation = assign_ipv6_address_on_creation.into_inner();
                consume_create_request_part_on_resume(availability_zone);
                consume_create_request_part_on_resume(availability_zone_id);
                consume_create_request_part_on_resume(cidr_block);
                let enable_dns64 = enable_dns64.into_inner();
                let enable_lni_at_device_index = enable_lni_at_device_index.into_inner();
                consume_create_request_part_on_resume(ipv4_ipam_pool_id);
                consume_create_request_part_on_resume(ipv4_netmask_length);
                consume_create_request_part_on_resume(ipv6_cidr_block);
                consume_create_request_part_on_resume(ipv6_ipam_pool_id);
                consume_create_request_part_on_resume(ipv6_native);
                consume_create_request_part_on_resume(ipv6_netmask_length);
                let map_public_ip_on_launch = map_public_ip_on_launch.into_inner();
                consume_create_request_part_on_resume(outpost_arn);
                let private_dns_name_options_on_launch =
                    private_dns_name_options_on_launch.into_inner();
                consume_create_request_part_on_resume(tags);
                consume_create_request_part_on_resume(vpc_id);
                Ok(Self::Subnet {
                    assign_ipv6_address_on_creation,
                    enable_dns64,
                    enable_lni_at_device_index,
                    map_public_ip_on_launch,
                    private_dns_name_options_on_launch,
                })
            }
            Ec2TaggedResourceKind::InternetGateway => {
                let input =
                    crate::send_models_generated::Ec2InternetGatewayCreateInput::decode(resource)?;
                let crate::send_models_generated::Ec2InternetGatewayCreateParts { tags, vpc_id } =
                    input.into_parts();
                consume_create_request_part_on_resume(tags);
                Ok(Self::InternetGateway {
                    vpc_id: vpc_id.into_inner(),
                })
            }
            Ec2TaggedResourceKind::RouteTable => Ok(Self::RouteTable),
            Ec2TaggedResourceKind::SecurityGroup => Ok(Self::SecurityGroup),
            Ec2TaggedResourceKind::EgressOnlyInternetGateway => Ok(Self::EgressOnlyInternetGateway),
            Ec2TaggedResourceKind::VpnGateway => Ok(Self::VpnGateway),
            Ec2TaggedResourceKind::VpcPeeringConnection => Ok(Self::VpcPeeringConnection),
            Ec2TaggedResourceKind::Address => Ok(Self::Address),
            Ec2TaggedResourceKind::NatGateway => Ok(Self::NatGateway),
            Ec2TaggedResourceKind::VpcEndpoint => Ok(Self::VpcEndpoint),
            Ec2TaggedResourceKind::FlowLog => Ok(Self::FlowLog),
            Ec2TaggedResourceKind::TransitGateway => Ok(Self::TransitGateway),
            Ec2TaggedResourceKind::TransitGatewayVpcAttachment => {
                Ok(Self::TransitGatewayVpcAttachment)
            }
        }
    }
}

impl Ec2TaggedResourceKind {
    fn create_operation(self) -> &'static str {
        match self {
            Self::Vpc => "ec2.CreateVpc",
            Self::Subnet => "ec2.CreateSubnet",
            Self::RouteTable => "ec2.CreateRouteTable",
            Self::SecurityGroup => "ec2.CreateSecurityGroup",
            Self::InternetGateway => "ec2.CreateInternetGateway",
            Self::EgressOnlyInternetGateway => "ec2.CreateEgressOnlyInternetGateway",
            Self::VpnGateway => "ec2.CreateVpnGateway",
            Self::VpcPeeringConnection => "ec2.CreateVpcPeeringConnection",
            Self::Address => "ec2.AllocateAddress",
            Self::NatGateway => "ec2.CreateNatGateway",
            Self::VpcEndpoint => "ec2.CreateVpcEndpoint",
            Self::FlowLog => "ec2.CreateFlowLogs",
            Self::TransitGateway => "ec2.CreateTransitGateway",
            Self::TransitGatewayVpcAttachment => "ec2.CreateTransitGatewayVpcAttachment",
        }
    }

    fn resource_type(self) -> &'static str {
        match self {
            Self::Vpc => "ec2.Vpc",
            Self::Subnet => "ec2.Subnet",
            Self::RouteTable => "ec2.RouteTable",
            Self::SecurityGroup => "ec2.SecurityGroup",
            Self::InternetGateway => "ec2.InternetGateway",
            Self::EgressOnlyInternetGateway => "ec2.EgressOnlyInternetGateway",
            Self::VpnGateway => "ec2.VpnGateway",
            Self::VpcPeeringConnection => "ec2.VpcPeeringConnection",
            Self::Address => "ec2.Eip",
            Self::NatGateway => "ec2.NatGateway",
            Self::VpcEndpoint => "ec2.VpcEndpoint",
            Self::FlowLog => "ec2.FlowLog",
            Self::TransitGateway => "ec2.TransitGateway",
            Self::TransitGatewayVpcAttachment => "ec2.TransitGatewayAttachment",
        }
    }

    fn create_error_context(self) -> &'static str {
        match self {
            Self::Vpc => "Failed to create VPC",
            Self::Subnet => "Failed to create subnet",
            Self::RouteTable => "Failed to create route table",
            Self::SecurityGroup => "Failed to create security group",
            Self::InternetGateway => "Failed to create internet gateway",
            Self::EgressOnlyInternetGateway => "Failed to create egress-only internet gateway",
            Self::VpnGateway => "Failed to create VPN gateway",
            Self::VpcPeeringConnection => "Failed to create VPC peering connection",
            Self::Address => "Failed to allocate address",
            Self::NatGateway => "Failed to create NAT gateway",
            Self::VpcEndpoint => "Failed to create VPC endpoint",
            Self::FlowLog => "Failed to create flow logs",
            Self::TransitGateway => "Failed to create transit gateway",
            Self::TransitGatewayVpcAttachment => "Failed to create transit gateway VPC attachment",
        }
    }

    fn discovery_error_context(self) -> &'static str {
        match self {
            Self::Vpc => "Failed to discover VPC by create token",
            Self::Subnet => "Failed to discover subnet by create token",
            Self::RouteTable => "Failed to discover route table by create token",
            Self::SecurityGroup => "Failed to discover security group by create token",
            Self::InternetGateway => "Failed to discover internet gateway by create token",
            Self::EgressOnlyInternetGateway => {
                "Failed to discover egress-only internet gateway by create token"
            }
            Self::VpnGateway => "Failed to discover VPN gateway by create token",
            Self::VpcPeeringConnection => {
                "Failed to discover VPC peering connection by create token"
            }
            Self::Address => "Failed to discover address by create token",
            Self::NatGateway => "Failed to discover NAT gateway by create token",
            Self::VpcEndpoint => "Failed to discover VPC endpoint by create token",
            Self::FlowLog => "Failed to discover flow logs by create token",
            Self::TransitGateway => "Failed to discover transit gateway by create token",
            Self::TransitGatewayVpcAttachment => {
                "Failed to discover transit gateway attachment by create token"
            }
        }
    }

    /// Issue the kind-specific EC2 describe-by-tag request and normalize its
    /// identifier field. Keeping this dispatch beside the operation metadata
    /// leaves the discovery state machine independent of individual EC2 shapes.
    async fn describe_by_tag(
        self,
        provider: &AwsProvider,
        id: &ResourceId,
        filter: aws_sdk_ec2::types::Filter,
    ) -> ProviderResult<Vec<String>> {
        macro_rules! describe_ids {
            ($request:expr, $items:ident, $identifier:ident, paginated) => {{
                let mut stream = $request.into_paginator().send();
                let mut pages = Vec::new();
                for _ in 0..TAGGED_EC2_DISCOVERY_MAX_PAGES {
                    let Some(page) = stream.try_next().await.map_err(|error| {
                        AwsProvider::tagged_ec2_discovery_call_error(id, self, error)
                    })?
                    else {
                        break;
                    };
                    pages.push(page);
                }
                if pages
                    .last()
                    .and_then(|page| page.next_token())
                    .is_some_and(|token| !token.is_empty())
                {
                    return Err(ProviderError::api_error(format!(
                        "{} discovery exceeded the bounded {}-page scan",
                        self.create_operation(),
                        TAGGED_EC2_DISCOVERY_MAX_PAGES,
                    ))
                    .for_resource(id.clone()));
                }
                describe_ids_from_pages(
                    &pages,
                    |output| output.$items(),
                    |resource| resource.$identifier(),
                )
            }};
            ($request:expr, $items:ident, $identifier:ident, single) => {{
                let output = $request.send().await.map_err(|error| {
                    AwsProvider::tagged_ec2_discovery_call_error(id, self, error)
                })?;
                describe_ids_from_pages(
                    std::slice::from_ref(&output),
                    |output| output.$items(),
                    |resource| resource.$identifier(),
                )
            }};
        }

        let mut identifiers = match self {
            Self::Vpc => describe_ids!(
                provider.ec2_client.describe_vpcs().filters(filter),
                vpcs,
                vpc_id,
                paginated
            ),
            Self::Subnet => describe_ids!(
                provider.ec2_client.describe_subnets().filters(filter),
                subnets,
                subnet_id,
                paginated
            ),
            Self::RouteTable => describe_ids!(
                provider.ec2_client.describe_route_tables().filters(filter),
                route_tables,
                route_table_id,
                paginated
            ),
            Self::SecurityGroup => describe_ids!(
                provider
                    .ec2_client
                    .describe_security_groups()
                    .filters(filter),
                security_groups,
                group_id,
                paginated
            ),
            Self::InternetGateway => describe_ids!(
                provider
                    .ec2_client
                    .describe_internet_gateways()
                    .filters(filter),
                internet_gateways,
                internet_gateway_id,
                paginated
            ),
            Self::EgressOnlyInternetGateway => describe_ids!(
                provider
                    .ec2_client
                    .describe_egress_only_internet_gateways()
                    .filters(filter),
                egress_only_internet_gateways,
                egress_only_internet_gateway_id,
                paginated
            ),
            Self::VpnGateway => describe_ids!(
                provider.ec2_client.describe_vpn_gateways().filters(filter),
                vpn_gateways,
                vpn_gateway_id,
                single
            ),
            Self::VpcPeeringConnection => describe_ids!(
                provider
                    .ec2_client
                    .describe_vpc_peering_connections()
                    .filters(filter),
                vpc_peering_connections,
                vpc_peering_connection_id,
                paginated
            ),
            Self::Address => describe_ids!(
                provider.ec2_client.describe_addresses().filters(filter),
                addresses,
                allocation_id,
                single
            ),
            Self::NatGateway => describe_ids!(
                provider.ec2_client.describe_nat_gateways().filter(filter),
                nat_gateways,
                nat_gateway_id,
                paginated
            ),
            Self::VpcEndpoint => describe_ids!(
                provider.ec2_client.describe_vpc_endpoints().filters(filter),
                vpc_endpoints,
                vpc_endpoint_id,
                paginated
            ),
            Self::FlowLog => describe_ids!(
                provider.ec2_client.describe_flow_logs().filter(filter),
                flow_logs,
                flow_log_id,
                paginated
            ),
            Self::TransitGateway => describe_ids!(
                provider
                    .ec2_client
                    .describe_transit_gateways()
                    .filters(filter),
                transit_gateways,
                transit_gateway_id,
                paginated
            ),
            Self::TransitGatewayVpcAttachment => describe_ids!(
                provider
                    .ec2_client
                    .describe_transit_gateway_vpc_attachments()
                    .filters(filter),
                transit_gateway_vpc_attachments,
                transit_gateway_attachment_id,
                paginated
            ),
        };

        if self == Self::FlowLog {
            identifiers.sort();
            identifiers.dedup();
        }
        Ok(identifiers)
    }
}

#[derive(Debug)]
enum TaggedDiscovery {
    Retry {
        token: AwsCreateToken,
        poll_after_secs: u32,
    },
    Found(AppliedIdentifier),
    Exhausted {
        token: AwsCreateToken,
        error: ProviderError,
    },
    Ambiguous {
        token: AwsCreateToken,
        error: ProviderError,
    },
}

#[derive(Debug)]
enum AcmResendTransition {
    Issued(AppliedIdentifier),
    Pending(AwsCreateToken),
    InFlight(ProviderError),
}

/// The initial request always has at least one later attempt available, so a
/// maybe-sent result has a pending continuation rather than an in-flight
/// outcome.
#[derive(Debug)]
enum InitialAcmRequestTransition {
    Issued(AppliedIdentifier, Sent),
    Pending(AwsCreateToken, Sent),
    Untracked(ProviderError),
}

#[derive(Debug)]
enum TaggedEc2Continuation {
    Complete(CreateCompletion),
    Pending(AwsCreateToken),
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

/// A classified post-mutation read, finished through the caller's typestate handle.
#[derive(Debug)]
pub(crate) enum CreateCompletion {
    Partial {
        identifier: AppliedIdentifier,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    },
    ReadBackPending {
        continuation: ReadBackContinuation,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    },
    Failed {
        identifier: AppliedIdentifier,
        observation: Observation,
        error: ProviderError,
    },
    Success(AppliedState),
}

pub(crate) type TypedCreateCompletion = (Sent, CreateCompletion);

/// Native providers drive pending steps themselves, while the WASM guest
/// returns one step to the host. These adapters let both paths consume the
/// same protocol-v15 typestate at the exact AWS classification boundary.
#[doc(hidden)]
pub trait AwsIssuedCreate: Sized {
    type Step;
    type Verdict;

    fn identifier(&self) -> &AppliedIdentifier;
    fn succeeded(self, attributes: HashMap<String, Value>) -> Result<Self::Step, Self::Verdict>;
    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict>;
    fn pending_read_back(
        self,
        continuation: ReadBackContinuation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict;
    fn discarded(self, error: ProviderError) -> Self::Verdict;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict;
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
    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict;
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
pub trait AwsResumedCreate: Sized {
    type Step;
    type Verdict;

    fn identifier(&self) -> &AppliedIdentifier;
    fn completed(self, state: AppliedState) -> Result<Self::Step, Self::Verdict>;
    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict>;
    fn pending_read_back(
        self,
        continuation: ReadBackContinuation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
    fn failed(self, observation: Observation, error: ProviderError) -> Self::Verdict;
    fn discarded(self, error: ProviderError) -> Self::Verdict;
    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict>;
    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict;
}

#[doc(hidden)]
pub trait AwsResumeAttempt {
    type Step;
    type Verdict;
    type Resumed: AwsResumedCreate<Step = Self::Step, Verdict = Self::Verdict>;

    fn untracked(self, error: ProviderError) -> Self::Verdict;
    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict;
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

fn encode_create_token(token: &AwsCreateToken) -> PendingToken {
    // Every token field is a string or integer, so this enum is always valid
    // JSON. Keeping this conversion infallible is important: a poll/resume
    // classification must never discard an already in-flight continuation.
    PendingToken::new(
        serde_json::to_string(token)
            .expect("AWS create continuation contains only JSON-safe fields"),
    )
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
        Ok(CoreCreateStep::Pending(self.pending(
            encode_create_token(&token),
            sent,
            poll_after_secs,
        )))
    }
}

impl AwsPollAttempt for PollAttempt {
    type Step = CoreCreateStep;
    type Verdict = CreateFailure;
    type Issued = IssuedCreate;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        self.untracked(error)
    }

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.pending(encode_create_token(&token), poll_after_secs)
            .preserve(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Issued {
        self.issued(identifier)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreCreateStep::Pending(
            self.pending(encode_create_token(&token), poll_after_secs),
        ))
    }
}

impl AwsIssuedCreate for IssuedCreate {
    type Step = CoreCreateStep;
    type Verdict = CreateFailure;

    fn identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }

    fn succeeded(self, attributes: HashMap<String, Value>) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreCreateStep::Done(self.succeeded(attributes)))
    }

    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        let Some(diagnostic) =
            PartialReadDiagnostic::new(reason.to_string(), missing_attributes.into())
        else {
            return Err(self.discarded(ProviderError::internal(
                "read-back partial requires missing attributes",
            )));
        };
        Ok(CoreCreateStep::Done(self.partial(attributes, diagnostic)))
    }

    fn pending_read_back(
        self,
        continuation: ReadBackContinuation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreCreateStep::Pending(self.pending_read_back(
            encode_create_token(&AwsCreateToken::ReadBack(continuation)),
            poll_after_secs,
        )))
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
        Ok(CoreCreateStep::Pending(
            self.pending(encode_create_token(&token), poll_after_secs),
        ))
    }

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        if token.phase() == PendingPhase::ReadBack {
            self.pending_read_back(encode_create_token(&token), poll_after_secs)
                .preserve(error)
        } else {
            self.pending(encode_create_token(&token), poll_after_secs)
                .preserve(error)
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

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        self.repend(encode_create_token(&token)).preserve(error)
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Resumed {
        self.issued(identifier)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreResumeStep::Pending(Box::new(
            self.repend(encode_create_token(&token)),
        )))
    }
}

impl AwsResumedCreate for ResumedCreate {
    type Step = CoreResumeStep;
    type Verdict = ResumeFailure;

    fn identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }

    fn completed(self, state: AppliedState) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreResumeStep::Done(self.completed(state)))
    }

    fn partial(
        self,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        let Some(diagnostic) =
            PartialReadDiagnostic::new(reason.to_string(), missing_attributes.into())
        else {
            return Err(self.discarded(ProviderError::internal(
                "read-back partial requires missing attributes",
            )));
        };
        Ok(CoreResumeStep::Done(self.partial(attributes, diagnostic)))
    }

    fn pending_read_back(
        self,
        continuation: ReadBackContinuation,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(CoreResumeStep::Pending(Box::new(self.repend(
            encode_create_token(&AwsCreateToken::ReadBack(continuation)),
        ))))
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
        Ok(CoreResumeStep::Pending(Box::new(
            self.repend(encode_create_token(&token)),
        )))
    }

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        self.repend(encode_create_token(&token)).preserve(error)
    }
}

fn delete_pending(token: AwsDeleteToken) -> AwsDeleteStep {
    OperationStep::Pending {
        pending: token,
        poll_after_secs: POLL_AFTER_SECS,
        phase: PendingPhase::Operation,
    }
}

fn delete_done(_: ()) -> AwsDeleteStep {
    OperationStep::Done(())
}

async fn drive_operation<T, Tok, Initial, Poll, PollFuture, Sleep, SleepFuture>(
    id: &ResourceId,
    kind: OperationKind,
    budget: Duration,
    read_back_budget: Duration,
    initial: Initial,
    mut poll: Poll,
    sleep: Sleep,
) -> ProviderResult<CompletedOperation<T>>
where
    Tok: Clone,
    Initial: Future<Output = ProviderResult<OperationStep<T, ProviderError, Tok>>>,
    Poll: FnMut(Tok, PollRequest) -> PollFuture,
    PollFuture: Future<Output = ProviderResult<OperationStep<T, ProviderError, Tok>>>,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    match drive_pending(
        budget,
        read_back_budget,
        initial,
        |token, finality| poll(token, PollRequest { finality }),
        sleep,
    )
    .await
    {
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
        Ok(DriveOutcome::ProtocolViolation { .. }) => Err(ProviderError::internal(
            "AWS operation returned an invalid pending phase",
        )
        .for_resource(id.clone())),
        Err(DriveError::Initial(error)) | Err(DriveError::Poll { error, .. }) => Err(error),
    }
}

fn native_create_step(
    step: Result<CoreCreateStep, CreateFailure>,
) -> OperationStep<CreateOutcome, CreateFailure, PendingCreate> {
    match step {
        Ok(CoreCreateStep::Done(done)) => OperationStep::Done(done),
        Ok(CoreCreateStep::Pending(pending)) => OperationStep::Pending {
            poll_after_secs: pending.poll_after_secs(),
            phase: pending.phase(),
            pending,
        },
        Err(error) => OperationStep::Verdict(error),
    }
}

struct NativeResumePending {
    resume: Box<Resume>,
    token: AwsCreateToken,
}

fn native_resume_step(
    step: Result<CoreResumeStep, ResumeFailure>,
) -> OperationStep<ResumeOutcome, ResumeFailure, NativeResumePending> {
    match step {
        Ok(CoreResumeStep::Done(done)) => OperationStep::Done(done),
        Ok(CoreResumeStep::Pending(resume)) => match decode_create_token(resume.token()) {
            Ok(token) => OperationStep::Pending {
                poll_after_secs: token.poll_after_secs(),
                phase: token.phase(),
                pending: NativeResumePending { resume, token },
            },
            Err(error) => OperationStep::Verdict(resume.preserve(error)),
        },
        Err(error) => OperationStep::Verdict(error),
    }
}

impl AwsProvider {
    fn operation_budget(&self, resource_type: &str, kind: OperationKind) -> Duration {
        self.resource_schema(resource_type)
            .and_then(|schema| schema.operation_config())
            .map_or(DEFAULT_OPERATION_BUDGET, |config| config.budget(kind))
    }

    pub(crate) fn create_outcome_from_read(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        attributes: HashMap<String, Value>,
        outcome: ProviderResult<ReadOutcome>,
    ) -> CreateCompletion {
        let requirements = match self.declared_read_back_requirements(id, &attributes) {
            Ok(requirements) => requirements,
            Err(error) => return invalid_read_back(id, identifier, error),
        };
        self.classify_read_back(id, identifier, attributes, outcome, requirements)
    }

    fn read_back_budget(&self, resource_type: &str) -> Duration {
        self.resource_schema(resource_type)
            .and_then(|schema| schema.operation_config())
            .map_or(DEFAULT_READ_BACK_BUDGET, |config| config.read_back_budget())
    }

    async fn request_acm_certificate_transition(
        &self,
        resource: &Resource<impl Sized>,
        schema: &carina_core::schema::CompiledResourceSchema,
        fresh_client_token: FreshAcmClientToken,
        attempts_remaining: u8,
    ) -> AcmResendTransition {
        let FreshAcmClientToken {
            client_token,
            first_used_at_unix_secs,
        } = fresh_client_token;
        let token = |attempts_remaining| AwsCreateToken::AcmRequestCertificate {
            client_token: client_token.clone(),
            first_used_at_unix_secs,
            attempts_remaining,
        };
        match self
            .request_acm_certificate(resource, schema, &client_token)
            .await
        {
            Ok((sent, result)) => {
                // A retry/resume already carries the original Sent record in
                // core's typestate; this decode is solely the replay input.
                drop(sent);
                match result {
                    Ok(identifier) => AcmResendTransition::Issued(identifier),
                    Err(error) => {
                        let classification = error.classification().with_prior_maybe_sent();
                        let retryable = provider_retry_allowed(
                            classification,
                            aws_sdk_acm::operation::request_certificate::RequestCertificateError::IDEMPOTENCY,
                            &error,
                        );
                        if retryable && attempts_remaining > 1 {
                            AcmResendTransition::Pending(token(attempts_remaining - 1))
                        } else {
                            let error = crate::services::acm::certificate::request_certificate_outcome_unknown_error(
                                &resource.id,
                                error,
                            );
                            AcmResendTransition::InFlight(error)
                        }
                    }
                }
            }
            Err(error) => AcmResendTransition::InFlight(error),
        }
    }

    async fn request_initial_acm_certificate_transition(
        &self,
        resource: &Resource<impl Sized>,
        schema: &carina_core::schema::CompiledResourceSchema,
        fresh_client_token: FreshAcmClientToken,
        attempts_after_request: NonZeroU8,
    ) -> InitialAcmRequestTransition {
        let FreshAcmClientToken {
            client_token,
            first_used_at_unix_secs,
        } = fresh_client_token;
        let pending_token = AwsCreateToken::AcmRequestCertificate {
            client_token: client_token.clone(),
            first_used_at_unix_secs,
            attempts_remaining: attempts_after_request.get(),
        };
        match self
            .request_acm_certificate(resource, schema, &client_token)
            .await
        {
            Ok((sent, Ok(identifier))) => InitialAcmRequestTransition::Issued(identifier, sent),
            Ok((sent, Err(error))) => match error.classification() {
                SendClassification::MaybeSent { .. } => {
                    InitialAcmRequestTransition::Pending(pending_token, sent)
                }
                SendClassification::NotSent(_) | SendClassification::Rejected => {
                    drop(sent);
                    InitialAcmRequestTransition::Untracked(
                        crate::services::acm::certificate::request_certificate_error(
                            &resource.id,
                            error,
                        ),
                    )
                }
            },
            Err(error) => InitialAcmRequestTransition::Untracked(error),
        }
    }

    async fn finish_acm_certificate_issued<I: AwsIssuedCreate>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &carina_core::schema::CompiledResourceSchema,
        identifier: AppliedIdentifier,
        poll_request: PollRequest,
        issued: I,
    ) -> Result<I::Step, I::Verdict> {
        match self
            .continue_acm_certificate_create(resource, schema, identifier)
            .await
        {
            Ok(completion) => self.finish_issued(issued, completion, poll_request),
            Err(error) => Err(self.failed_issued(&resource.id, issued, error).await),
        }
    }

    async fn finish_acm_certificate_resumed<R: AwsResumedCreate>(
        &self,
        resource: &Resource<impl Sized>,
        schema: &carina_core::schema::CompiledResourceSchema,
        identifier: AppliedIdentifier,
        poll_request: PollRequest,
        resumed: R,
    ) -> Result<R::Step, R::Verdict> {
        match self
            .continue_acm_certificate_create(resource, schema, identifier)
            .await
        {
            Ok(completion) => self.finish_resumed_completion(resumed, completion, poll_request),
            Err(error) => Err(self.failed_resumed(&resource.id, resumed, error).await),
        }
    }

    fn finish_create<A: AwsCreateAttempt>(
        &self,
        attempt: A,
        sent: Sent,
        result: ProviderResult<CreateCompletion>,
    ) -> Result<A::Step, A::Verdict> {
        match result {
            Ok(CreateCompletion::Success(state)) => {
                let (_id, identifier, attributes) = state.into_parts();
                attempt.issued(identifier, sent).succeeded(attributes)
            }
            Ok(CreateCompletion::ReadBackPending { continuation, .. }) => attempt
                .issued(continuation.identifier().clone(), sent)
                .pending_read_back(continuation, POLL_AFTER_SECS),
            Ok(CreateCompletion::Partial {
                identifier,
                attributes,
                reason,
                missing_attributes,
            }) => attempt
                .issued(identifier, sent)
                .partial(attributes, reason, missing_attributes),
            Ok(CreateCompletion::Failed {
                identifier,
                observation,
                error,
            }) => Err(attempt.issued(identifier, sent).failed(observation, error)),
            Err(error) => Err(attempt.untracked(error)),
        }
    }

    fn finish_typed_create<A: AwsCreateAttempt>(
        &self,
        attempt: A,
        result: ProviderResult<TypedCreateCompletion>,
    ) -> Result<A::Step, A::Verdict> {
        match result {
            Ok((sent, completion)) => self.finish_create(attempt, sent, Ok(completion)),
            Err(error) => Err(attempt.untracked(error)),
        }
    }

    pub(crate) fn finish_issued<I: AwsIssuedCreate>(
        &self,
        issued: I,
        completion: CreateCompletion,
        poll_request: PollRequest,
    ) -> Result<I::Step, I::Verdict> {
        match (poll_request.finality, completion) {
            (PollFinality::Continue, CreateCompletion::ReadBackPending { continuation, .. }) => {
                issued.pending_read_back(continuation, POLL_AFTER_SECS)
            }
            (PollFinality::Continue | PollFinality::FinalAttempt, completion) => {
                self.finish_issued_completion(issued, finish_final_read_back(completion))
            }
        }
    }

    /// The final-attempt branch can only reach this terminal-only consumer.
    fn finish_issued_completion<I: AwsIssuedCreate>(
        &self,
        issued: I,
        completion: ReadBackCompletion,
    ) -> Result<I::Step, I::Verdict> {
        match completion {
            ReadBackCompletion::Failed { observation, error } => {
                Err(issued.failed(observation, error))
            }
            ReadBackCompletion::Success(state) => {
                let (_id, _identifier, attributes) = state.into_parts();
                issued.succeeded(attributes)
            }
            ReadBackCompletion::Partial {
                attributes,
                reason,
                missing_attributes,
                ..
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

    pub(crate) fn pending_tagged_ec2<A: AwsCreateAttempt>(
        attempt: A,
        kind: Ec2TaggedResourceKind,
        create_token: &ClientToken,
        sent: Sent,
    ) -> Result<A::Step, A::Verdict> {
        attempt.pending(
            AwsCreateToken::TaggedEc2 {
                kind,
                create_token: create_token.clone(),
                attempts_remaining: TAGGED_EC2_DISCOVERY_ATTEMPTS,
                next_delay_secs: TAGGED_EC2_INITIAL_DELAY_SECS,
                original_failure: None,
            },
            sent,
            TAGGED_EC2_INITIAL_DELAY_SECS,
        )
    }

    pub(crate) fn tagged_ec2_send_error<A, E>(
        attempt: A,
        id: &ResourceId,
        kind: Ec2TaggedResourceKind,
        create_token: &ClientToken,
        sent: Sent,
        error: SendFailure<E>,
    ) -> Result<A::Step, A::Verdict>
    where
        A: AwsCreateAttempt,
        E: std::error::Error
            + DecidedOperationError
            + aws_smithy_types::error::metadata::ProvideErrorMetadata
            + aws_smithy_types::retry::ProvideErrorKind
            + Send
            + Sync
            + 'static,
    {
        match error.classification() {
            SendClassification::MaybeSent { .. } => {
                let original_failure = AwsFailureSummary::from_send_failure(&error);
                attempt.pending(
                    AwsCreateToken::TaggedEc2 {
                        kind,
                        create_token: create_token.clone(),
                        attempts_remaining: TAGGED_EC2_DISCOVERY_ATTEMPTS,
                        next_delay_secs: TAGGED_EC2_INITIAL_DELAY_SECS,
                        original_failure: Some(original_failure),
                    },
                    sent,
                    TAGGED_EC2_INITIAL_DELAY_SECS,
                )
            }
            SendClassification::NotSent(_) | SendClassification::Rejected => Err(attempt
                .untracked(
                    crate::error_helpers::api_error_with_meta(kind.create_error_context(), error)
                        .for_resource(id.clone()),
                )),
        }
    }

    pub(crate) async fn finish_tagged_ec2_create<A, O, Failure, Identifier>(
        &self,
        resource: &Resource<impl Sized>,
        attempt: A,
        context: TaggedEc2CreateContext<'_>,
        result: Result<O, Failure>,
        identifier: Identifier,
    ) -> Result<A::Step, A::Verdict>
    where
        A: AwsCreateAttempt,
        Failure: IntoSendFailure,
        Failure::Error: std::error::Error
            + DecidedOperationError
            + aws_smithy_types::error::metadata::ProvideErrorMetadata
            + aws_smithy_types::retry::ProvideErrorKind
            + Send
            + Sync
            + 'static,
        Identifier: FnOnce(&O) -> Option<String>,
    {
        let TaggedEc2CreateContext {
            create,
            create_token,
            sent,
        } = context;
        let kind = create.kind();
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                return Self::tagged_ec2_send_error(
                    attempt,
                    &resource.id,
                    kind,
                    create_token,
                    sent,
                    error.into_send_failure(),
                );
            }
        };
        let Some(identifier) = identifier(&output) else {
            return Self::pending_tagged_ec2(attempt, kind, create_token, sent);
        };
        let identifier = match crate::helpers::applied_identifier(
            &resource.id,
            kind.create_operation(),
            &identifier,
        ) {
            Ok(identifier) => identifier,
            Err(error) => return Err(attempt.untracked(error)),
        };
        let issued = attempt.issued(identifier.clone(), sent);
        self.finish_tagged_ec2_issued(
            resource,
            create,
            identifier,
            issued,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
    }

    fn tagged_ec2_discovery_call_error<E>(
        id: &ResourceId,
        kind: Ec2TaggedResourceKind,
        error: crate::error_helpers::AwsSdkError<E>,
    ) -> ProviderError
    where
        E: std::error::Error
            + DecidedOperationError
            + aws_smithy_types::error::metadata::ProvideErrorMetadata
            + aws_smithy_types::retry::ProvideErrorKind
            + Send
            + Sync
            + 'static,
    {
        crate::error_helpers::api_error_with_meta(kind.discovery_error_context(), error)
            .for_resource(id.clone())
    }

    async fn discover_tagged_ec2_resources(
        &self,
        resource: &Resource<impl Sized>,
        kind: Ec2TaggedResourceKind,
        create_token: &ClientToken,
    ) -> ProviderResult<Vec<AppliedIdentifier>> {
        let id = &resource.id;
        let filter = aws_sdk_ec2::types::Filter::builder()
            .name(format!("tag:{EC2_CREATE_TOKEN_TAG}"))
            .values(create_token.as_str())
            .build();
        let mut raw_identifiers = kind.describe_by_tag(self, id, filter).await?;

        if kind == Ec2TaggedResourceKind::FlowLog {
            let requested_count = match resource.get_attr("resource_ids") {
                Some(Value::Concrete(carina_core::resource::ConcreteValue::List(items))) => {
                    items.len()
                }
                Some(Value::Concrete(carina_core::resource::ConcreteValue::StringList(items))) => {
                    items.len()
                }
                Some(_) | None => {
                    return Err(ProviderError::invalid_input(
                        "resource_ids must be a non-empty list of resource identifiers",
                    )
                    .for_resource(id.clone()));
                }
            };
            if requested_count == 0 {
                return Err(ProviderError::invalid_input(
                    "resource_ids must be a non-empty list of resource identifiers",
                )
                .for_resource(id.clone()));
            }
            match raw_identifiers.len().cmp(&requested_count) {
                std::cmp::Ordering::Less => raw_identifiers.clear(),
                std::cmp::Ordering::Equal => {
                    raw_identifiers = vec![raw_identifiers.join(",")];
                }
                std::cmp::Ordering::Greater => {}
            }
        }

        raw_identifiers
            .into_iter()
            .map(|value| {
                crate::helpers::applied_identifier(id, kind.create_operation(), value.as_str())
            })
            .collect()
    }

    async fn read_tagged_ec2_resource(
        &self,
        id: &ResourceId,
        kind: Ec2TaggedResourceKind,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        match kind {
            Ec2TaggedResourceKind::Vpc => self.read_ec2_vpc(id, identifier).await,
            Ec2TaggedResourceKind::Subnet => self.read_ec2_subnet(id, identifier).await,
            Ec2TaggedResourceKind::RouteTable => self.read_ec2_route_table(id, identifier).await,
            Ec2TaggedResourceKind::SecurityGroup => {
                self.read_ec2_security_group(id, identifier).await
            }
            Ec2TaggedResourceKind::InternetGateway => {
                self.read_ec2_internet_gateway(id, identifier).await
            }
            Ec2TaggedResourceKind::EgressOnlyInternetGateway => {
                self.read_ec2_egress_only_internet_gateway(id, identifier)
                    .await
            }
            Ec2TaggedResourceKind::VpnGateway => self.read_ec2_vpn_gateway(id, identifier).await,
            Ec2TaggedResourceKind::VpcPeeringConnection => {
                self.read_ec2_vpc_peering_connection(id, identifier).await
            }
            Ec2TaggedResourceKind::Address => self.read_ec2_eip(id, identifier).await,
            Ec2TaggedResourceKind::NatGateway => self.read_ec2_nat_gateway(id, identifier).await,
            Ec2TaggedResourceKind::VpcEndpoint => self.read_ec2_vpc_endpoint(id, identifier).await,
            Ec2TaggedResourceKind::FlowLog => self.read_ec2_flow_log(id, identifier).await,
            Ec2TaggedResourceKind::TransitGateway => {
                self.read_ec2_transit_gateway(id, identifier).await
            }
            Ec2TaggedResourceKind::TransitGatewayVpcAttachment => {
                self.read_ec2_transit_gateway_attachment(id, identifier)
                    .await
            }
        }
    }

    fn tagged_ec2_outcome_unknown_message(
        kind: Ec2TaggedResourceKind,
        create_token: &ClientToken,
        original_failure: Option<&AwsFailureSummary>,
    ) -> String {
        let mut message = format!(
            "{} may have been created with tag {EC2_CREATE_TOKEN_TAG}={}; check before re-applying",
            kind.resource_type(),
            create_token.as_str()
        );
        if let Some(original_failure) = original_failure {
            message.push_str("; original send failure: ");
            message.push_str(&original_failure.to_string());
        }
        message
    }

    fn tagged_ec2_outcome_unknown_error(
        id: &ResourceId,
        kind: Ec2TaggedResourceKind,
        create_token: &ClientToken,
        original_failure: Option<&AwsFailureSummary>,
        discovery_error: Option<ProviderError>,
    ) -> ProviderError {
        let message =
            Self::tagged_ec2_outcome_unknown_message(kind, create_token, original_failure);
        match discovery_error {
            None => ProviderError::api_error(message),
            Some(cause) => {
                // ProviderError's typed-cause constructor deliberately takes a
                // static context. This terminal path is rare and bounded by the
                // discovery window; leaking this one diagnostic string lets us
                // preserve the typed discovery error instead of flattening its
                // AWS status/code/request-id chain.
                let context: &'static str = Box::leak(message.into_boxed_str());
                ProviderError::api_error_caused_by(context, cause)
            }
        }
        .for_resource(id.clone())
    }

    fn tagged_ec2_ambiguous_error(
        id: &ResourceId,
        kind: Ec2TaggedResourceKind,
        identifiers: &[AppliedIdentifier],
    ) -> ProviderError {
        let identifiers = identifiers
            .iter()
            .map(AppliedIdentifier::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        ProviderError::api_error(format!(
            "outcome unknown: {} matched multiple resources by create token: {identifiers}",
            kind.create_operation()
        ))
        .for_resource(id.clone())
    }

    async fn tagged_ec2_discovery(
        &self,
        resource: &Resource<impl Sized>,
        kind: Ec2TaggedResourceKind,
        create_token: ClientToken,
        attempts_remaining: u8,
        next_delay_secs: u32,
        original_failure: Option<AwsFailureSummary>,
    ) -> TaggedDiscovery {
        let id = &resource.id;
        let result = self
            .discover_tagged_ec2_resources(resource, kind, &create_token)
            .await;
        let token = || AwsCreateToken::TaggedEc2 {
            kind,
            create_token: create_token.clone(),
            attempts_remaining,
            next_delay_secs,
            original_failure: original_failure.clone(),
        };

        match result {
            Ok(mut identifiers) if identifiers.len() == 1 => {
                TaggedDiscovery::Found(identifiers.remove(0))
            }
            Ok(identifiers) if identifiers.len() > 1 => TaggedDiscovery::Ambiguous {
                token: token(),
                error: Self::tagged_ec2_ambiguous_error(id, kind, &identifiers),
            },
            Ok(_) | Err(_) if attempts_remaining > 1 => {
                let poll_after_secs = next_tagged_ec2_delay(next_delay_secs);
                TaggedDiscovery::Retry {
                    token: AwsCreateToken::TaggedEc2 {
                        kind,
                        create_token,
                        attempts_remaining: attempts_remaining - 1,
                        next_delay_secs: poll_after_secs,
                        original_failure,
                    },
                    poll_after_secs,
                }
            }
            Ok(_) => TaggedDiscovery::Exhausted {
                token: token(),
                error: Self::tagged_ec2_outcome_unknown_error(
                    id,
                    kind,
                    &create_token,
                    original_failure.as_ref(),
                    None,
                ),
            },
            Err(discovery_error) => TaggedDiscovery::Exhausted {
                token: token(),
                error: Self::tagged_ec2_outcome_unknown_error(
                    id,
                    kind,
                    &create_token,
                    original_failure.as_ref(),
                    Some(discovery_error),
                ),
            },
        }
    }

    async fn continue_tagged_ec2_create(
        &self,
        resource: &Resource<impl Sized>,
        create: TaggedEc2Create,
        identifier: AppliedIdentifier,
    ) -> ProviderResult<TaggedEc2Continuation> {
        let kind = create.kind();
        match create {
            TaggedEc2Create::Vpc {
                enable_dns_hostnames,
                enable_dns_support,
            } => {
                self.continue_ec2_vpc_create_with(
                    &resource.id,
                    identifier.as_str(),
                    enable_dns_hostnames,
                    enable_dns_support,
                )
                .await?;
            }
            TaggedEc2Create::Subnet {
                assign_ipv6_address_on_creation,
                enable_dns64,
                enable_lni_at_device_index,
                map_public_ip_on_launch,
                private_dns_name_options_on_launch,
            } => {
                let schema = self
                    .resource_schema(resource.id.resource_type.as_str())
                    .ok_or_else(|| {
                        ProviderError::internal("missing EC2 subnet schema")
                            .for_resource(resource.id.clone())
                    })?;
                self.continue_ec2_subnet_create_with(
                    &resource.id,
                    schema,
                    identifier.as_str(),
                    assign_ipv6_address_on_creation,
                    enable_dns64,
                    enable_lni_at_device_index,
                    map_public_ip_on_launch,
                    private_dns_name_options_on_launch.as_ref(),
                )
                .await?;
            }
            TaggedEc2Create::InternetGateway { vpc_id } => {
                self.continue_ec2_internet_gateway_create_with(
                    &resource.id,
                    identifier.as_str(),
                    vpc_id.as_deref(),
                )
                .await?;
            }
            TaggedEc2Create::RouteTable
            | TaggedEc2Create::VpcEndpoint
            | TaggedEc2Create::SecurityGroup
            | TaggedEc2Create::EgressOnlyInternetGateway
            | TaggedEc2Create::VpnGateway
            | TaggedEc2Create::VpcPeeringConnection
            | TaggedEc2Create::Address
            | TaggedEc2Create::NatGateway
            | TaggedEc2Create::FlowLog
            | TaggedEc2Create::TransitGateway
            | TaggedEc2Create::TransitGatewayVpcAttachment => {}
        }

        let pending = match kind {
            Ec2TaggedResourceKind::NatGateway => Some(AwsCreateToken::NatGateway {
                nat_gateway_id: identifier.clone(),
            }),
            Ec2TaggedResourceKind::TransitGateway => Some(AwsCreateToken::TransitGateway {
                transit_gateway_id: identifier.clone(),
            }),
            Ec2TaggedResourceKind::TransitGatewayVpcAttachment => {
                Some(AwsCreateToken::TransitGatewayAttachment {
                    attachment_id: identifier.clone(),
                })
            }
            Ec2TaggedResourceKind::Vpc
            | Ec2TaggedResourceKind::Subnet
            | Ec2TaggedResourceKind::RouteTable
            | Ec2TaggedResourceKind::SecurityGroup
            | Ec2TaggedResourceKind::InternetGateway
            | Ec2TaggedResourceKind::EgressOnlyInternetGateway
            | Ec2TaggedResourceKind::VpnGateway
            | Ec2TaggedResourceKind::VpcPeeringConnection
            | Ec2TaggedResourceKind::Address
            | Ec2TaggedResourceKind::VpcEndpoint
            | Ec2TaggedResourceKind::FlowLog => None,
        };
        if let Some(token) = pending {
            return Ok(TaggedEc2Continuation::Pending(token));
        }

        let outcome = self
            .read_tagged_ec2_resource(&resource.id, kind, &identifier)
            .await;
        Ok(TaggedEc2Continuation::Complete(
            self.create_outcome_from_read(
                &resource.id,
                identifier,
                resource.resolved_attributes(),
                outcome,
            ),
        ))
    }

    pub(crate) async fn finish_tagged_ec2_issued<I: AwsIssuedCreate>(
        &self,
        resource: &Resource<impl Sized>,
        create: TaggedEc2Create,
        identifier: AppliedIdentifier,
        issued: I,
        poll_request: PollRequest,
    ) -> Result<I::Step, I::Verdict> {
        match self
            .continue_tagged_ec2_create(resource, create, identifier)
            .await
        {
            Ok(TaggedEc2Continuation::Complete(completion)) => {
                self.finish_issued(issued, completion, poll_request)
            }
            Ok(TaggedEc2Continuation::Pending(token)) => issued.pending(token, POLL_AFTER_SECS),
            Err(error) => Err(self.failed_issued(&resource.id, issued, error).await),
        }
    }

    async fn finish_tagged_ec2_resumed<R: AwsResumedCreate>(
        &self,
        resource: &Resource<impl Sized>,
        kind: Ec2TaggedResourceKind,
        identifier: AppliedIdentifier,
        resumed: R,
        poll_request: PollRequest,
    ) -> Result<R::Step, R::Verdict> {
        let create = match TaggedEc2Create::decode_resumed(kind, resource, &self.schema_registry) {
            Ok(create) => create,
            Err(error) => return Err(self.failed_resumed(&resource.id, resumed, error).await),
        };
        match self
            .continue_tagged_ec2_create(resource, create, identifier)
            .await
        {
            Ok(TaggedEc2Continuation::Complete(completion)) => {
                self.finish_resumed_completion(resumed, completion, poll_request)
            }
            Ok(TaggedEc2Continuation::Pending(token)) => resumed.pending(token, POLL_AFTER_SECS),
            Err(error) => Err(self.failed_resumed(&resource.id, resumed, error).await),
        }
    }

    /// Perform the initial, non-blocking create step.
    pub async fn create_step<A: AwsCreateAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        attempt: A,
    ) -> Result<A::Step, A::Verdict> {
        if let Err(error) =
            crate::ec2_tags::validate_ec2_user_tags(&resource.id, &resource.resolved_attributes())
        {
            return Err(attempt.untracked(error));
        }
        let client_token = crate::aws_operation::ClientToken::new();
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
                self.create_ec2_nat_gateway_start(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.TransitGateway" => {
                self.create_ec2_transit_gateway_start(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.TransitGatewayAttachment" => {
                self.create_ec2_transit_gateway_attachment_start(
                    resource,
                    schema,
                    &client_token,
                    attempt,
                )
                .await
            }
            "organizations.Account" => {
                let (request_id, sent) = match self
                    .create_organizations_account_start(resource, schema)
                    .await
                {
                    Ok(request_id) => request_id,
                    Err(error) => return Err(attempt.untracked(error)),
                };
                attempt.pending(
                    AwsCreateToken::OrganizationsAccount {
                        create_account_request_id: request_id,
                    },
                    sent,
                    POLL_AFTER_SECS,
                )
            }
            "acm.Certificate" => {
                let first_used_at_unix_secs = match unix_timestamp_now() {
                    Ok(timestamp) => timestamp,
                    Err(error) => {
                        return Err(attempt.untracked(error.for_resource(resource.id.clone())));
                    }
                };
                let fresh_client_token = match fresh_acm_client_token(
                    client_token,
                    first_used_at_unix_secs,
                    first_used_at_unix_secs,
                ) {
                    Some(fresh_client_token) => fresh_client_token,
                    None => {
                        return Err(attempt.untracked(
                            ProviderError::internal(
                                "new ACM client token is outside its idempotency window",
                            )
                            .for_resource(resource.id.clone()),
                        ));
                    }
                };
                match self
                    .request_initial_acm_certificate_transition(
                        resource,
                        schema,
                        fresh_client_token,
                        ACM_REQUEST_CERTIFICATE_POLL_ATTEMPTS,
                    )
                    .await
                {
                    InitialAcmRequestTransition::Issued(identifier, sent) => {
                        let issued = attempt.issued(identifier.clone(), sent);
                        self.finish_acm_certificate_issued(
                            resource,
                            schema,
                            identifier,
                            PollRequest {
                                finality: PollFinality::Continue,
                            },
                            issued,
                        )
                        .await
                    }
                    InitialAcmRequestTransition::Pending(token, sent) => {
                        attempt.pending(token, sent, POLL_AFTER_SECS)
                    }
                    InitialAcmRequestTransition::Untracked(error) => Err(attempt.untracked(error)),
                }
            }
            "s3.Bucket" => self.create_s3_bucket(resource, schema, attempt).await,
            "s3.BucketPolicy" => {
                self.finish_typed_create(attempt, self.create_s3_bucket_policy(resource).await)
            }
            "s3.BucketPublicAccessBlock" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_public_access_block(resource).await,
            ),
            "s3.BucketVersioning" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_versioning(resource, schema).await,
            ),
            "s3.BucketObjectLockConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_object_lock_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketServerSideEncryptionConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_server_side_encryption_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketAcl" => {
                self.finish_typed_create(attempt, self.create_s3_bucket_acl(resource, schema).await)
            }
            "s3.BucketOwnershipControls" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_ownership_controls(resource, schema)
                    .await,
            ),
            "s3.BucketReplicationConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_replication_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketLifecycleConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_lifecycle_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketWebsiteConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_website_configuration(resource, schema)
                    .await,
            ),
            "s3.BucketCorsConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_cors_configuration(resource).await,
            ),
            "s3.BucketNotificationConfiguration" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_notification_configuration(resource)
                    .await,
            ),
            "s3.BucketLogging" => self.finish_typed_create(
                attempt,
                self.create_s3_bucket_logging(resource, schema).await,
            ),
            "ec2.Eip" => {
                self.create_ec2_eip(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.Vpc" => {
                self.create_ec2_vpc(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.Subnet" => {
                self.create_ec2_subnet(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.InternetGateway" => {
                self.create_ec2_internet_gateway(resource, &client_token, attempt)
                    .await
            }
            "ec2.RouteTable" => {
                self.create_ec2_route_table(resource, &client_token, attempt)
                    .await
            }
            "ec2.Route" => self.finish_typed_create(attempt, self.create_ec2_route(resource).await),
            "ec2.SecurityGroup" => {
                self.create_ec2_security_group(resource, &client_token, attempt)
                    .await
            }
            "ec2.SecurityGroupIngress" => self.finish_typed_create(
                attempt,
                self.create_ec2_security_group_ingress(resource, schema)
                    .await,
            ),
            "ec2.SecurityGroupEgress" => self.finish_typed_create(
                attempt,
                self.create_ec2_security_group_egress(resource, schema)
                    .await,
            ),
            "ec2.SubnetRouteTableAssociation" => self.finish_typed_create(
                attempt,
                self.create_ec2_subnet_route_table_association(resource)
                    .await,
            ),
            "ec2.FlowLog" => {
                self.create_ec2_flow_log(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.VpcEndpoint" => {
                self.create_ec2_vpc_endpoint(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.VpcGatewayAttachment" => self.finish_typed_create(
                attempt,
                self.create_ec2_vpc_gateway_attachment(resource).await,
            ),
            "ec2.VpnGateway" => {
                self.create_ec2_vpn_gateway(resource, schema, &client_token, attempt)
                    .await
            }
            "ec2.VpcPeeringConnection" => {
                self.create_ec2_vpc_peering_connection(resource, &client_token, attempt)
                    .await
            }
            "ec2.EgressOnlyInternetGateway" => {
                self.create_ec2_egress_only_internet_gateway(resource, &client_token, attempt)
                    .await
            }
            "organizations.Organization" => self.finish_typed_create(
                attempt,
                self.create_organizations_organization(resource, schema)
                    .await,
            ),
            "iam.Role" => self.finish_typed_create(attempt, self.create_iam_role(resource).await),
            "logs.LogGroup" => self.create_logs_log_group(resource, schema, attempt).await,
            "route53.RecordSet" => self.finish_typed_create(
                attempt,
                self.create_route53_record_set(resource, schema).await,
            ),
            "sqs.Queue" => {
                self.finish_typed_create(attempt, self.create_sqs_queue(resource, schema).await)
            }
            unknown => Err(attempt.untracked(
                ProviderError::internal(format!("Unknown resource type: {unknown}"))
                    .for_resource(resource.id.clone()),
            )),
        }
    }

    /// Perform one describe step for a pending create operation.
    pub async fn poll_create_step<U: AwsPollAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        token: AwsCreateToken,
        poll: AwsCreatePoll<U, U::Issued>,
        poll_request: PollRequest,
    ) -> Result<U::Step, U::Verdict> {
        let id = &resource.id;
        match token {
            AwsCreateToken::ReadBack(continuation) => {
                let issued = match poll {
                    AwsCreatePoll::Unissued(attempt) => {
                        return Err(attempt.preserve(
                            AwsCreateToken::ReadBack(continuation),
                            ProviderError::internal(
                                "read-back requires an issued create identifier",
                            )
                            .for_resource(id.clone()),
                        ));
                    }
                    AwsCreatePoll::Issued(issued) => issued,
                };
                if let Err(error) = continuation.validate(id, issued.identifier()) {
                    return Err(issued.preserve(AwsCreateToken::ReadBack(continuation), error));
                }
                let completion = self
                    .read_back_by_identifier(resource, &continuation, ReadBackKind::Create)
                    .await;
                self.finish_issued(issued, completion, poll_request)
            }
            AwsCreateToken::AcmRequestCertificate {
                client_token,
                first_used_at_unix_secs,
                attempts_remaining,
            } => {
                let attempt = match poll {
                    AwsCreatePoll::Unissued(attempt) => attempt,
                    AwsCreatePoll::Issued(issued) => {
                        return Err(issued.failed(
                            Observation::ReadFailed(acm_request_certificate_issued_handle_error(
                                id,
                            )),
                            acm_request_certificate_issued_handle_error(id),
                        ));
                    }
                };
                let pending_token = AwsCreateToken::AcmRequestCertificate {
                    client_token: client_token.clone(),
                    first_used_at_unix_secs,
                    attempts_remaining,
                };
                let now_unix_secs = match unix_timestamp_now() {
                    Ok(timestamp) => timestamp,
                    Err(error) => {
                        return Err(attempt.preserve(pending_token, error.for_resource(id.clone())));
                    }
                };
                let fresh_client_token = match fresh_acm_client_token(
                    client_token,
                    first_used_at_unix_secs,
                    now_unix_secs,
                ) {
                    Some(fresh_client_token) => fresh_client_token,
                    None => {
                        return Err(attempt
                            .preserve(pending_token, acm_request_certificate_outcome_unknown(id)));
                    }
                };
                let schema = match self.resource_schema(resource.id.resource_type.as_str()) {
                    Some(schema) => schema,
                    None => {
                        let error = ProviderError::internal("missing ACM certificate schema")
                            .for_resource(id.clone());
                        return Err(attempt.preserve(pending_token, error));
                    }
                };
                match self
                    .request_acm_certificate_transition(
                        resource,
                        schema,
                        fresh_client_token,
                        attempts_remaining,
                    )
                    .await
                {
                    AcmResendTransition::Issued(identifier) => {
                        let issued = attempt.issued(identifier.clone());
                        self.finish_acm_certificate_issued(
                            resource,
                            schema,
                            identifier,
                            poll_request,
                            issued,
                        )
                        .await
                    }
                    AcmResendTransition::Pending(token) => attempt.pending(token, POLL_AFTER_SECS),
                    AcmResendTransition::InFlight(error) => {
                        Err(attempt.preserve(pending_token, error))
                    }
                }
            }
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
                                resource.get_attr("tags"),
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
                        self.finish_issued(issued, completion, poll_request)
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
                        self.finish_issued(issued, completion, poll_request)
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
                        self.finish_issued(issued, completion, poll_request)
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
                            Ok(completion) => self.finish_issued(issued, completion, poll_request),
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
            AwsCreateToken::TaggedEc2 {
                kind,
                create_token,
                attempts_remaining,
                next_delay_secs,
                original_failure,
            } => {
                match self
                    .tagged_ec2_discovery(
                        resource,
                        kind,
                        create_token,
                        attempts_remaining,
                        next_delay_secs,
                        original_failure,
                    )
                    .await
                {
                    TaggedDiscovery::Retry {
                        token,
                        poll_after_secs,
                    } => match poll {
                        AwsCreatePoll::Unissued(attempt) => attempt.pending(token, poll_after_secs),
                        AwsCreatePoll::Issued(issued) => issued.pending(token, poll_after_secs),
                    },
                    TaggedDiscovery::Exhausted { token, error }
                    | TaggedDiscovery::Ambiguous { token, error } => match poll {
                        AwsCreatePoll::Unissued(attempt) => Err(attempt.preserve(token, error)),
                        AwsCreatePoll::Issued(issued) => Err(issued.preserve(token, error)),
                    },
                    TaggedDiscovery::Found(identifier) => {
                        let issued = match poll {
                            AwsCreatePoll::Unissued(attempt) => attempt.issued(identifier.clone()),
                            AwsCreatePoll::Issued(issued) => issued,
                        };
                        issued.pending(
                            AwsCreateToken::TaggedEc2Found { kind, identifier },
                            POLL_AFTER_SECS,
                        )
                    }
                }
            }
            AwsCreateToken::TaggedEc2Found { kind, identifier } => {
                let issued = match poll {
                    AwsCreatePoll::Unissued(attempt) => attempt.issued(identifier.clone()),
                    AwsCreatePoll::Issued(issued) => issued,
                };
                let create =
                    match TaggedEc2Create::decode_resumed(kind, resource, &self.schema_registry) {
                        Ok(create) => create,
                        Err(error) => {
                            return Err(self.failed_issued(&resource.id, issued, error).await);
                        }
                    };
                self.finish_tagged_ec2_issued(resource, create, identifier, issued, poll_request)
                    .await
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn with_operation_config_for_test(
        mut self,
        resource_type: &str,
        config: carina_provider_protocol::types::OperationConfig,
    ) -> Self {
        let schema = crate::schemas::generated::configs()
            .into_iter()
            .find(|config| {
                config.resource_type_name == resource_type
                    && config.schema.kind() == carina_core::schema::SchemaKind::Resource
            })
            .expect("known test resource")
            .schema
            .with_operation_config(config);
        self.schema_registry
            .insert("aws", schema.validate().expect("valid budget override"));
        self
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub async fn create_with_poll_sleep_for_test<Sleep, SleepFuture>(
        &self,
        resource: &Resource<impl Sized>,
        attempt: CreateAttempt,
        sleep: Sleep,
    ) -> Result<CreateOutcome, CreateFailure>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        self.drive_native_create_with_sleep(resource, attempt, sleep)
            .await
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub async fn resume_with_poll_sleep_for_test<Sleep, SleepFuture>(
        &self,
        request: ResumeCreateRequest,
        sleep: Sleep,
    ) -> Result<ResumeOutcome, ResumeFailure>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        self.drive_native_resume_with_sleep(request, sleep).await
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub async fn update_with_poll_sleep_for_test<Sleep, SleepFuture>(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        patch: &UpdatePatch,
        sleep: Sleep,
    ) -> ProviderResult<UpdateOutcome>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        self.update_with_patch_and_sleep(id, identifier, from, patch, sleep)
            .await
    }

    async fn drive_native_create(
        &self,
        resource: &Resource<impl Sized>,
        attempt: CreateAttempt,
    ) -> Result<CreateOutcome, CreateFailure> {
        self.drive_native_create_with_sleep(resource, attempt, tokio::time::sleep)
            .await
    }

    async fn drive_native_create_with_sleep<Sleep, SleepFuture>(
        &self,
        resource: &Resource<impl Sized>,
        attempt: CreateAttempt,
        sleep: Sleep,
    ) -> Result<CreateOutcome, CreateFailure>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        let budget =
            self.operation_budget(resource.id.resource_type.as_str(), OperationKind::Create);
        let read_back_budget = self.read_back_budget(resource.id.resource_type.as_str());
        let outcome = drive_pending_infallible(
            budget,
            read_back_budget,
            async { native_create_step(Box::pin(self.create_step(resource, attempt)).await) },
            |pending: PendingCreate, finality| async move {
                let token = match decode_create_token(pending.token()) {
                    Ok(token) => token,
                    Err(error) => return OperationStep::Verdict(pending.preserve(error)),
                };
                let poll = match pending.into_poll() {
                    Poll::Unissued(attempt) => AwsCreatePoll::Unissued(attempt),
                    Poll::Issued(issued) => AwsCreatePoll::Issued(issued),
                };
                native_create_step(
                    Box::pin(self.poll_create_step(
                        resource,
                        token,
                        poll,
                        PollRequest { finality },
                    ))
                    .await,
                )
            },
            sleep,
        )
        .await;
        match outcome {
            DriveOutcome::Done(done) => Ok(CreateOutcome::from_host_completion(done)),
            DriveOutcome::Verdict(error) => Err(error),
            DriveOutcome::OverBudget { last_pending } => Err(last_pending.preserve(
                ProviderError::timeout(format!(
                    "create of {} did not finish within {budget:?}",
                    resource.id.resource_type
                ))
                .for_resource(resource.id.clone()),
            )),
            DriveOutcome::ProtocolViolation { last_pending } => Err(last_pending.preserve(
                ProviderError::internal("AWS create returned an invalid pending phase")
                    .for_resource(resource.id.clone()),
            )),
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
        resumed: R,
        completion: CreateCompletion,
        poll_request: PollRequest,
    ) -> Result<R::Step, R::Verdict> {
        match (poll_request.finality, completion) {
            (PollFinality::Continue, CreateCompletion::ReadBackPending { continuation, .. }) => {
                resumed.pending_read_back(continuation, POLL_AFTER_SECS)
            }
            (PollFinality::Continue | PollFinality::FinalAttempt, completion) => {
                self.finish_resumed_terminal(resumed, finish_final_read_back(completion))
            }
        }
    }

    fn finish_resumed_terminal<R: AwsResumedCreate>(
        &self,
        resumed: R,
        completion: ReadBackCompletion,
    ) -> Result<R::Step, R::Verdict> {
        match completion {
            ReadBackCompletion::Success(state) => resumed.completed(state),
            ReadBackCompletion::Partial {
                attributes,
                reason,
                missing_attributes,
                ..
            } => resumed.partial(attributes, reason, missing_attributes),
            ReadBackCompletion::Failed { observation, error } => {
                Err(resumed.failed(observation, error))
            }
        }
    }

    async fn complete_resumed_read<R: AwsResumedCreate>(
        &self,
        resource: &Resource<impl Sized>,
        resumed: R,
        outcome: ProviderResult<ReadOutcome>,
        poll_request: PollRequest,
    ) -> Result<R::Step, R::Verdict> {
        let completion = self.create_outcome_from_read(
            &resource.id,
            resumed.identifier().clone(),
            resource.resolved_attributes(),
            outcome,
        );
        self.finish_resumed_completion(resumed, completion, poll_request)
    }

    /// Perform one step while resuming a create loaded from state.
    pub async fn resume_create_step<U: AwsResumeAttempt>(
        &self,
        resource: &Resource<impl Sized>,
        token: AwsCreateToken,
        resume: AwsCreateResume<U, U::Resumed>,
        poll_request: PollRequest,
    ) -> Result<U::Step, U::Verdict> {
        let id = &resource.id;
        match token {
            AwsCreateToken::ReadBack(continuation) => {
                let resumed = match resume {
                    AwsCreateResume::Unissued(attempt) => {
                        return Err(attempt.preserve(
                            AwsCreateToken::ReadBack(continuation),
                            ProviderError::internal(
                                "read-back requires an issued resume identifier",
                            )
                            .for_resource(id.clone()),
                        ));
                    }
                    AwsCreateResume::Issued(resumed) => resumed,
                };
                if let Err(error) = continuation.validate(id, resumed.identifier()) {
                    return Err(resumed.preserve(AwsCreateToken::ReadBack(continuation), error));
                }
                let completion = self
                    .read_back_by_identifier(resource, &continuation, ReadBackKind::Create)
                    .await;
                self.finish_resumed_completion(resumed, completion, poll_request)
            }
            AwsCreateToken::AcmRequestCertificate {
                client_token,
                first_used_at_unix_secs,
                attempts_remaining,
            } => {
                let pending_token = AwsCreateToken::AcmRequestCertificate {
                    client_token: client_token.clone(),
                    first_used_at_unix_secs,
                    attempts_remaining,
                };
                let attempt = match resume {
                    AwsCreateResume::Unissued(attempt) => attempt,
                    AwsCreateResume::Issued(resumed) => {
                        return Err(resumed.failed(
                            Observation::ReadFailed(acm_request_certificate_issued_handle_error(
                                id,
                            )),
                            acm_request_certificate_issued_handle_error(id),
                        ));
                    }
                };
                let now_unix_secs = match unix_timestamp_now() {
                    Ok(timestamp) => timestamp,
                    Err(error) => {
                        let error = error.for_resource(id.clone());
                        return Err(attempt.preserve(pending_token, error));
                    }
                };
                let fresh_client_token = match fresh_acm_client_token(
                    client_token,
                    first_used_at_unix_secs,
                    now_unix_secs,
                ) {
                    Some(fresh_client_token) => fresh_client_token,
                    None => {
                        return Err(attempt
                            .preserve(pending_token, acm_request_certificate_outcome_unknown(id)));
                    }
                };
                let schema = match self.resource_schema(resource.id.resource_type.as_str()) {
                    Some(schema) => schema,
                    None => {
                        let error = ProviderError::internal("missing ACM certificate schema")
                            .for_resource(id.clone());
                        return Err(attempt.preserve(pending_token, error));
                    }
                };
                match self
                    .request_acm_certificate_transition(
                        resource,
                        schema,
                        fresh_client_token,
                        attempts_remaining,
                    )
                    .await
                {
                    AcmResendTransition::Issued(identifier) => {
                        let resumed = attempt.issued(identifier.clone());
                        self.finish_acm_certificate_resumed(
                            resource,
                            schema,
                            identifier,
                            poll_request,
                            resumed,
                        )
                        .await
                    }
                    AcmResendTransition::Pending(token) => attempt.pending(token, POLL_AFTER_SECS),
                    AcmResendTransition::InFlight(error) => {
                        Err(attempt.preserve(pending_token, error))
                    }
                }
            }
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
                                resource.get_attr("tags"),
                                None,
                            )
                            .await
                        {
                            return Err(self.failed_resumed(id, resumed, error).await);
                        }
                        let outcome = self.read_ec2_nat_gateway(id, &nat_gateway_id).await;
                        self.complete_resumed_read(resource, resumed, outcome, poll_request)
                            .await
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
                        self.complete_resumed_read(resource, resumed, outcome, poll_request)
                            .await
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
                        self.complete_resumed_read(resource, resumed, outcome, poll_request)
                            .await
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
                            AwsCreateResume::Unissued(attempt) => {
                                Err(attempt.preserve(token, error))
                            }
                            AwsCreateResume::Issued(resumed) => Err(resumed.preserve(token, error)),
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
                                self.finish_resumed_completion(resumed, completion, poll_request)
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
            AwsCreateToken::TaggedEc2 {
                kind,
                create_token,
                attempts_remaining,
                next_delay_secs,
                original_failure,
            } => {
                match self
                    .tagged_ec2_discovery(
                        resource,
                        kind,
                        create_token,
                        attempts_remaining,
                        next_delay_secs,
                        original_failure,
                    )
                    .await
                {
                    TaggedDiscovery::Retry {
                        token,
                        poll_after_secs,
                    } => match resume {
                        AwsCreateResume::Unissued(attempt) => {
                            attempt.pending(token, poll_after_secs)
                        }
                        AwsCreateResume::Issued(resumed) => resumed.pending(token, poll_after_secs),
                    },
                    TaggedDiscovery::Exhausted { token, error }
                    | TaggedDiscovery::Ambiguous { token, error } => match resume {
                        AwsCreateResume::Unissued(attempt) => Err(attempt.preserve(token, error)),
                        AwsCreateResume::Issued(resumed) => Err(resumed.preserve(token, error)),
                    },
                    TaggedDiscovery::Found(identifier) => {
                        let resumed = match resume {
                            AwsCreateResume::Unissued(attempt) => {
                                attempt.issued(identifier.clone())
                            }
                            AwsCreateResume::Issued(resumed) => resumed,
                        };
                        resumed.pending(
                            AwsCreateToken::TaggedEc2Found { kind, identifier },
                            POLL_AFTER_SECS,
                        )
                    }
                }
            }
            AwsCreateToken::TaggedEc2Found { kind, identifier } => {
                let resumed = match resume {
                    AwsCreateResume::Unissued(attempt) => attempt.issued(identifier.clone()),
                    AwsCreateResume::Issued(resumed) => resumed,
                };
                self.finish_tagged_ec2_resumed(resource, kind, identifier, resumed, poll_request)
                    .await
            }
        }
    }

    async fn drive_native_resume(
        &self,
        request: ResumeCreateRequest,
    ) -> Result<ResumeOutcome, ResumeFailure> {
        self.drive_native_resume_with_sleep(request, tokio::time::sleep)
            .await
    }

    async fn drive_native_resume_with_sleep<Sleep, SleepFuture>(
        &self,
        request: ResumeCreateRequest,
        sleep: Sleep,
    ) -> Result<ResumeOutcome, ResumeFailure>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        let budget =
            self.operation_budget(request.id().resource_type.as_str(), OperationKind::Create);
        let read_back_budget = self.read_back_budget(request.id().resource_type.as_str());
        let (resume, create_request) = request.into_parts();
        let resource = create_request.resource().as_resource();
        let outcome = drive_pending_infallible(
            budget,
            read_back_budget,
            async {
                let token = match decode_create_token(resume.token()) {
                    Ok(token) => token,
                    Err(error) => return OperationStep::Verdict(resume.preserve(error)),
                };
                let resume = match resume {
                    Resume::Unissued(attempt) => AwsCreateResume::Unissued(attempt),
                    Resume::Issued(resumed) => AwsCreateResume::Issued(resumed),
                };
                native_resume_step(
                    self.resume_create_step(
                        resource,
                        token,
                        resume,
                        PollRequest {
                            finality: PollFinality::Continue,
                        },
                    )
                    .await,
                )
            },
            |pending: NativeResumePending, finality| async move {
                let resume = match *pending.resume {
                    Resume::Unissued(attempt) => AwsCreateResume::Unissued(attempt),
                    Resume::Issued(resumed) => AwsCreateResume::Issued(resumed),
                };
                native_resume_step(
                    self.resume_create_step(
                        resource,
                        pending.token,
                        resume,
                        PollRequest { finality },
                    )
                    .await,
                )
            },
            sleep,
        )
        .await;
        match outcome {
            DriveOutcome::Done(done) => Ok(ResumeOutcome::from_host_completion(done)),
            DriveOutcome::Verdict(error) => Err(error),
            DriveOutcome::OverBudget { last_pending } => Err(last_pending.resume.preserve(
                ProviderError::timeout(format!(
                    "create of {} did not finish within {budget:?}",
                    resource.id.resource_type
                ))
                .for_resource(resource.id.clone()),
            )),
            DriveOutcome::ProtocolViolation { last_pending } => Err(last_pending.resume.preserve(
                ProviderError::internal("AWS resume returned an invalid pending phase")
                    .for_resource(resource.id.clone()),
            )),
        }
    }

    /// Dispatch a checked data source's provider-facing payload.
    ///
    /// The WASM guest bridge already receives a protocol-validated request and
    /// cannot forge carina-core's sealed provider-readiness witness, so it
    /// shares this body with the core [`Provider`] implementation.
    pub async fn read_data_source_resource(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> ProviderResult<State> {
        crate::provider_generated::dispatch_read_data_source(self, resource).await
    }

    /// Apply a checked update patch without constructing a sealed core request.
    ///
    /// This is also the WASM guest bridge entry point. The bridge receives the
    /// protocol request's current state and patch separately, while native core
    /// calls reach the same body through [`Provider::update`].
    pub async fn update_step(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        patch: &UpdatePatch,
    ) -> ProviderResult<AwsUpdateStep> {
        let id = id.clone();
        // The aws provider's per-resource `update_*` methods predate the
        // Level 3 patch contract and accept a full `to: Resource`.
        // Reconstruct that from `(from, patch)` here so each method's
        // existing logic continues to work without per-resource churn.
        // Future per-resource updates can consume `patch` directly for
        // partial-update API paths once those are wired in.
        let to = apply_patch_to_state(from, patch);
        let partial_attributes = to.resolved_attributes();
        crate::ec2_tags::validate_ec2_user_tags(&id, &partial_attributes)?;
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
                self.update_ec2_vpc_gateway_attachment(id, identifier, &to)
                    .await
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
                self.update_acm_certificate(id.clone(), identifier, from, to, schema)
                    .await?;
                let requirements =
                    self.declared_read_back_requirements(&id, &partial_attributes)?;
                let completion = self
                    .observe_read_back(
                        &id,
                        identifier.clone(),
                        partial_attributes,
                        requirements,
                        ReadBackKind::Update,
                    )
                    .await;
                return self.finish_update_read_back(
                    &id,
                    completion,
                    PollRequest {
                        finality: PollFinality::Continue,
                    },
                );
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
        let completion = self.create_outcome_from_read(
            &outcome_id,
            identifier.clone(),
            partial_attributes,
            read_back,
        );
        self.finish_update_read_back(
            &outcome_id,
            completion,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
    }

    fn finish_update_read_back(
        &self,
        id: &ResourceId,
        completion: CreateCompletion,
        poll_request: PollRequest,
    ) -> ProviderResult<AwsUpdateStep> {
        match (poll_request.finality, completion) {
            (PollFinality::Continue, CreateCompletion::ReadBackPending { continuation, .. }) => {
                Ok(OperationStep::Pending {
                    pending: AwsUpdateToken::ReadBack(continuation),
                    poll_after_secs: POLL_AFTER_SECS,
                    phase: PendingPhase::ReadBack,
                })
            }
            (PollFinality::Continue | PollFinality::FinalAttempt, completion) => {
                self.finish_update_completion(id, finish_final_read_back(completion))
            }
        }
    }

    fn finish_update_completion(
        &self,
        id: &ResourceId,
        completion: ReadBackCompletion,
    ) -> ProviderResult<AwsUpdateStep> {
        match completion {
            ReadBackCompletion::Success(state) => {
                Ok(OperationStep::Done(UpdateOutcome::success(state)))
            }
            ReadBackCompletion::Partial {
                identifier,
                attributes,
                reason,
                missing_attributes,
            } => Ok(OperationStep::Done(UpdateOutcome::partial_success(
                crate::read_model::partial_applied_state(id.clone(), identifier, attributes),
                reason.to_string(),
                missing_attributes.into(),
            ))),
            ReadBackCompletion::Failed { error, .. } => Err(error),
        }
    }

    pub async fn poll_update_step(
        &self,
        resource: &Resource<impl Sized>,
        identifier: &AppliedIdentifier,
        token: AwsUpdateToken,
        poll_request: PollRequest,
    ) -> ProviderResult<AwsUpdateStep> {
        match token {
            AwsUpdateToken::ReadBack(continuation) => {
                continuation.validate(&resource.id, identifier)?;
                let completion = self
                    .read_back_by_identifier(resource, &continuation, ReadBackKind::Update)
                    .await;
                self.finish_update_read_back(&resource.id, completion, poll_request)
            }
        }
    }

    pub async fn update_with_patch(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        patch: &UpdatePatch,
    ) -> ProviderResult<UpdateOutcome> {
        self.update_with_patch_and_sleep(id, identifier, from, patch, tokio::time::sleep)
            .await
    }

    async fn update_with_patch_and_sleep<Sleep, SleepFuture>(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        patch: &UpdatePatch,
        sleep: Sleep,
    ) -> ProviderResult<UpdateOutcome>
    where
        Sleep: FnMut(Duration) -> SleepFuture,
        SleepFuture: Future<Output = ()>,
    {
        let resource = apply_patch_to_state(from, patch);
        let completed = drive_operation(
            id,
            OperationKind::Update,
            self.operation_budget(id.resource_type.as_str(), OperationKind::Update),
            self.read_back_budget(id.resource_type.as_str()),
            self.update_step(id, identifier, from, patch),
            |token, poll_request| self.poll_update_step(&resource, identifier, token, poll_request),
            sleep,
        )
        .await?;
        Ok(UpdateOutcome::from_host_completion(completed))
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
                .delete_s3_bucket(id, identifier, request.directives())
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
                self.read_back_budget(id.resource_type.as_str()),
                self.delete_step(&id, &identifier, &request),
                |token, _poll_request| self.poll_delete_step(&id, token),
                tokio::time::sleep,
            )
            .await
            .map(CompletedOperation::into_output)
        })
    }
}

#[cfg(test)]
mod pending_operation_tests {
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::future::ready;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use aws_credential_types::provider::error::CredentialsError;
    use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
    use aws_smithy_runtime_api::client::http::{
        HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn,
    };
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_types::body::SdkBody;
    use carina_provider_protocol::OperationStep;

    use super::*;

    fn identifier(value: &str) -> AppliedIdentifier {
        AppliedIdentifier::new(value).expect("test identifier must be non-empty")
    }

    struct PendingOnlyAttempt;

    struct UnusedIssued;

    #[derive(Debug)]
    enum AcmPollVerdict {
        Preserved {
            token: AwsCreateToken,
            error: ProviderError,
        },
        Untracked(ProviderError),
    }

    struct RecordingAcmPollAttempt;

    struct RecordingAcmIssued {
        identifier: AppliedIdentifier,
    }

    impl AwsPollAttempt for RecordingAcmPollAttempt {
        type Step = AwsCreateToken;
        type Verdict = AcmPollVerdict;
        type Issued = RecordingAcmIssued;

        fn untracked(self, error: ProviderError) -> Self::Verdict {
            AcmPollVerdict::Untracked(error)
        }

        fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
            AcmPollVerdict::Preserved { token, error }
        }

        fn issued(self, identifier: AppliedIdentifier) -> Self::Issued {
            RecordingAcmIssued { identifier }
        }

        fn pending(
            self,
            token: AwsCreateToken,
            _poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            Ok(token)
        }
    }

    impl AwsIssuedCreate for RecordingAcmIssued {
        type Step = AwsCreateToken;
        type Verdict = AcmPollVerdict;

        fn identifier(&self) -> &AppliedIdentifier {
            &self.identifier
        }

        fn succeeded(
            self,
            _attributes: HashMap<String, Value>,
        ) -> Result<Self::Step, Self::Verdict> {
            unreachable!("the ACM resend tests never complete an issued create")
        }

        fn partial(
            self,
            _attributes: HashMap<String, Value>,
            _reason: ProviderError,
            _missing_attributes: NonEmptyAttributeNames,
        ) -> Result<Self::Step, Self::Verdict> {
            unreachable!("the ACM resend tests never complete an issued create")
        }

        fn pending_read_back(
            self,
            continuation: ReadBackContinuation,
            poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            self.pending(AwsCreateToken::ReadBack(continuation), poll_after_secs)
        }

        fn failed(self, _observation: Observation, _error: ProviderError) -> Self::Verdict {
            unreachable!("the ACM resend tests never issue a certificate")
        }

        fn discarded(self, _error: ProviderError) -> Self::Verdict {
            unreachable!("the ACM resend tests never issue a certificate")
        }

        fn pending(
            self,
            _token: AwsCreateToken,
            _poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            unreachable!("the ACM resend tests never issue a certificate")
        }

        fn preserve(self, _token: AwsCreateToken, _error: ProviderError) -> Self::Verdict {
            unreachable!("the ACM resend tests never issue a certificate")
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum AcmConnectorAction {
        MaybeSent,
        InvalidParameter,
        LimitExceeded,
        ServerError,
        Throttled,
    }

    #[derive(Clone, Debug)]
    struct AcmTestConnector {
        actions: Arc<Mutex<VecDeque<AcmConnectorAction>>>,
        calls: Arc<AtomicUsize>,
    }

    impl AcmTestConnector {
        fn new(actions: impl IntoIterator<Item = AcmConnectorAction>) -> Self {
            Self {
                actions: Arc::new(Mutex::new(actions.into_iter().collect())),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl HttpConnector for AcmTestConnector {
        fn call(&self, _request: HttpRequest) -> HttpConnectorFuture {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let action = self
                .actions
                .lock()
                .expect("ACM connector lock")
                .pop_front()
                .expect("ACM connector must not receive an unexpected request");
            HttpConnectorFuture::new(async move {
                match action {
                    AcmConnectorAction::MaybeSent => Err(
                        aws_smithy_runtime_api::client::result::ConnectorError::other(
                            std::io::Error::other("test ACM send outcome is unknown").into(),
                            None,
                        ),
                    ),
                    AcmConnectorAction::InvalidParameter => {
                        let mut response = HttpResponse::new(
                            400.try_into().expect("valid status"),
                            SdkBody::from(r#"{"message":"invalid certificate request"}"#),
                        );
                        response
                            .headers_mut()
                            .insert("content-type", "application/x-amz-json-1.1");
                        response
                            .headers_mut()
                            .insert("x-amzn-errortype", "InvalidParameterException");
                        Ok(response)
                    }
                    AcmConnectorAction::LimitExceeded => {
                        let mut response = HttpResponse::new(
                            400.try_into().expect("valid status"),
                            SdkBody::from(r#"{"message":"certificate quota exceeded"}"#),
                        );
                        response
                            .headers_mut()
                            .insert("content-type", "application/x-amz-json-1.1");
                        response
                            .headers_mut()
                            .insert("x-amzn-errortype", "LimitExceededException");
                        Ok(response)
                    }
                    AcmConnectorAction::ServerError => {
                        let mut response = HttpResponse::new(
                            500.try_into().expect("valid status"),
                            SdkBody::from(r#"{"message":"temporary ACM failure"}"#),
                        );
                        response
                            .headers_mut()
                            .insert("content-type", "application/x-amz-json-1.1");
                        response
                            .headers_mut()
                            .insert("x-amzn-errortype", "InternalFailure");
                        Ok(response)
                    }
                    AcmConnectorAction::Throttled => {
                        let mut response = HttpResponse::new(
                            429.try_into().expect("valid status"),
                            SdkBody::from(r#"{"message":"request throttled"}"#),
                        );
                        response
                            .headers_mut()
                            .insert("content-type", "application/x-amz-json-1.1");
                        response
                            .headers_mut()
                            .insert("x-amzn-errortype", "ThrottlingException");
                        Ok(response)
                    }
                }
            })
        }
    }

    #[derive(Clone, Debug)]
    struct AcmCredentialsThenMissing {
        calls: Arc<AtomicUsize>,
    }

    impl ProvideCredentials for AcmCredentialsThenMissing {
        fn provide_credentials<'a>(
            &'a self,
        ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                aws_credential_types::provider::future::ProvideCredentials::ready(Ok(
                    aws_credential_types::Credentials::new(
                        "test",
                        "test",
                        None,
                        None,
                        "acm-poll-test",
                    ),
                ))
            } else {
                aws_credential_types::provider::future::ProvideCredentials::ready(Err(
                    CredentialsError::not_loaded(std::io::Error::other(
                        "test ACM credentials unavailable",
                    )),
                ))
            }
        }
    }

    fn acm_test_provider(
        connector: AcmTestConnector,
        credentials_provider: SharedCredentialsProvider,
    ) -> AwsProvider {
        let selected = connector.clone();
        let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
        let bounded = crate::BoundedSdkConfig::for_test(
            aws_config::SdkConfig::builder()
                .region(aws_config::Region::new("us-east-1"))
                .endpoint_url("https://acm.example.test")
                .credentials_provider(credentials_provider)
                .http_client(http_client)
                .sleep_impl(aws_smithy_async::rt::sleep::TokioSleep::new())
                .identity_cache(aws_config::identity::IdentityCache::no_cache())
                .build(),
        );
        AwsProvider::from_sdk_config("us-east-1".to_string(), Vec::new(), Vec::new(), &bounded)
    }

    fn acm_certificate_resource() -> Resource {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name",
            Value::Concrete(carina_core::resource::ConcreteValue::String(
                "example.com".into(),
            )),
        );
        resource.set_attr(
            "validation_method",
            Value::Concrete(carina_core::resource::ConcreteValue::String("DNS".into())),
        );
        resource
    }

    fn acm_resend_token() -> AwsCreateToken {
        AwsCreateToken::AcmRequestCertificate {
            client_token: ClientToken::new(),
            first_used_at_unix_secs: unix_timestamp_now().expect("current test time"),
            attempts_remaining: ACM_REQUEST_CERTIFICATE_POLL_ATTEMPTS.get(),
        }
    }

    async fn initial_acm_maybe_sent_token(
        provider: &AwsProvider,
        resource: &Resource,
    ) -> AwsCreateToken {
        let first_used_at_unix_secs = unix_timestamp_now().expect("current test time");
        let fresh_client_token = fresh_acm_client_token(
            ClientToken::new(),
            first_used_at_unix_secs,
            first_used_at_unix_secs,
        )
        .expect("new ACM token must be fresh");
        let schema = provider
            .resource_schema("acm.Certificate")
            .expect("ACM test schema");

        match provider
            .request_initial_acm_certificate_transition(
                resource,
                schema,
                fresh_client_token,
                ACM_REQUEST_CERTIFICATE_POLL_ATTEMPTS,
            )
            .await
        {
            InitialAcmRequestTransition::Pending(token, _) => token,
            transition => panic!("initial ACM request was not MaybeSent: {transition:?}"),
        }
    }

    impl AwsCreateAttempt for PendingOnlyAttempt {
        type Step = AwsCreateToken;
        type Verdict = ProviderError;
        type Issued = UnusedIssued;

        fn untracked(self, error: ProviderError) -> Self::Verdict {
            error
        }

        fn issued(self, _identifier: AppliedIdentifier, _sent: Sent) -> Self::Issued {
            UnusedIssued
        }

        fn pending(
            self,
            token: AwsCreateToken,
            _sent: Sent,
            _poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            Ok(token)
        }
    }

    impl AwsIssuedCreate for UnusedIssued {
        type Step = AwsCreateToken;
        type Verdict = ProviderError;

        fn identifier(&self) -> &AppliedIdentifier {
            unreachable!("the pending-only test never issues a create")
        }

        fn succeeded(
            self,
            _attributes: HashMap<String, Value>,
        ) -> Result<Self::Step, Self::Verdict> {
            unreachable!("the pending-only test never succeeds")
        }

        fn partial(
            self,
            _attributes: HashMap<String, Value>,
            _reason: ProviderError,
            _missing_attributes: NonEmptyAttributeNames,
        ) -> Result<Self::Step, Self::Verdict> {
            unreachable!("the pending-only test never completes partially")
        }

        fn pending_read_back(
            self,
            continuation: ReadBackContinuation,
            poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            self.pending(AwsCreateToken::ReadBack(continuation), poll_after_secs)
        }

        fn failed(self, _observation: Observation, error: ProviderError) -> Self::Verdict {
            error
        }

        fn discarded(self, error: ProviderError) -> Self::Verdict {
            error
        }

        fn pending(
            self,
            token: AwsCreateToken,
            _poll_after_secs: u32,
        ) -> Result<Self::Step, Self::Verdict> {
            Ok(token)
        }

        fn preserve(self, _token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
            error
        }
    }

    #[test]
    fn present_write_only_attributes_are_recorded_without_a_hand_kept_name() {
        let mut resource = Resource::with_provider("aws", "ec2.Vpc", "test", None);
        resource.set_attr(
            "ipv4_ipam_pool_id",
            Value::Concrete(carina_core::resource::ConcreteValue::String(
                "ipam-pool-12345678".into(),
            )),
        );
        let sent = crate::send_models_generated::Ec2VpcCreateInput::decode(
            &resource,
            crate::test_resource_schema("ec2.Vpc"),
        )
        .expect("typed VPC input should decode")
        .sent();
        let request_attributes = resource
            .attributes
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        assert!(
            sent.names(&request_attributes)
                .contains("ipv4_ipam_pool_id")
        );
    }

    #[test]
    fn sticky_maybe_sent_failure_keeps_tagged_create_pending() {
        use aws_sdk_ec2::operation::create_nat_gateway::CreateNatGatewayError;
        use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
        use aws_smithy_runtime_api::client::result::SdkError;
        use aws_smithy_types::body::SdkBody;
        use aws_smithy_types::error::ErrorMetadata;

        let error = CreateNatGatewayError::generic(
            ErrorMetadata::builder()
                .code("RequestLimitExceeded")
                .message("throttled after an earlier timeout")
                .build(),
        );
        let mut response =
            HttpResponse::new(429.try_into().expect("valid status"), SdkBody::empty());
        response
            .headers_mut()
            .insert("x-amzn-requestid", "sticky-request-id");
        let failure = SendFailure::new(
            carina_aws_types::SendClassification::MaybeSent {
                final_attempt: carina_aws_types::FinalAttempt::Sent,
            },
            SdkError::service_error(error, response),
        );
        let create_token = ClientToken::new();
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "test", None);

        let pending = AwsProvider::tagged_ec2_send_error(
            PendingOnlyAttempt,
            &id,
            Ec2TaggedResourceKind::NatGateway,
            &create_token,
            Sent::AllRequestAttributes,
            failure,
        )
        .expect("sticky maybe-sent outcome must remain pending");

        let json = serde_json::to_value(&pending).expect("pending token should serialize");
        assert_eq!(json["phase"], "tagged_ec2");
        assert_eq!(json["kind"], "nat_gateway");
        assert_eq!(json["create_token"], create_token.as_str());
        assert_eq!(
            json["original_failure"],
            serde_json::json!({
                "operation": "ec2.CreateNatGateway",
                "status": 429,
                "code": "RequestLimitExceeded",
                "request_id": "sticky-request-id"
            })
        );
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

    #[test]
    fn tagged_ec2_token_round_trips_with_kind_token_attempts_and_next_delay() {
        let token = AwsCreateToken::TaggedEc2 {
            kind: Ec2TaggedResourceKind::Vpc,
            create_token: crate::aws_operation::ClientToken::new(),
            attempts_remaining: 8,
            next_delay_secs: 5,
            original_failure: None,
        };

        let json = serde_json::to_string(&token).expect("tagged EC2 token should serialize");
        assert_eq!(
            serde_json::from_str::<AwsCreateToken>(&json)
                .expect("tagged EC2 token should deserialize"),
            token
        );
        let object =
            serde_json::from_str::<serde_json::Value>(&json).expect("tagged EC2 token JSON");
        assert_eq!(object["phase"], "tagged_ec2");
        assert_eq!(object["kind"], "vpc");
        assert_eq!(object["attempts_remaining"], 8);
        assert_eq!(object["next_delay_secs"], 5);
        assert!(object["create_token"].as_str().is_some());
    }

    #[test]
    fn tagged_ec2_discovery_window_fits_wasm_operation_budget() {
        let mut delay = TAGGED_EC2_INITIAL_DELAY_SECS;
        let mut delays = Vec::new();
        for _ in 0..TAGGED_EC2_DISCOVERY_ATTEMPTS {
            delays.push(delay);
            delay = next_tagged_ec2_delay(delay);
        }

        assert_eq!(delays, [5, 10, 20, 30]);
        assert_eq!(delays.iter().sum::<u32>(), 65);
        assert!(delays.iter().all(|delay| *delay <= 300));
        let worst_case_secs = u64::from(delays.iter().sum::<u32>())
            + (1 + u64::from(TAGGED_EC2_DISCOVERY_ATTEMPTS)
                * u64::from(TAGGED_EC2_DISCOVERY_MAX_PAGES))
                * 90;
        assert_eq!(worst_case_secs, 875);
        assert!(worst_case_secs < DEFAULT_OPERATION_BUDGET.as_secs());
        assert_eq!(
            AwsCreateToken::TaggedEc2 {
                kind: Ec2TaggedResourceKind::Vpc,
                create_token: ClientToken::new(),
                attempts_remaining: 4,
                next_delay_secs: 60,
                original_failure: None,
            }
            .poll_after_secs(),
            60,
            "native resume must preserve the same backoff as host polling"
        );

        let continuation = AwsCreateToken::TaggedEc2Found {
            kind: Ec2TaggedResourceKind::Subnet,
            identifier: identifier("subnet-found"),
        };
        let json = serde_json::to_string(&continuation).expect("continuation should serialize");
        assert_eq!(
            serde_json::from_str::<AwsCreateToken>(&json).expect("continuation should deserialize"),
            continuation
        );
        assert_eq!(continuation.poll_after_secs(), POLL_AFTER_SECS);
    }

    #[test]
    fn acm_request_token_round_trips_and_respects_the_safety_margin() {
        let token = AwsCreateToken::AcmRequestCertificate {
            client_token: ClientToken::new(),
            first_used_at_unix_secs: 10_000,
            attempts_remaining: ACM_REQUEST_CERTIFICATE_POLL_ATTEMPTS.get(),
        };
        let json = serde_json::to_string(&token).expect("ACM request token should serialize");
        assert_eq!(
            serde_json::from_str::<AwsCreateToken>(&json)
                .expect("ACM request token should deserialize"),
            token
        );
        let object =
            serde_json::from_str::<serde_json::Value>(&json).expect("ACM request token JSON");
        assert_eq!(object["phase"], "acm_request_certificate");
        assert_eq!(object.as_object().map(|object| object.len()), Some(4));

        let safe_window = ACM_IDEMPOTENCY_WINDOW_SECS - ACM_IDEMPOTENCY_SAFETY_MARGIN_SECS;
        assert_eq!(safe_window, 45 * 60);
        let now = 20_000;
        assert!(acm_client_token_can_be_resent(now - safe_window + 1, now));
        assert!(!acm_client_token_can_be_resent(now - safe_window, now));
        assert!(!acm_client_token_can_be_resent(now + 1, now));
    }

    #[tokio::test]
    async fn acm_poll_resend_preserves_token_when_credentials_are_not_loaded() {
        let connector = AcmTestConnector::new([AcmConnectorAction::MaybeSent]);
        let credential_calls = Arc::new(AtomicUsize::new(0));
        let provider = acm_test_provider(
            connector.clone(),
            SharedCredentialsProvider::new(AcmCredentialsThenMissing {
                calls: Arc::clone(&credential_calls),
            }),
        );
        let resource = acm_certificate_resource();
        let token = initial_acm_maybe_sent_token(&provider, &resource).await;

        let result = provider
            .poll_create_step(
                &resource,
                token.clone(),
                AwsCreatePoll::Unissued(RecordingAcmPollAttempt),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await;

        match result {
            Err(AcmPollVerdict::Preserved {
                token: preserved,
                error,
            }) => {
                assert_eq!(preserved, token);
                assert!(
                    error.to_string().contains("credentials unavailable"),
                    "{error}"
                );
            }
            Err(AcmPollVerdict::Untracked(error)) => {
                panic!("ACM continuation was discarded: {error}")
            }
            Ok(next) => panic!("permanent credential failure unexpectedly repended: {next:?}"),
        }
        assert_eq!(credential_calls.load(Ordering::Relaxed), 2);
        assert_eq!(connector.calls(), 1);
    }

    #[tokio::test]
    async fn acm_poll_resend_preserves_token_after_rejection() {
        let connector = AcmTestConnector::new([
            AcmConnectorAction::MaybeSent,
            AcmConnectorAction::InvalidParameter,
        ]);
        let credentials =
            aws_credential_types::Credentials::new("test", "test", None, None, "acm-poll-test");
        let provider = acm_test_provider(
            connector.clone(),
            SharedCredentialsProvider::new(credentials),
        );
        let resource = acm_certificate_resource();
        let token = initial_acm_maybe_sent_token(&provider, &resource).await;

        let result = provider
            .poll_create_step(
                &resource,
                token.clone(),
                AwsCreatePoll::Unissued(RecordingAcmPollAttempt),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await;

        match result {
            Err(AcmPollVerdict::Preserved {
                token: preserved,
                error,
            }) => {
                assert_eq!(preserved, token);
                assert!(
                    error.to_string().contains(
                        "acm.Certificate may have been created; check before re-applying"
                    ),
                    "{error}"
                );
            }
            Err(AcmPollVerdict::Untracked(error)) => {
                panic!("ACM continuation was discarded: {error}")
            }
            Ok(next) => panic!("non-retryable rejection unexpectedly repended: {next:?}"),
        }
        assert_eq!(connector.calls(), 2);
    }

    #[tokio::test]
    async fn acm_poll_resend_uses_final_non_retryable_verdict() {
        let connector = AcmTestConnector::new([
            AcmConnectorAction::MaybeSent,
            AcmConnectorAction::ServerError,
            AcmConnectorAction::InvalidParameter,
        ]);
        let credentials =
            aws_credential_types::Credentials::new("test", "test", None, None, "acm-poll-test");
        let provider = acm_test_provider(
            connector.clone(),
            SharedCredentialsProvider::new(credentials),
        );
        let resource = acm_certificate_resource();
        let token = initial_acm_maybe_sent_token(&provider, &resource).await;

        let result = provider
            .poll_create_step(
                &resource,
                token.clone(),
                AwsCreatePoll::Unissued(RecordingAcmPollAttempt),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await;

        match result {
            Err(AcmPollVerdict::Preserved {
                token: preserved,
                error,
            }) => {
                assert_eq!(preserved, token);
                assert!(
                    error.to_string().contains(
                        "acm.Certificate may have been created; check before re-applying"
                    ),
                    "{error}"
                );
            }
            Err(AcmPollVerdict::Untracked(error)) => {
                panic!("ACM continuation was discarded: {error}")
            }
            Ok(next) => panic!("non-retryable final attempt unexpectedly repended: {next:?}"),
        }
        assert_eq!(connector.calls(), 3);
    }

    #[tokio::test]
    async fn acm_poll_resend_keeps_limit_exceeded_in_flight() {
        let connector = AcmTestConnector::new([
            AcmConnectorAction::MaybeSent,
            AcmConnectorAction::LimitExceeded,
            AcmConnectorAction::LimitExceeded,
            AcmConnectorAction::LimitExceeded,
        ]);
        let credentials =
            aws_credential_types::Credentials::new("test", "test", None, None, "acm-poll-test");
        let provider = acm_test_provider(
            connector.clone(),
            SharedCredentialsProvider::new(credentials),
        );
        let resource = acm_certificate_resource();
        let token = initial_acm_maybe_sent_token(&provider, &resource).await;

        let result = provider
            .poll_create_step(
                &resource,
                token.clone(),
                AwsCreatePoll::Unissued(RecordingAcmPollAttempt),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await;

        match result {
            Err(AcmPollVerdict::Preserved {
                token: preserved,
                error,
            }) => {
                assert_eq!(preserved, token);
                assert!(
                    error.to_string().contains(
                        "acm.Certificate may have been created; check before re-applying"
                    ),
                    "{error}"
                );
            }
            Err(AcmPollVerdict::Untracked(error)) => {
                panic!("ACM continuation was discarded: {error}")
            }
            Ok(next) => panic!("quota failure unexpectedly repended: {next:?}"),
        }
        // The SDK exhausts its three internal attempts; the provider does not
        // schedule another poll resend for this unmodeled quota error.
        assert_eq!(connector.calls(), 4);
    }

    #[tokio::test]
    async fn acm_poll_resend_keeps_unmodeled_throttling_in_flight() {
        let token = acm_resend_token();
        let connector = AcmTestConnector::new([
            AcmConnectorAction::Throttled,
            AcmConnectorAction::Throttled,
            AcmConnectorAction::Throttled,
        ]);
        let credentials =
            aws_credential_types::Credentials::new("test", "test", None, None, "acm-poll-test");
        let provider = acm_test_provider(
            connector.clone(),
            SharedCredentialsProvider::new(credentials),
        );

        let result = provider
            .poll_create_step(
                &acm_certificate_resource(),
                token.clone(),
                AwsCreatePoll::Unissued(RecordingAcmPollAttempt),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await;

        match result {
            Err(AcmPollVerdict::Preserved {
                token: preserved,
                error,
            }) => {
                assert_eq!(preserved, token);
                assert!(
                    error.to_string().contains(
                        "acm.Certificate may have been created; check before re-applying"
                    ),
                    "{error}"
                );
            }
            Err(AcmPollVerdict::Untracked(error)) => {
                panic!("ACM continuation was discarded: {error}")
            }
            Ok(next) => panic!("unmodeled throttling unexpectedly repended: {next:?}"),
        }
        assert_eq!(connector.calls(), 3);
    }

    fn vpc_page(ids: &[&str]) -> aws_sdk_ec2::operation::describe_vpcs::DescribeVpcsOutput {
        aws_sdk_ec2::operation::describe_vpcs::DescribeVpcsOutput::builder()
            .set_vpcs(Some(
                ids.iter()
                    .map(|id| aws_sdk_ec2::types::Vpc::builder().vpc_id(*id).build())
                    .collect(),
            ))
            .build()
    }

    #[test]
    fn describe_ids_considers_a_match_after_an_empty_first_page() {
        let pages = [vpc_page(&[]), vpc_page(&["vpc-second-page"])];

        let ids = describe_ids_from_pages(&pages, |page| page.vpcs(), |vpc| vpc.vpc_id());

        assert_eq!(ids, ["vpc-second-page"]);
    }

    #[test]
    fn describe_ids_keeps_multiple_matches_across_pages() {
        let pages = [vpc_page(&["vpc-first"]), vpc_page(&["vpc-second"])];

        let ids = describe_ids_from_pages(&pages, |page| page.vpcs(), |vpc| vpc.vpc_id());

        assert_eq!(ids, ["vpc-first", "vpc-second"]);
    }

    #[test]
    fn tagged_ec2_outcome_unknown_error_names_the_resource() {
        let id = ResourceId::with_provider_identity("aws", "ec2.Vpc", "example", None);
        let error = AwsProvider::tagged_ec2_outcome_unknown_error(
            &id,
            Ec2TaggedResourceKind::Vpc,
            &ClientToken::new(),
            None,
            None,
        );

        assert_eq!(
            error
                .to_wire()
                .resource_id
                .as_ref()
                .map(|resource_id| resource_id.identity.as_str()),
            Some("example")
        );
    }

    #[tokio::test]
    async fn drive_operation_returns_done_value_from_poll() {
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "example", None);
        let expected = crate::read_model::test_applied_state(
            id.clone(),
            identifier("nat-read-back"),
            HashMap::new(),
        );
        let mut done = Some(expected.clone());

        let outcome = drive_operation(
            &id,
            OperationKind::Create,
            Duration::from_secs(30),
            DEFAULT_READ_BACK_BUDGET,
            ready(Ok(OperationStep::Pending {
                pending: AwsCreateToken::NatGateway {
                    nat_gateway_id: identifier("nat-123"),
                },
                poll_after_secs: 5,
                phase: PendingPhase::Operation,
            })),
            |_, _| ready(Ok(OperationStep::Done(done.take().expect("one poll")))),
            |_| ready(()),
        )
        .await
        .expect("pending create should settle");

        assert_eq!(outcome.into_output(), expected);
    }

    #[tokio::test]
    async fn native_create_driver_maps_over_budget_to_resource_timeout() {
        let id = ResourceId::with_provider_identity("aws", "ec2.NatGateway", "example", None);
        let result: ProviderResult<CompletedOperation<()>> = drive_operation(
            &id,
            OperationKind::Create,
            Duration::ZERO,
            DEFAULT_READ_BACK_BUDGET,
            ready(Ok(OperationStep::Pending {
                pending: AwsCreateToken::NatGateway {
                    nat_gateway_id: identifier("nat-123"),
                },
                poll_after_secs: 5,
                phase: PendingPhase::Operation,
            })),
            |token, _| {
                ready(Ok(OperationStep::Pending {
                    pending: token,
                    poll_after_secs: 5,
                    phase: PendingPhase::Operation,
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
