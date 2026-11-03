//! Provider-level coverage for host-driven pending operations against a
//! scripted in-process EC2 Query service. No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use aws_smithy_runtime_api::client::http::{HttpClient, SharedHttpConnector, http_client_fn};
use carina_core::provider::{
    AppliedIdentifier, AppliedState, CreateFailure, CreateRequest, DeleteRequest, Observation,
    PatchOp, ProviderDirectiveSchema, ProviderError, ReadOutcome, Sent, UpdatePatch,
};
use carina_core::resource::{
    ConcreteValue, Directives, Resource, ResourceId, SnapshotState, Value,
};
use carina_plugin_sdk::PhasedToken;
use carina_provider_aws::{
    AwsCreateAttempt, AwsCreateOperation, AwsCreatePoll, AwsCreateResume, AwsCreateToken,
    AwsDeleteToken, AwsIssuedCreate, AwsNormalizer, AwsPollAttempt, AwsProvider, AwsResumeAttempt,
    AwsResumedCreate, BoundedSdkConfig, Ec2TaggedResourceKind,
};
use carina_provider_protocol::{OperationStep, PollFinality, PollRequest};
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

const REGION: &str = "us-east-1";
const NAT_GATEWAY_ID: &str = "nat-0123456789abcdef0";
const SUBNET_ID: &str = "subnet-0123456789abcdef0";
const RESOURCE_TYPE: &str = "ec2.NatGateway";
const FLOW_LOG_ID: &str = "fl-0123456789abcdef0";
const FLOW_LOG_RESOURCE_ID: &str = "vpc-0123456789abcdef0";
const REJECTED_FLOW_LOG_RESOURCE_ID: &str = "vpc-0fedcba9876543210";
const ACM_CERTIFICATE_ARN: &str =
    "arn:aws:acm:us-east-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef";
const ACM_CLIENT_TOKEN: &str = "abcdef0123456789abcdef0123456789";

#[derive(Debug)]
enum TestCreateStep {
    Success(TestAppliedState),
    Partial {
        state: TestAppliedState,
        reason: String,
        missing_attributes: Vec<String>,
    },
    Pending {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
        poll_after_secs: u32,
    },
}

#[derive(Debug)]
struct TestAppliedState {
    _id: ResourceId,
    identifier: AppliedIdentifier,
    _attributes: std::collections::HashMap<String, Value>,
}

impl TestAppliedState {
    fn new(
        id: ResourceId,
        identifier: AppliedIdentifier,
        attributes: std::collections::HashMap<String, Value>,
    ) -> Self {
        Self {
            _id: id,
            identifier,
            _attributes: attributes,
        }
    }

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }
}

#[derive(Debug)]
enum TestCreateVerdict {
    NotCreated(ProviderError),
    InFlight {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
        error: ProviderError,
    },
    IdentifierReported(Box<TestIdentifierReportedVerdict>),
    NoResource(ProviderError),
}

#[derive(Debug)]
struct TestIdentifierReportedVerdict {
    identifier: AppliedIdentifier,
    observation: Observation,
    sent: Sent,
    error: ProviderError,
}

impl TestCreateVerdict {
    fn provider_error(&self) -> &ProviderError {
        match self {
            Self::NotCreated(error) | Self::NoResource(error) => error,
            Self::InFlight { error, .. } => error,
            Self::IdentifierReported(reported) => &reported.error,
        }
    }

    fn in_flight(&self) -> (&AwsCreateToken, Option<&AppliedIdentifier>) {
        match self {
            Self::InFlight {
                token, identifier, ..
            } => (token, identifier.as_ref()),
            other => panic!("expected in-flight create verdict, got {other:?}"),
        }
    }
}

struct TestCreateAttempt {
    id: ResourceId,
}

struct TestPollAttempt {
    id: ResourceId,
}

struct TestIssuedCreate {
    id: ResourceId,
    identifier: AppliedIdentifier,
    sent: Sent,
}

impl AwsCreateAttempt for TestCreateAttempt {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;
    type Issued = TestIssuedCreate;

    fn not_created(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::NotCreated(error)
    }

    fn issued(self, identifier: AppliedIdentifier, sent: Sent) -> Self::Issued {
        TestIssuedCreate {
            id: self.id,
            identifier,
            sent,
        }
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        _sent: Sent,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token: token.into(),
            identifier: None,
            poll_after_secs,
        })
    }
}

impl AwsPollAttempt for TestPollAttempt {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;
    type Issued = TestIssuedCreate;

    fn not_created(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::NotCreated(error)
    }

    fn preserve(self, token: AwsCreateOperation, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::InFlight {
            token: token.into(),
            identifier: None,
            error,
        }
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Issued {
        TestIssuedCreate {
            id: self.id,
            identifier,
            sent: Sent::AllRequestAttributes,
        }
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token: token.into(),
            identifier: None,
            poll_after_secs,
        })
    }
}

impl AwsIssuedCreate for TestIssuedCreate {
    type Step = TestCreateStep;
    type Verdict = TestCreateVerdict;

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    fn succeeded(
        self,
        attributes: std::collections::HashMap<String, Value>,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Success(TestAppliedState::new(
            self.id,
            self.identifier,
            attributes,
        )))
    }

    fn partial(
        self,
        attributes: std::collections::HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: carina_provider_aws::NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Partial {
            state: TestAppliedState::new(self.id, self.identifier, attributes),
            reason: reason.to_string(),
            missing_attributes: missing_attributes.into(),
        })
    }

    fn identifier_reported(self, observation: Observation, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::IdentifierReported(Box::new(TestIdentifierReportedVerdict {
            identifier: self.identifier,
            observation,
            sent: self.sent,
            error,
        }))
    }

    fn no_resource(self, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::NoResource(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestCreateStep::Pending {
            token,
            identifier: Some(self.identifier),
            poll_after_secs,
        })
    }

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        TestCreateVerdict::InFlight {
            token,
            identifier: Some(self.identifier),
            error,
        }
    }
}

#[derive(Debug)]
enum TestResumeStep {
    Completed(AppliedState),
    Partial {
        identifier: AppliedIdentifier,
        attributes: std::collections::HashMap<String, Value>,
        reason: String,
        missing_attributes: Vec<String>,
    },
    Pending {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
        poll_after_secs: u32,
    },
}

#[derive(Debug)]
enum TestResumeVerdict {
    NotCreated(ProviderError),
    InFlight {
        token: AwsCreateToken,
        identifier: Option<AppliedIdentifier>,
        error: ProviderError,
    },
    IdentifierReported(ProviderError),
    NoResource(ProviderError),
}

impl TestResumeVerdict {
    fn provider_error(&self) -> &ProviderError {
        match self {
            Self::NotCreated(error) | Self::IdentifierReported(error) | Self::NoResource(error) => {
                error
            }
            Self::InFlight { error, .. } => error,
        }
    }

    fn in_flight(&self) -> (&AwsCreateToken, Option<&AppliedIdentifier>) {
        match self {
            Self::InFlight {
                token, identifier, ..
            } => (token, identifier.as_ref()),
            other => panic!("expected in-flight resume verdict, got {other:?}"),
        }
    }
}

struct TestResumeAttempt;

struct TestResumedCreate {
    identifier: AppliedIdentifier,
}

impl AwsResumeAttempt for TestResumeAttempt {
    type Step = TestResumeStep;
    type Verdict = TestResumeVerdict;
    type Resumed = TestResumedCreate;

    fn not_created(self, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::NotCreated(error)
    }

    fn preserve(self, token: AwsCreateOperation, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::InFlight {
            token: token.into(),
            identifier: None,
            error,
        }
    }

    fn issued(self, identifier: AppliedIdentifier) -> Self::Resumed {
        TestResumedCreate { identifier }
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Pending {
            token: token.into(),
            identifier: None,
            poll_after_secs,
        })
    }
}

fn delete_request() -> DeleteRequest {
    DeleteRequest::resolve(&Directives::default(), ProviderDirectiveSchema::Unavailable)
}

impl AwsResumedCreate for TestResumedCreate {
    type Step = TestResumeStep;
    type Verdict = TestResumeVerdict;

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    fn completed(self, state: AppliedState) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Completed(state))
    }

    fn partial(
        self,
        attributes: std::collections::HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: carina_provider_aws::NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Partial {
            identifier: self.identifier,
            attributes,
            reason: reason.to_string(),
            missing_attributes: missing_attributes.into(),
        })
    }

    fn identifier_reported(self, _observation: Observation, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::IdentifierReported(error)
    }

    fn no_resource(self, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::NoResource(error)
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(TestResumeStep::Pending {
            token,
            identifier: Some(self.identifier),
            poll_after_secs,
        })
    }

    fn preserve(self, token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        TestResumeVerdict::InFlight {
            token,
            identifier: Some(self.identifier),
            error,
        }
    }
}

struct ScriptedEc2Service {
    describe_states: Mutex<VecDeque<String>>,
    reject_create: bool,
    reject_tags: bool,
    create_nat_gateway_calls: AtomicUsize,
    create_tags_calls: AtomicUsize,
    describe_nat_gateways_calls: AtomicUsize,
    delete_nat_gateway_calls: AtomicUsize,
}

impl ScriptedEc2Service {
    fn new(describe_states: &[&str]) -> Self {
        Self::new_with_failures(describe_states, false, false)
    }

    fn new_with_failures(describe_states: &[&str], reject_create: bool, reject_tags: bool) -> Self {
        assert!(
            !describe_states.is_empty(),
            "a scripted service needs at least one describe state"
        );
        Self {
            describe_states: Mutex::new(
                describe_states
                    .iter()
                    .map(|state| (*state).to_string())
                    .collect(),
            ),
            reject_create,
            reject_tags,
            create_nat_gateway_calls: AtomicUsize::new(0),
            create_tags_calls: AtomicUsize::new(0),
            describe_nat_gateways_calls: AtomicUsize::new(0),
            delete_nat_gateway_calls: AtomicUsize::new(0),
        }
    }

    fn create_nat_gateway_calls(&self) -> usize {
        self.create_nat_gateway_calls.load(Ordering::Relaxed)
    }

    fn create_tags_calls(&self) -> usize {
        self.create_tags_calls.load(Ordering::Relaxed)
    }

    fn describe_nat_gateways_calls(&self) -> usize {
        self.describe_nat_gateways_calls.load(Ordering::Relaxed)
    }

    fn delete_nat_gateway_calls(&self) -> usize {
        self.delete_nat_gateway_calls.load(Ordering::Relaxed)
    }

    fn next_describe_state(&self) -> String {
        let mut states = self
            .describe_states
            .lock()
            .expect("describe-state script lock");
        if states.len() > 1 {
            states.pop_front().expect("non-empty describe-state script")
        } else {
            states
                .front()
                .expect("non-empty describe-state script")
                .clone()
        }
    }

    fn response_for(&self, request: &MockRequest) -> MockResponse {
        assert_eq!(request.method, "POST", "EC2 Query requests use POST");
        match query_action(request) {
            "CreateNatGateway" => {
                self.create_nat_gateway_calls
                    .fetch_add(1, Ordering::Relaxed);
                if self.reject_create {
                    MockResponse::error(400, "InvalidParameterValue", "scripted create rejection")
                } else {
                    MockResponse::xml(200, create_nat_gateway_response())
                }
            }
            "CreateTags" => {
                self.create_tags_calls.fetch_add(1, Ordering::Relaxed);
                if self.reject_tags {
                    MockResponse::error(400, "InvalidParameterValue", "scripted tag rejection")
                } else {
                    MockResponse::xml(
                        200,
                        r#"<CreateTagsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"/>"#,
                    )
                }
            }
            "DescribeNatGateways" => {
                self.describe_nat_gateways_calls
                    .fetch_add(1, Ordering::Relaxed);
                let state = self.next_describe_state();
                if state == "read_error" {
                    MockResponse::error(
                        400,
                        "InvalidParameterValue",
                        "scripted NAT gateway read-back failure",
                    )
                } else {
                    MockResponse::xml(200, describe_nat_gateways_response(&state))
                }
            }
            "DeleteNatGateway" => {
                self.delete_nat_gateway_calls
                    .fetch_add(1, Ordering::Relaxed);
                MockResponse::xml(200, delete_nat_gateway_response())
            }
            action => panic!("unexpected EC2 action {action}: {:?}", request.body),
        }
    }
}

impl MockService for ScriptedEc2Service {
    fn service_name(&self) -> &str {
        "ec2"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = self.response_for(&request);
        Box::pin(async move { response })
    }
}

struct PartialFlowLogEc2Service;

impl PartialFlowLogEc2Service {
    fn response_for(request: &MockRequest) -> MockResponse {
        assert_eq!(request.method, "POST", "EC2 Query requests use POST");
        match query_action(request) {
            "CreateFlowLogs" => MockResponse::xml(200, partial_create_flow_logs_response()),
            "DescribeFlowLogs" => MockResponse::xml(200, empty_describe_flow_logs_response()),
            action => panic!("unexpected EC2 action {action}: {:?}", request.body),
        }
    }
}

struct TaggedFlowLogEc2Service {
    identifiers: Vec<String>,
}

impl MockService for TaggedFlowLogEc2Service {
    fn service_name(&self) -> &str {
        "ec2"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        assert_eq!(query_action(&request), "DescribeFlowLogs");
        let items = self
            .identifiers
            .iter()
            .enumerate()
            .map(|(index, identifier)| {
                format!(
                    "<item><flowLogId>{identifier}</flowLogId><resourceId>vpc-{index:017x}</resourceId><flowLogStatus>ACTIVE</flowLogStatus></item>"
                )
            })
            .collect::<String>();
        let response = MockResponse::xml(
            200,
            format!(
                r#"<DescribeFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>flow-log-discovery</requestId><flowLogSet>{items}</flowLogSet></DescribeFlowLogsResponse>"#
            ),
        );
        Box::pin(async move { response })
    }
}

#[derive(Clone, Copy)]
enum AcmRequestResult {
    Success,
    Rejected,
}

#[derive(Clone, Copy)]
enum AcmDescribeResult {
    Incomplete,
    PartialDvo,
    Populated,
    NotFound,
    Error,
    AccessDenied,
    Throttled,
    ServerError,
    DecodeError,
    Failed,
}

struct ScriptedAcmService {
    request_result: AcmRequestResult,
    request_bodies: Arc<Mutex<Vec<String>>>,
    describe_dvo: Mutex<VecDeque<AcmDescribeResult>>,
    update_calls: Arc<AtomicUsize>,
    describe_calls: Arc<AtomicUsize>,
}

impl MockService for ScriptedAcmService {
    fn service_name(&self) -> &str {
        "acm"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://acm\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let target = request
            .headers
            .get("x-amz-target")
            .and_then(|value| value.to_str().ok())
            .expect("ACM request should carry x-amz-target");
        let response = match target {
            "CertificateManager.RequestCertificate" => {
                self.request_bodies
                    .lock()
                    .expect("ACM request-body lock")
                    .push(
                        str::from_utf8(&request.body)
                            .expect("ACM request body should be UTF-8")
                            .to_string(),
                    );
                match self.request_result {
                    AcmRequestResult::Success => MockResponse::json(
                        200,
                        format!(r#"{{"CertificateArn":"{ACM_CERTIFICATE_ARN}"}}"#),
                    ),
                    AcmRequestResult::Rejected => MockResponse::json(
                        400,
                        r#"{"__type":"InvalidParameterException","message":"scripted ACM rejection"}"#,
                    ),
                }
            }
            "CertificateManager.DescribeCertificate" => {
                self.describe_calls.fetch_add(1, Ordering::Relaxed);
                let mut script = self.describe_dvo.lock().unwrap();
                let state = if script.len() > 1 {
                    script.pop_front().unwrap()
                } else {
                    *script.front().unwrap_or(&AcmDescribeResult::Incomplete)
                };
                let dvo = match state {
                    AcmDescribeResult::Populated => {
                        r#","DomainValidationOptions":[{"DomainName":"example.com","ValidationMethod":"DNS","ResourceRecord":{"Name":"_abc.example.com.","Type":"CNAME","Value":"_xyz.acm-validations.aws."}}]"#
                    }
                    AcmDescribeResult::PartialDvo => {
                        r#","DomainValidationOptions":[{"DomainName":"example.com","ValidationMethod":"DNS"}]"#
                    }
                    AcmDescribeResult::NotFound => {
                        return Box::pin(async {
                            MockResponse::json(
                                400,
                                r#"{"__type":"ResourceNotFoundException","message":"not visible yet"}"#,
                            )
                        });
                    }
                    AcmDescribeResult::Error => {
                        return Box::pin(async {
                            MockResponse::json(
                                400,
                                r#"{"__type":"InvalidStateException","message":"read failed"}"#,
                            )
                        });
                    }
                    AcmDescribeResult::AccessDenied => {
                        return Box::pin(async {
                            MockResponse::json(
                                403,
                                r#"{"__type":"AccessDeniedException","message":"read access denied"}"#,
                            )
                        });
                    }
                    AcmDescribeResult::Throttled => {
                        return Box::pin(async {
                            MockResponse::json(
                                400,
                                r#"{"__type":"ThrottlingException","message":"read throttled"}"#,
                            )
                        });
                    }
                    AcmDescribeResult::ServerError => {
                        return Box::pin(async {
                            MockResponse::json(
                                503,
                                r#"{"__type":"Unavailable","message":"read unavailable"}"#,
                            )
                        });
                    }
                    AcmDescribeResult::DecodeError => {
                        return Box::pin(async {
                            MockResponse::json(200, r#"{"Certificate":{"DomainName":123}}"#)
                        });
                    }
                    _ => "",
                };
                let status = if matches!(state, AcmDescribeResult::Failed) {
                    "FAILED"
                } else {
                    "PENDING_VALIDATION"
                };
                MockResponse::json(
                    200,
                    format!(
                        r#"{{"Certificate":{{"CertificateArn":"{ACM_CERTIFICATE_ARN}","DomainName":"example.com","Status":"{status}","FailureReason":"DOMAIN_NOT_ALLOWED","Type":"AMAZON_ISSUED","KeyAlgorithm":"RSA_2048"{dvo}}}}}"#
                    ),
                )
            }
            "CertificateManager.AddTagsToCertificate"
            | "CertificateManager.RemoveTagsFromCertificate" => {
                self.update_calls.fetch_add(1, Ordering::Relaxed);
                MockResponse::json(200, "{}")
            }
            "CertificateManager.ListTagsForCertificate" => {
                MockResponse::json(200, r#"{"Tags":[]}"#)
            }
            operation => panic!("unexpected ACM operation {operation}"),
        };
        Box::pin(async move { response })
    }
}

#[derive(Clone, Debug)]
struct RecordingPostSendFailure {
    expected_body_fragment: &'static str,
    request_bodies: Arc<Mutex<Vec<String>>>,
}

impl aws_smithy_runtime_api::client::http::HttpConnector for RecordingPostSendFailure {
    fn call(
        &self,
        request: aws_smithy_runtime_api::client::orchestrator::HttpRequest,
    ) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        let body = str::from_utf8(request.body().bytes().unwrap_or_default())
            .expect("AWS request body should be UTF-8")
            .to_string();
        assert!(body.contains(self.expected_body_fragment), "{body}");
        self.request_bodies
            .lock()
            .expect("recorded request-body lock")
            .push(body);
        aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(async {
            Err(aws_smithy_runtime_api::client::result::ConnectorError::io(
                std::io::Error::other("connection reset after request send").into(),
            ))
        })
    }
}

struct TaggedVpcEc2Service {
    identifiers: Vec<String>,
    discovery_error: bool,
    repeat_next_token: bool,
    describe_calls: Arc<AtomicUsize>,
}

impl MockService for TaggedVpcEc2Service {
    fn service_name(&self) -> &str {
        "ec2"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match query_action(&request) {
            "DescribeVpcs" => {
                self.describe_calls.fetch_add(1, Ordering::Relaxed);
                if self.discovery_error {
                    return Box::pin(async {
                        MockResponse::xml(
                            403,
                            r#"<Response><Errors><Error><Code>UnauthorizedOperation</Code><Message>scripted discovery denial</Message></Error></Errors><RequestID>discovery-request-id</RequestID></Response>"#,
                        )
                    });
                }
                let items = self
                    .identifiers
                    .iter()
                    .map(|identifier| format!("<item><vpcId>{identifier}</vpcId><cidrBlock>10.0.0.0/16</cidrBlock><state>available</state></item>"))
                    .collect::<String>();
                let next_token = if self.repeat_next_token {
                    "<nextToken>another-page</nextToken>"
                } else {
                    ""
                };
                MockResponse::xml(
                    200,
                    format!(
                        r#"<DescribeVpcsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>request-id</requestId><vpcSet>{items}</vpcSet>{next_token}</DescribeVpcsResponse>"#
                    ),
                )
            }
            "DescribeVpcAttribute" => MockResponse::xml(
                200,
                r#"<DescribeVpcAttributeResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"><requestId>request-id</requestId><enableDnsSupport><value>true</value></enableDnsSupport><enableDnsHostnames><value>false</value></enableDnsHostnames></DescribeVpcAttributeResponse>"#,
            ),
            action => panic!("unexpected EC2 action {action}: {:?}", request.body),
        };
        Box::pin(async move { response })
    }
}

#[derive(Clone, Debug)]
struct PostSendVpcFailure {
    calls: Arc<AtomicUsize>,
    expected_action: &'static str,
}

impl aws_smithy_runtime_api::client::http::HttpConnector for PostSendVpcFailure {
    fn call(
        &self,
        request: aws_smithy_runtime_api::client::orchestrator::HttpRequest,
    ) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        let body = request.body().bytes().unwrap_or_default();
        assert!(
            str::from_utf8(body)
                .expect("EC2 Query body")
                .contains(self.expected_action)
        );
        self.calls.fetch_add(1, Ordering::Relaxed);
        aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(async {
            Err(aws_smithy_runtime_api::client::result::ConnectorError::io(
                std::io::Error::other("connection reset after request send").into(),
            ))
        })
    }
}

impl MockService for PartialFlowLogEc2Service {
    fn service_name(&self) -> &str {
        "ec2"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = Self::response_for(&request);
        Box::pin(async move { response })
    }
}

fn query_action(request: &MockRequest) -> &str {
    str::from_utf8(&request.body)
        .expect("EC2 Query body should be UTF-8")
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("Action="))
        .expect("EC2 Query body should contain Action")
}

fn create_nat_gateway_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000001</requestId>
  <natGateway>
    <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
    <state>pending</state>
    <subnetId>{SUBNET_ID}</subnetId>
  </natGateway>
</CreateNatGatewayResponse>"#
    )
}

fn describe_nat_gateways_response(state: &str) -> String {
    if state == "not_found" {
        return r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeNatGatewaysResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000002</requestId>
  <natGatewaySet/>
</DescribeNatGatewaysResponse>"#
            .to_string();
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeNatGatewaysResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000002</requestId>
  <natGatewaySet>
    <item>
      <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
      <state>{state}</state>
      <subnetId>{SUBNET_ID}</subnetId>
    </item>
  </natGatewaySet>
</DescribeNatGatewaysResponse>"#
    )
}

fn delete_nat_gateway_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteNatGatewayResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000003</requestId>
  <natGatewayId>{NAT_GATEWAY_ID}</natGatewayId>
</DeleteNatGatewayResponse>"#
    )
}

fn partial_create_flow_logs_response() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CreateFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000004</requestId>
  <flowLogIdSet>
    <item>{FLOW_LOG_ID}</item>
  </flowLogIdSet>
  <unsuccessful>
    <item>
      <error>
        <code>InvalidParameterValue</code>
        <message>scripted per-resource rejection</message>
      </error>
      <resourceId>{REJECTED_FLOW_LOG_RESOURCE_ID}</resourceId>
    </item>
  </unsuccessful>
</CreateFlowLogsResponse>"#
    )
}

fn empty_describe_flow_logs_response() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeFlowLogsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>00000000-0000-0000-0000-000000000005</requestId>
  <flowLogSet/>
</DescribeFlowLogsResponse>"#
}

fn provider_from_config(sdk_config: aws_types::SdkConfig) -> AwsProvider {
    AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(sdk_config),
    )
}

async fn provider_with_ec2(describe_states: &[&str]) -> (AwsProvider, Arc<ScriptedEc2Service>) {
    let service = Arc::new(ScriptedEc2Service::new(describe_states));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    (provider_from_config(sdk_config), service)
}

async fn provider_with_ec2_failures(
    describe_states: &[&str],
    reject_create: bool,
    reject_tags: bool,
) -> (AwsProvider, Arc<ScriptedEc2Service>) {
    let service = Arc::new(ScriptedEc2Service::new_with_failures(
        describe_states,
        reject_create,
        reject_tags,
    ));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    (provider_from_config(sdk_config), service)
}

async fn provider_with_partial_flow_log() -> AwsProvider {
    let mock = MockAws::builder()
        .with_service(PartialFlowLogEc2Service)
        .build();
    let sdk_config = mock.sdk_config(REGION).await;
    provider_from_config(sdk_config)
}

async fn provider_with_tagged_flow_logs(identifiers: &[&str]) -> AwsProvider {
    let mock = MockAws::builder()
        .with_service(TaggedFlowLogEc2Service {
            identifiers: identifiers
                .iter()
                .map(|identifier| (*identifier).to_string())
                .collect(),
        })
        .build();
    provider_from_config(mock.sdk_config(REGION).await)
}

async fn provider_with_acm(
    request_result: AcmRequestResult,
) -> (AwsProvider, Arc<Mutex<Vec<String>>>) {
    let request_bodies = Arc::new(Mutex::new(Vec::new()));
    let mock = MockAws::builder()
        .with_service(ScriptedAcmService {
            request_result,
            request_bodies: Arc::clone(&request_bodies),
            describe_dvo: Mutex::new(VecDeque::new()),
            describe_calls: Arc::new(AtomicUsize::new(0)),
            update_calls: Arc::new(AtomicUsize::new(0)),
        })
        .build();
    (
        provider_from_config(mock.sdk_config(REGION).await),
        request_bodies,
    )
}

async fn provider_with_recording_post_send_failure(
    expected_body_fragment: &'static str,
) -> (AwsProvider, Arc<Mutex<Vec<String>>>) {
    let request_bodies = Arc::new(Mutex::new(Vec::new()));
    let connector = RecordingPostSendFailure {
        expected_body_fragment,
        request_bodies: Arc::clone(&request_bodies),
    };
    let selected = connector.clone();
    let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
    let base = MockAws::builder().build().sdk_config(REGION).await;
    let config = base.into_builder().http_client(http_client).build();
    (provider_from_config(config), request_bodies)
}

async fn provider_without_services() -> AwsProvider {
    let mock = MockAws::builder().build();
    let sdk_config = mock.sdk_config(REGION).await;
    provider_from_config(sdk_config)
}

async fn provider_with_tagged_vpcs(identifiers: &[&str]) -> AwsProvider {
    let describe_calls = Arc::new(AtomicUsize::new(0));
    let mock = MockAws::builder()
        .with_service(TaggedVpcEc2Service {
            identifiers: identifiers
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            discovery_error: false,
            repeat_next_token: false,
            describe_calls,
        })
        .build();
    provider_from_config(mock.sdk_config(REGION).await)
}

async fn provider_with_tagged_vpc_discovery_error() -> AwsProvider {
    let describe_calls = Arc::new(AtomicUsize::new(0));
    let mock = MockAws::builder()
        .with_service(TaggedVpcEc2Service {
            identifiers: Vec::new(),
            discovery_error: true,
            repeat_next_token: false,
            describe_calls,
        })
        .build();
    provider_from_config(mock.sdk_config(REGION).await)
}

async fn provider_with_unbounded_tagged_vpc_pages() -> (AwsProvider, Arc<AtomicUsize>) {
    let describe_calls = Arc::new(AtomicUsize::new(0));
    let mock = MockAws::builder()
        .with_service(TaggedVpcEc2Service {
            identifiers: Vec::new(),
            discovery_error: false,
            repeat_next_token: true,
            describe_calls: Arc::clone(&describe_calls),
        })
        .build();
    (
        provider_from_config(mock.sdk_config(REGION).await),
        describe_calls,
    )
}

async fn provider_with_post_send_failure(
    expected_action: &'static str,
) -> (AwsProvider, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let connector = PostSendVpcFailure {
        calls: Arc::clone(&calls),
        expected_action,
    };
    let selected = connector.clone();
    let http_client = http_client_fn(move |_, _| SharedHttpConnector::new(selected.clone()));
    let base = MockAws::builder().build().sdk_config(REGION).await;
    let config = base.into_builder().http_client(http_client).build();
    (provider_from_config(config), calls)
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn nat_gateway_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "pending", None);
    resource.set_attr("subnet_id", string(SUBNET_ID));
    resource.set_attr(
        "tags",
        Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "Name".to_string(),
            string("pending-operation-test"),
        )]))),
    );
    resource
}

fn flow_log_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", "ec2.FlowLog", "partial", None);
    resource.set_attr(
        "resource_ids",
        Value::Concrete(ConcreteValue::List(vec![
            string(FLOW_LOG_RESOURCE_ID),
            string(REJECTED_FLOW_LOG_RESOURCE_ID),
        ])),
    );
    resource.set_attr("resource_type", string("VPC"));
    resource.set_attr("traffic_type", string("ALL"));
    resource
}

fn acm_certificate_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", "acm.Certificate", "retry", None);
    resource.set_attr("domain_name", string("example.com"));
    resource.set_attr("validation_method", string("EMAIL"));
    resource
}

fn vpc_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", "ec2.Vpc", "outcome-unknown", None);
    resource.set_attr("cidr_block", string("10.0.0.0/16"));
    resource
}

fn route_table_resource() -> Resource {
    let mut resource = Resource::with_provider("aws", "ec2.RouteTable", "outcome-unknown", None);
    resource.set_attr("vpc_id", string("vpc-0123456789abcdef0"));
    resource
}

const TAGGED_VPC_CREATE_TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn tagged_vpc_token(attempts_remaining: u8, next_delay_secs: u32) -> AwsCreateToken {
    serde_json::from_value(serde_json::json!({
        "phase": "tagged_ec2",
        "kind": "vpc",
        "create_token": TAGGED_VPC_CREATE_TOKEN,
        "attempts_remaining": attempts_remaining,
        "next_delay_secs": next_delay_secs
    }))
    .expect("valid tagged VPC token")
}

fn tagged_vpc_token_with_original_failure() -> AwsCreateToken {
    serde_json::from_value(serde_json::json!({
        "phase": "tagged_ec2",
        "kind": "vpc",
        "create_token": TAGGED_VPC_CREATE_TOKEN,
        "attempts_remaining": 1,
        "next_delay_secs": 60,
        "original_failure": {
            "operation": "ec2.CreateVpc",
            "status": 503,
            "code": "InternalError",
            "request_id": "original-create-request-id"
        }
    }))
    .expect("valid tagged VPC token with an original failure")
}

fn tagged_flow_log_token(attempts_remaining: u8, next_delay_secs: u32) -> AwsCreateToken {
    serde_json::from_value(serde_json::json!({
        "phase": "tagged_ec2",
        "kind": "flow_log",
        "create_token": TAGGED_VPC_CREATE_TOKEN,
        "attempts_remaining": attempts_remaining,
        "next_delay_secs": next_delay_secs
    }))
    .expect("valid tagged flow-log token")
}

fn acm_request_token(first_used_at_unix_secs: u64, attempts_remaining: u8) -> AwsCreateToken {
    serde_json::from_value(serde_json::json!({
        "phase": "acm_request_certificate",
        "client_token": ACM_CLIENT_TOKEN,
        "first_used_at_unix_secs": first_used_at_unix_secs,
        "attempts_remaining": attempts_remaining
    }))
    .expect("valid ACM request token")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock should be after Unix epoch")
        .as_secs()
}

fn schema_registry() -> carina_core::schema::SchemaRegistry {
    carina_provider_aws::schemas::schema_registry()
}

async fn normalized_create_for_test(
    resource: &Resource,
) -> carina_core::executor::normalized::NormalizedResource {
    carina_core::executor::normalized::apply_desired_normalization(
        resource.clone(),
        &[],
        &AwsNormalizer,
        &schema_registry(),
    )
    .await
    .expect("test create input should pass desired normalization")
}

async fn create_request_for_test(resource: Resource) -> CreateRequest {
    carina_core::validation::deferred_types::with_session_for_test_async(
        &carina_core::parser::LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            let bindings = carina_core::binding_index::ResolvedBindings::default();
            let module_gate =
                carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
            let schemas = schema_registry();
            let preparation = carina_core::executor::ProviderPreparationContext::new(
                &bindings,
                &module_gate,
                &[],
                &AwsNormalizer,
                &[],
                &schemas,
                type_checks,
            );
            carina_core::executor::prepare_create_request(resource, &preparation)
                .await
                .expect("test NAT gateway should pass checked create preparation")
        },
    )
    .await
}

async fn create_state_for_test(
    provider: &AwsProvider,
    id: &ResourceId,
    request: CreateRequest,
) -> Result<SnapshotState, CreateFailure> {
    let accepted = carina_core::executor::AcceptedRequest::from_create_request(&request);
    let (create, _cancellation) = carina_core::executor::dispatch_create(provider, id, request);
    create
        .await
        .map(|outcome| carina_core::executor::complete_create_for_writeback(outcome, accepted).0)
}

fn expect_create_pending(step: TestCreateStep) -> AwsCreateToken {
    match step {
        TestCreateStep::Pending {
            token, identifier, ..
        } => {
            assert_eq!(
                token,
                AwsCreateToken::Operation(AwsCreateOperation::NatGateway {
                    nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
                })
            );
            assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
            token
        }
        _ => panic!("expected pending create"),
    }
}

fn expect_delete_pending(step: OperationStep<(), ProviderError, AwsDeleteToken>) -> AwsDeleteToken {
    match step {
        OperationStep::Pending {
            pending,
            poll_after_secs,
            phase,
        } => {
            assert_eq!(poll_after_secs, 5);
            assert_eq!(phase, carina_provider_protocol::PendingPhase::Operation);
            assert_eq!(
                pending,
                AwsDeleteToken::NatGateway {
                    nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
                }
            );
            pending
        }
        OperationStep::Done(()) => panic!("expected pending delete"),
        OperationStep::Verdict(_) => panic!("expected pending delete, got a verdict"),
    }
}

fn poll_attempt(
    resource: &Resource,
    token: AwsCreateToken,
) -> AwsCreatePoll<TestPollAttempt, TestIssuedCreate> {
    AwsCreatePoll::Unissued(
        TestPollAttempt {
            id: resource.id.clone(),
        },
        (token).into_operation().unwrap(),
    )
}

fn issued_poll(
    resource: &Resource,
    sent: Sent,
    token: AwsCreateToken,
) -> AwsCreatePoll<TestPollAttempt, TestIssuedCreate> {
    AwsCreatePoll::Issued(
        TestIssuedCreate {
            id: resource.id.clone(),
            identifier: applied_identifier(NAT_GATEWAY_ID),
            sent,
        },
        token,
    )
}

fn issued_resume(token: AwsCreateToken) -> AwsCreateResume<TestResumeAttempt, TestResumedCreate> {
    AwsCreateResume::Issued(
        TestResumedCreate {
            identifier: applied_identifier(NAT_GATEWAY_ID),
        },
        token,
    )
}

fn applied_identifier(value: &str) -> AppliedIdentifier {
    AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
}

trait TestStateIdentifier {
    fn test_identifier(&self) -> &AppliedIdentifier;
}

impl TestStateIdentifier for AppliedState {
    fn test_identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }
}

impl TestStateIdentifier for TestAppliedState {
    fn test_identifier(&self) -> &AppliedIdentifier {
        self.identifier()
    }
}

fn assert_nat_gateway_state_identifier(state: &impl TestStateIdentifier) {
    assert_eq!(state.test_identifier().as_str(), NAT_GATEWAY_ID);
}

#[tokio::test]
async fn rejected_create_is_not_created() {
    let (provider, service) = provider_with_ec2_failures(&["pending"], true, false).await;
    let resource = nat_gateway_resource();

    let verdict = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect_err("the scripted CreateNatGateway rejection must fail");

    assert!(matches!(verdict, TestCreateVerdict::NotCreated(_)));
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 0);
}

#[tokio::test]
async fn create_vpc_post_send_failure_becomes_unissued_pending() {
    let (provider, calls) = provider_with_post_send_failure("Action=CreateVpc").await;
    let resource = vpc_resource();

    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect("an ambiguous create must be persisted for discovery");

    let TestCreateStep::Pending {
        token, identifier, ..
    } = step
    else {
        panic!("expected unissued pending create");
    };
    assert_eq!(identifier, None);
    assert!(matches!(
        token,
        AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2 {
            kind: Ec2TaggedResourceKind::Vpc,
            attempts_remaining: 4,
            next_delay_secs: 5,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn token_bearing_route_table_post_send_failure_enters_tagged_discovery() {
    let (provider, calls) = provider_with_post_send_failure("Action=CreateRouteTable").await;
    let resource = route_table_resource();

    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect("a maybe-sent route-table create must stay pending");

    assert!(matches!(
        step,
        TestCreateStep::Pending {
            token: AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2 {
                kind: Ec2TaggedResourceKind::RouteTable,
                attempts_remaining: 4,
                next_delay_secs: 5,
                ..
            }),
            identifier: None,
            poll_after_secs: 5,
        }
    ));
    assert_eq!(
        calls.load(Ordering::Relaxed),
        3,
        "the SDK may safely retry a token-bearing create before discovery"
    );
}

#[tokio::test]
async fn maybe_sent_request_certificate_becomes_secret_free_pending_with_same_token() {
    let (provider, request_bodies) = provider_with_recording_post_send_failure("example.com").await;
    let resource = acm_certificate_resource();
    let before = unix_now();

    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect("a maybe-sent certificate request must remain pending");
    let after = unix_now();
    let TestCreateStep::Pending {
        token,
        identifier: None,
        poll_after_secs: 5,
    } = step
    else {
        panic!("expected an unissued ACM pending step");
    };
    let token_json = serde_json::to_value(&token).expect("ACM token should serialize");
    assert_eq!(token_json["phase"], "acm_request_certificate");
    assert_eq!(token_json["attempts_remaining"], 3);
    assert!(
        token_json["first_used_at_unix_secs"]
            .as_u64()
            .is_some_and(|timestamp| (before..=after).contains(&timestamp))
    );
    assert!(
        !token_json.to_string().contains("example.com"),
        "the pending token must not copy desired resource data"
    );

    let client_token = token_json["client_token"]
        .as_str()
        .expect("ACM pending token should carry a client token");
    let request_bodies = request_bodies.lock().expect("recorded request-body lock");
    assert_eq!(request_bodies.len(), 3, "the SDK uses its bounded retries");
    for body in request_bodies.iter() {
        let request: serde_json::Value =
            serde_json::from_str(body).expect("RequestCertificate body should be JSON");
        assert_eq!(request["IdempotencyToken"], client_token);
    }
}

#[tokio::test]
async fn request_certificate_poll_success_issues_arn_and_runs_normal_continuation() {
    let (provider, request_bodies) = provider_with_acm(AcmRequestResult::Success).await;
    let resource = acm_certificate_resource();

    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, acm_request_token(unix_now(), 3)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("resending the same ACM token should recover the ARN");

    let TestCreateStep::Success(state) = step else {
        panic!("successful ACM resend should complete normal create continuation");
    };
    assert_eq!(state.identifier().as_str(), ACM_CERTIFICATE_ARN);
    let request_bodies = request_bodies.lock().expect("ACM request-body lock");
    assert_eq!(request_bodies.len(), 1);
    let request: serde_json::Value =
        serde_json::from_str(&request_bodies[0]).expect("RequestCertificate body should be JSON");
    assert_eq!(request["DomainName"], "example.com");
    assert_eq!(request["IdempotencyToken"], ACM_CLIENT_TOKEN);
}

#[tokio::test]
async fn maybe_sent_request_certificate_poll_repends_with_a_bounded_attempt() {
    let (provider, request_bodies) = provider_with_recording_post_send_failure("example.com").await;
    let resource = acm_certificate_resource();
    let first_used_at_unix_secs = unix_now();

    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, acm_request_token(first_used_at_unix_secs, 2)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("a maybe-sent resend inside the poll budget must remain pending");

    let TestCreateStep::Pending {
        token,
        identifier: None,
        poll_after_secs: 5,
    } = step
    else {
        panic!("expected a repended ACM request");
    };
    let token_json = serde_json::to_value(token).expect("ACM token should serialize");
    assert_eq!(token_json["client_token"], ACM_CLIENT_TOKEN);
    assert_eq!(
        token_json["first_used_at_unix_secs"],
        first_used_at_unix_secs
    );
    assert_eq!(token_json["attempts_remaining"], 1);
    assert_eq!(
        request_bodies
            .lock()
            .expect("recorded request-body lock")
            .len(),
        3
    );
}

#[tokio::test]
async fn maybe_sent_request_certificate_poll_exhaustion_stays_in_flight() {
    let (provider, _request_bodies) =
        provider_with_recording_post_send_failure("example.com").await;
    let resource = acm_certificate_resource();

    let token = acm_request_token(unix_now(), 1);
    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token.clone()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("the bounded ACM resend budget must stay in flight");

    assert!(matches!(verdict, TestCreateVerdict::InFlight { .. }));
    assert_eq!(verdict.in_flight(), (&token, None));
    assert_eq!(
        verdict.provider_error().to_wire().message,
        "acm.Certificate may have been created; check before re-applying"
    );
}

#[tokio::test]
async fn expired_request_certificate_poll_stays_in_flight_without_resending() {
    let (provider, request_bodies) = provider_with_recording_post_send_failure("example.com").await;
    let resource = acm_certificate_resource();

    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(
                &resource,
                acm_request_token(unix_now().saturating_sub(45 * 60), 3),
            ),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("an expired ACM token must not be resent by poll");

    match &verdict {
        TestCreateVerdict::InFlight { error, .. } => assert_eq!(
            error.to_wire().message,
            "acm.Certificate may have been created; check before re-applying"
        ),
        TestCreateVerdict::NotCreated(_) => {
            panic!("expired poll returned NotCreated")
        }
        TestCreateVerdict::IdentifierReported(_) => {
            panic!("expired poll returned IdentifierReported")
        }
        TestCreateVerdict::NoResource(_) => {
            panic!("expired poll returned NoResource")
        }
    }
    assert!(
        request_bodies
            .lock()
            .expect("recorded request-body lock")
            .is_empty(),
        "expiry must be checked before poll rebuilds or sends the request"
    );
}

#[tokio::test]
async fn request_certificate_token_with_issued_poll_fails_without_resending() {
    let (provider, request_bodies) = provider_with_acm(AcmRequestResult::Success).await;
    let resource = acm_certificate_resource();

    let verdict = provider
        .poll_create_step(
            &resource,
            issued_poll(
                &resource,
                Sent::AllRequestAttributes,
                acm_request_token(unix_now(), 3),
            ),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("an ACM request token cannot have an issued poll handle");

    match &verdict {
        TestCreateVerdict::IdentifierReported(reported) => assert_eq!(
            reported.error.to_wire().message,
            "ACM request-certificate token cannot be paired with an issued create handle"
        ),
        TestCreateVerdict::NotCreated(_) => {
            panic!("impossible issued poll returned NotCreated")
        }
        TestCreateVerdict::InFlight { .. } => {
            panic!("impossible issued poll stayed in flight")
        }
        TestCreateVerdict::NoResource(_) => {
            panic!("impossible issued poll returned NoResource")
        }
    }
    assert!(
        request_bodies
            .lock()
            .expect("ACM request-body lock")
            .is_empty(),
        "an impossible issued poll pairing must fail before resending"
    );
}

#[tokio::test]
async fn rejected_request_certificate_poll_preserves_the_continuation() {
    let (provider, request_bodies) = provider_with_acm(AcmRequestResult::Rejected).await;
    let resource = acm_certificate_resource();
    let token = acm_request_token(unix_now(), 3);

    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token.clone()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("an explicit ACM rejection must preserve the continuation");

    assert_eq!(verdict.in_flight(), (&token, None));
    assert_eq!(
        verdict.provider_error().to_wire().message,
        "acm.Certificate may have been created; check before re-applying"
    );
    assert_eq!(
        request_bodies.lock().expect("ACM request-body lock").len(),
        1
    );
}

#[tokio::test]
async fn request_certificate_resume_within_window_resends_the_same_token() {
    let (provider, request_bodies) = provider_with_acm(AcmRequestResult::Success).await;
    let resource = acm_certificate_resource();

    let step = provider
        .resume_create_step(
            &resource,
            AwsCreateResume::Unissued(
                TestResumeAttempt,
                (acm_request_token(unix_now(), 3)).into_operation().unwrap(),
            ),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("an in-window resume should recover the certificate ARN");

    let TestResumeStep::Completed(state) = step else {
        panic!("successful ACM resume should complete the create");
    };
    assert_eq!(state.identifier().as_str(), ACM_CERTIFICATE_ARN);
    let request_bodies = request_bodies.lock().expect("ACM request-body lock");
    let request: serde_json::Value =
        serde_json::from_str(&request_bodies[0]).expect("RequestCertificate body should be JSON");
    assert_eq!(request["IdempotencyToken"], ACM_CLIENT_TOKEN);
}

#[tokio::test]
async fn expired_request_certificate_resume_stays_in_flight_without_resending() {
    let (provider, request_bodies) = provider_with_recording_post_send_failure("example.com").await;
    let resource = acm_certificate_resource();

    let token = acm_request_token(unix_now().saturating_sub(3_600), 3);
    let verdict = provider
        .resume_create_step(
            &resource,
            AwsCreateResume::Unissued(TestResumeAttempt, (token.clone()).into_operation().unwrap()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("an expired ACM token must not be resent");

    assert!(matches!(verdict, TestResumeVerdict::InFlight { .. }));
    assert_eq!(verdict.in_flight(), (&token, None));
    assert_eq!(
        verdict.provider_error().to_wire().message,
        "acm.Certificate may have been created; check before re-applying"
    );
    assert!(
        request_bodies
            .lock()
            .expect("recorded request-body lock")
            .is_empty(),
        "expiry must be checked before rebuilding or sending the request"
    );
}

#[tokio::test]
async fn request_certificate_token_with_issued_resume_fails_without_resending() {
    let (provider, request_bodies) = provider_with_acm(AcmRequestResult::Success).await;
    let resource = acm_certificate_resource();

    let verdict = provider
        .resume_create_step(
            &resource,
            issued_resume(acm_request_token(unix_now(), 3)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("an ACM request token cannot have an issued resume handle");

    match &verdict {
        TestResumeVerdict::IdentifierReported(error) => assert_eq!(
            error.to_wire().message,
            "ACM request-certificate token cannot be paired with an issued create handle"
        ),
        TestResumeVerdict::NotCreated(_) => {
            panic!("impossible issued resume returned NotCreated")
        }
        TestResumeVerdict::InFlight { .. } => {
            panic!("impossible issued resume stayed in flight")
        }
        TestResumeVerdict::NoResource(_) => {
            panic!("impossible issued resume returned NoResource")
        }
    }
    assert!(
        request_bodies
            .lock()
            .expect("ACM request-body lock")
            .is_empty(),
        "an impossible issued resume pairing must fail before resending"
    );
}

#[tokio::test]
async fn tagged_vpc_poll_with_one_match_hands_continuation_to_next_poll() {
    let provider = provider_with_tagged_vpcs(&["vpc-00000553"]).await;
    let resource = vpc_resource();

    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_vpc_token(4, 5)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("one discovered VPC must become an issued continuation");

    let TestCreateStep::Pending {
        token,
        identifier: Some(identifier),
        poll_after_secs: 5,
    } = step
    else {
        panic!("one discovered VPC must hand continuation to a later poll");
    };
    assert_eq!(identifier.as_str(), "vpc-00000553");
    assert!(matches!(
        &token,
        AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2Found {
            kind: Ec2TaggedResourceKind::Vpc,
            identifier: found,
        }) if found == &identifier
    ));

    let step = provider
        .poll_create_step::<TestPollAttempt>(
            &resource,
            AwsCreatePoll::Issued(
                TestIssuedCreate {
                    id: resource.id.clone(),
                    identifier,
                    sent: Sent::AllRequestAttributes,
                },
                token,
            ),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("the later poll must run the normal continuation");

    let TestCreateStep::Success(state) = step else {
        panic!("the continuation poll must complete the VPC, got {step:?}");
    };
    assert_eq!(state.identifier().as_str(), "vpc-00000553");
}

#[tokio::test]
async fn tagged_vpc_poll_with_no_match_uses_full_window_then_stays_in_flight() {
    let provider = provider_with_tagged_vpcs(&[]).await;
    let resource = vpc_resource();

    let mut token = tagged_vpc_token(4, 5);
    for (remaining, expected_delay) in [(3, 10), (2, 20), (1, 30)] {
        let step = provider
            .poll_create_step(
                &resource,
                poll_attempt(&resource, token),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .expect("the discovery window must remain pending");
        let TestCreateStep::Pending {
            token: next,
            identifier,
            poll_after_secs,
        } = step
        else {
            panic!("an empty discovery result inside the window must re-pend");
        };
        assert_eq!(identifier, None);
        assert_eq!(poll_after_secs, expected_delay);
        assert!(matches!(
            next,
            AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2 {
                attempts_remaining,
                next_delay_secs,
                ..
            }) if attempts_remaining == remaining && next_delay_secs == expected_delay
        ));
        token = next;
    }

    let expected_token = token.clone();
    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("exhaustion cannot prove the create was not applied");
    assert!(matches!(verdict, TestCreateVerdict::InFlight { .. }));
    assert_eq!(verdict.in_flight(), (&expected_token, None));
    assert_eq!(
        verdict.provider_error().to_wire().message,
        format!(
            "ec2.Vpc may have been created with tag carina:create-token={TAGGED_VPC_CREATE_TOKEN}; check before re-applying"
        )
    );
}

#[tokio::test]
async fn tagged_vpc_discovery_stops_after_the_bounded_page_count() {
    let (provider, describe_calls) = provider_with_unbounded_tagged_vpc_pages().await;
    let resource = vpc_resource();

    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_vpc_token(4, 5)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("a page-bound error inside the discovery window must re-pend");

    assert_eq!(describe_calls.load(Ordering::Relaxed), 2);
    assert!(matches!(
        step,
        TestCreateStep::Pending {
            token: AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2 {
                attempts_remaining: 3,
                next_delay_secs: 10,
                ..
            }),
            identifier: None,
            poll_after_secs: 10,
        }
    ));
}

#[tokio::test]
async fn tagged_vpc_resume_with_no_match_at_exhaustion_stays_in_flight() {
    let provider = provider_with_tagged_vpcs(&[]).await;
    let resource = vpc_resource();

    let verdict = provider
        .resume_create_step(
            &resource,
            AwsCreateResume::Unissued(
                TestResumeAttempt,
                (tagged_vpc_token(1, 60)).into_operation().unwrap(),
            ),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("resume exhaustion must preserve the in-flight create");

    assert!(matches!(verdict, TestResumeVerdict::InFlight { .. }));
    assert_eq!(
        verdict.provider_error().to_wire().message,
        format!(
            "ec2.Vpc may have been created with tag carina:create-token={TAGGED_VPC_CREATE_TOKEN}; check before re-applying"
        )
    );
}

#[tokio::test]
async fn tagged_vpc_discovery_error_consumes_final_attempt_and_keeps_cause() {
    let provider = provider_with_tagged_vpc_discovery_error().await;
    let resource = vpc_resource();

    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_vpc_token(1, 60)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("a final discovery error must not re-pend forever");

    assert!(matches!(verdict, TestCreateVerdict::InFlight { .. }));
    let wire = verdict.provider_error().to_wire();
    assert_eq!(
        wire.message,
        format!(
            "ec2.Vpc may have been created with tag carina:create-token={TAGGED_VPC_CREATE_TOKEN}; check before re-applying"
        )
    );
    assert_eq!(
        wire.resource_id
            .as_ref()
            .map(|resource_id| resource_id.identity.as_str()),
        Some("outcome-unknown")
    );
    let rendered = verdict.provider_error().to_string();
    assert!(
        rendered.contains("UnauthorizedOperation"),
        "rendered discovery error should include the AWS error code"
    );
    assert!(
        rendered.contains("scripted discovery denial"),
        "rendered discovery error should include the AWS error message"
    );
}

#[tokio::test]
async fn tagged_vpc_exhaustion_keeps_the_original_send_failure_summary() {
    let provider = provider_with_tagged_vpcs(&[]).await;
    let resource = vpc_resource();

    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_vpc_token_with_original_failure()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("final discovery must retain the ambiguous send metadata");

    assert!(matches!(verdict, TestCreateVerdict::InFlight { .. }));
    let rendered = verdict.provider_error().to_string();
    assert!(
        rendered.contains("ec2.CreateVpc"),
        "rendered create error should include the operation name"
    );
    assert!(
        rendered.contains("status=503"),
        "rendered create error should include the status code"
    );
    assert!(
        rendered.contains("code=InternalError"),
        "rendered create error should include the AWS error code"
    );
    assert!(
        rendered.contains("request_id=original-create-request-id"),
        "rendered create error should include the request ID"
    );
}

#[tokio::test]
async fn partial_flow_log_discovery_keeps_polling_for_every_requested_resource_id() {
    let provider = provider_with_tagged_flow_logs(&[FLOW_LOG_ID]).await;
    let resource = flow_log_resource();

    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_flow_log_token(4, 5)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("one of two requested flow logs is still a partial discovery");

    assert!(matches!(
        step,
        TestCreateStep::Pending {
            token: AwsCreateToken::Operation(AwsCreateOperation::TaggedEc2 {
                kind: Ec2TaggedResourceKind::FlowLog,
                attempts_remaining: 3,
                next_delay_secs: 10,
                ..
            }),
            identifier: None,
            poll_after_secs: 10,
        }
    ));
}

#[tokio::test]
async fn tagged_vpc_poll_with_two_matches_stays_in_flight() {
    let provider = provider_with_tagged_vpcs(&["vpc-0553-a", "vpc-0553-b"]).await;
    let resource = vpc_resource();

    let verdict = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, tagged_vpc_token(4, 5)),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("multiple matches cannot choose an identifier safely");

    assert!(matches!(verdict, TestCreateVerdict::InFlight { .. }));
    let rendered = verdict.provider_error().to_string();
    assert!(
        rendered.contains("vpc-0553-a"),
        "rendered discovery error should include the first matching identifier"
    );
    assert!(
        rendered.contains("vpc-0553-b"),
        "rendered discovery error should include the second matching identifier"
    );
}

#[tokio::test]
async fn tagged_vpc_issued_poll_with_two_matches_preserves_identifier_and_token() {
    let provider = provider_with_tagged_vpcs(&["vpc-0553-a", "vpc-0553-b"]).await;
    let resource = vpc_resource();
    let token = tagged_vpc_token(4, 5);
    let identifier = applied_identifier("vpc-previously-issued");
    let poll: AwsCreatePoll<TestPollAttempt, TestIssuedCreate> = AwsCreatePoll::Issued(
        TestIssuedCreate {
            id: resource.id.clone(),
            identifier: identifier.clone(),
            sent: Sent::AllRequestAttributes,
        },
        token.clone(),
    );

    let verdict = provider
        .poll_create_step(
            &resource,
            poll,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("ambiguous discovery must preserve an issued continuation");

    assert_eq!(verdict.in_flight(), (&token, Some(&identifier)));
}

#[tokio::test]
async fn accepted_create_then_transient_waiter_failure_stays_in_flight_with_identifier() {
    let (provider, _service) = provider_with_ec2(&["read_error"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let step = provider
        .poll_create_step(
            &resource,
            issued_poll(&resource, Sent::only(["subnet_id"]), token.clone()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("a transient waiter error must re-pend");

    let TestCreateStep::Pending {
        token: repended,
        identifier,
        ..
    } = step
    else {
        panic!("expected the transient waiter failure to stay pending");
    };
    assert_eq!(repended, token);
    assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
}

#[tokio::test]
async fn post_create_failure_reports_identifier_and_excludes_failed_step_from_sent() {
    let (provider, service) =
        provider_with_ec2_failures(&["available", "available"], false, true).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let verdict = provider
        .poll_create_step(
            &resource,
            issued_poll(&resource, Sent::only(["subnet_id"]), token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("the scripted CreateTags rejection must fail after issue");

    let TestCreateVerdict::IdentifierReported(reported) = verdict else {
        panic!("a post-create failure must report the identifier");
    };
    let TestIdentifierReportedVerdict {
        identifier,
        observation,
        sent,
        error: _,
    } = *reported;
    assert_eq!(identifier.as_str(), NAT_GATEWAY_ID);
    assert!(matches!(
        observation,
        Observation::Read(ReadOutcome::Found { .. })
    ));
    assert_eq!(sent, Sent::only(["subnet_id"]));
    let Sent::Only(names) = sent else {
        panic!("NAT gateway base create must report an explicit sent subset");
    };
    assert!(!names.contains("tags"));
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn partial_flow_log_create_excludes_resource_ids_from_sent_when_read_back_is_absent() {
    let provider = provider_with_partial_flow_log().await;
    let resource = flow_log_resource();

    let verdict = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .expect_err("one rejected resource ID must retain the reported identifier");

    let TestCreateVerdict::IdentifierReported(reported) = verdict else {
        panic!("partial CreateFlowLogs must retain the IDs that AWS did create");
    };
    assert_eq!(reported.identifier.as_str(), FLOW_LOG_ID);
    assert!(matches!(
        reported.observation,
        Observation::Read(ReadOutcome::NotFound { .. })
    ));
    assert_eq!(
        reported.sent,
        Sent::only(["resource_type", "traffic_type"]),
        "the rejected resource_ids value must not be recorded as applied"
    );
}

#[tokio::test]
async fn post_create_failure_reports_identifier_with_read_failed_observation() {
    let (provider, _service) =
        provider_with_ec2_failures(&["available", "read_error"], false, true).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let verdict = provider
        .poll_create_step(
            &resource,
            issued_poll(&resource, Sent::only(["subnet_id"]), token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("the scripted CreateTags rejection must fail after issue");

    let TestCreateVerdict::IdentifierReported(reported) = verdict else {
        panic!("a post-create failure must report the identifier");
    };
    assert!(matches!(reported.observation, Observation::ReadFailed(_)));
}

#[tokio::test]
async fn resume_of_issued_create_still_creating_repends_with_same_identifier() {
    let (provider, _service) = provider_with_ec2(&["pending"]).await;
    let resource = nat_gateway_resource();
    let token = AwsCreateToken::Operation(AwsCreateOperation::NatGateway {
        nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
    });

    let step = provider
        .resume_create_step(
            &resource,
            issued_resume(token.clone()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("a still-creating NAT gateway must re-pend");
    let TestResumeStep::Pending {
        token: repended,
        identifier,
        poll_after_secs,
    } = step
    else {
        panic!("expected resumed NAT gateway to remain pending");
    };
    assert_eq!(repended, token);
    assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
    assert_eq!(poll_after_secs, 5);
}

#[tokio::test]
async fn resume_of_issued_create_found_completes() {
    let (provider, service) = provider_with_ec2(&["available", "available"]).await;
    let resource = nat_gateway_resource();
    let token = AwsCreateToken::Operation(AwsCreateOperation::NatGateway {
        nat_gateway_id: applied_identifier(NAT_GATEWAY_ID),
    });

    let step = provider
        .resume_create_step(
            &resource,
            issued_resume(token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("an available resumed NAT gateway must complete");
    let TestResumeStep::Completed(state) = step else {
        panic!("expected resumed NAT gateway completion");
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn resume_transient_error_without_identifier_stays_in_flight() {
    let provider = provider_without_services().await;
    let resource = Resource::with_provider("aws", "organizations.Account", "resume", None);
    let token = AwsCreateToken::Operation(AwsCreateOperation::OrganizationsAccount {
        create_account_request_id: "car-test-request".to_string(),
    });
    let expected_token = token.clone();
    let resume = AwsCreateResume::Unissued(TestResumeAttempt, token.into_operation().unwrap());
    let verdict = provider
        .resume_create_step(
            &resource,
            resume,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("a transient unissued resume error must stay in flight");
    assert!(matches!(verdict, TestResumeVerdict::InFlight { .. }));
    assert_eq!(verdict.in_flight(), (&expected_token, None));
    assert!(!verdict.provider_error().to_wire().message.is_empty());
}

#[tokio::test]
async fn nat_gateway_create_steps_issue_once_poll_once_and_read_back() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();

    let mut token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 0);
    assert_eq!(service.describe_nat_gateways_calls(), 0);

    token = expect_create_pending(
        provider
            .poll_create_step(
                &resource,
                poll_attempt(&resource, token),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .expect("first NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 1);

    token = expect_create_pending(
        provider
            .poll_create_step(
                &resource,
                poll_attempt(&resource, token),
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .expect("second NAT gateway create poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 2);

    let state = match provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("available NAT gateway create poll")
    {
        TestCreateStep::Success(state) => state,
        _ => panic!("expected completed NAT gateway create"),
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(
        service.describe_nat_gateways_calls(),
        4,
        "the available poll must perform one describe and one read-back describe"
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
}

#[tokio::test]
async fn create_read_back_not_found_then_complete() {
    assert_create_read_back_recovers("not_found").await;
}

#[tokio::test]
async fn create_read_back_permanent_error_finishes_partial_after_mutation_poll() {
    let (provider, service) = provider_with_ec2(&["available", "read_error"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap(),
    );
    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .unwrap();
    let TestCreateStep::Partial {
        state,
        missing_attributes,
        reason,
    } = step
    else {
        panic!("permanent read error must immediately finish partial: {step:?}")
    };
    assert_eq!(state.identifier().as_str(), NAT_GATEWAY_ID);
    assert_eq!(state._attributes, resource.resolved_attributes());
    let schema =
        carina_provider_aws::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config().schema;
    let mut expected = schema.attributes().keys().cloned().collect::<Vec<_>>();
    expected.sort();
    assert_eq!(missing_attributes, expected);
    assert!(
        reason.contains("Failed to describe NAT gateways"),
        "{reason}"
    );
    assert_eq!(service.describe_nat_gateways_calls(), 2);
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
}

async fn assert_create_read_back_recovers(first_read: &str) {
    let (provider, service) = provider_with_ec2(&["available", first_read, "available"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("issue create"),
    );
    let pending = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("read-back can recover");
    let TestCreateStep::Pending {
        token, identifier, ..
    } = pending
    else {
        panic!("an incomplete read-back must remain pending: {pending:?}");
    };
    assert_eq!(identifier, Some(applied_identifier(NAT_GATEWAY_ID)));
    assert_eq!(serde_json::to_value(&token).unwrap()["phase"], "read_back");
    let result = provider
        .poll_create_step(
            &resource,
            issued_poll(&resource, Sent::AllRequestAttributes, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("read by identifier");
    let TestCreateStep::Success(state) = result else {
        panic!("expected complete read-back: {result:?}")
    };
    assert_nat_gateway_state_identifier(&state);
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(
        service.create_tags_calls(),
        1,
        "read-back never replays follow-up mutations"
    );
    assert_eq!(service.describe_nat_gateways_calls(), 3);
}

#[tokio::test]
async fn resume_read_back_in_fresh_provider_reads_by_identifier() {
    let service = Arc::new(ScriptedEc2Service::new(&[
        "available",
        "not_found",
        "available",
    ]));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let config = mock.sdk_config(REGION).await;
    let provider = provider_from_config(config.clone());
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap(),
    );
    let step = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .unwrap();
    let TestCreateStep::Pending { token, .. } = step else {
        panic!("expected read-back continuation: {step:?}")
    };
    let serialized = serde_json::to_string(&token).unwrap();
    drop(provider);
    let mut resource = resource;
    resource.attributes.clear();
    let provider = provider_from_config(config);
    let step = provider
        .resume_create_step(
            &resource,
            issued_resume(serde_json::from_str(&serialized).unwrap()),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect("fresh provider can resume using only the identifier");
    let TestResumeStep::Completed(state) = step else {
        panic!("expected complete resume: {step:?}")
    };
    assert_eq!(state.identifier().as_str(), NAT_GATEWAY_ID);
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 3);
}

#[tokio::test]
async fn nat_gateway_failed_create_poll_returns_api_error() {
    let (provider, service) = provider_with_ec2(&["failed"]).await;
    let resource = nat_gateway_resource();
    let token = expect_create_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .expect("start NAT gateway create"),
    );

    let error = provider
        .poll_create_step(
            &resource,
            poll_attempt(&resource, token),
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .expect_err("failed NAT gateway should fail create polling");
    assert_eq!(error.provider_error().variant_name(), "api_error");
    assert_eq!(
        error.provider_error().to_wire().message,
        "NAT gateway creation failed"
    );
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 1);
}

#[tokio::test]
async fn nat_gateway_delete_steps_issue_once_and_poll_to_gone() {
    let (provider, service) = provider_with_ec2(&["deleting", "deleted"]).await;
    let id = ResourceId::with_provider_identity("aws", RESOURCE_TYPE, "pending", None);
    let request = delete_request();

    let mut token = expect_delete_pending(
        provider
            .delete_step(&id, &applied_identifier(NAT_GATEWAY_ID), &request)
            .await
            .expect("start NAT gateway delete"),
    );
    assert_eq!(service.delete_nat_gateway_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 0);

    token = expect_delete_pending(
        provider
            .poll_delete_step(&id, token)
            .await
            .expect("deleting NAT gateway poll"),
    );
    assert_eq!(service.describe_nat_gateways_calls(), 1);

    let deleted = provider
        .poll_delete_step(&id, token)
        .await
        .expect("deleted NAT gateway poll");
    assert!(matches!(deleted, OperationStep::Done(())));
    assert_eq!(service.describe_nat_gateways_calls(), 2);
    assert_eq!(service.delete_nat_gateway_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn native_provider_create_drives_nat_gateway_to_read_back_state() {
    let (provider, service) = provider_with_ec2(&["pending", "pending", "available"]).await;
    let resource = nat_gateway_resource();
    let id = resource.id.clone();
    let request = create_request_for_test(resource).await;

    let state = create_state_for_test(&provider, &id, request)
        .await
        .expect("drive native NAT gateway create");

    assert_eq!(state.identifier.as_deref(), Some(NAT_GATEWAY_ID));
    assert_eq!(service.create_nat_gateway_calls(), 1);
    assert_eq!(service.create_tags_calls(), 1);
    assert_eq!(service.describe_nat_gateways_calls(), 4);
}

#[tokio::test(start_paused = true)]
async fn acm_dvo_arrives_on_second_poll() {
    let request_bodies = Arc::new(Mutex::new(Vec::new()));
    let describe_calls = Arc::new(AtomicUsize::new(0));
    let mock = MockAws::builder()
        .with_service(ScriptedAcmService {
            request_result: AcmRequestResult::Success,
            request_bodies: Arc::clone(&request_bodies),
            describe_dvo: Mutex::new(VecDeque::from([
                AcmDescribeResult::Incomplete,
                AcmDescribeResult::PartialDvo,
                AcmDescribeResult::Populated,
            ])),
            describe_calls: Arc::clone(&describe_calls),
            update_calls: Arc::new(AtomicUsize::new(0)),
        })
        .build();
    let provider = provider_from_config(mock.sdk_config(REGION).await);
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let mut step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap();
    for expected_reads in 1..=2 {
        let TestCreateStep::Pending {
            token, identifier, ..
        } = step
        else {
            panic!("expected DVO read-back pending: {step:?}")
        };
        assert_eq!(identifier, Some(applied_identifier(ACM_CERTIFICATE_ARN)));
        assert_eq!(
            describe_calls.load(Ordering::Relaxed),
            expected_reads,
            "one fetch per step"
        );
        let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
            TestIssuedCreate {
                id: resource.id.clone(),
                identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                sent: Sent::AllRequestAttributes,
            },
            token,
        );
        step = provider
            .poll_create_step(
                &resource,
                poll,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .unwrap();
    }
    let TestCreateStep::Success(state) = step else {
        panic!("expected populated DVO: {step:?}")
    };
    assert!(state._attributes.contains_key("domain_validation_options"));
    assert_eq!(describe_calls.load(Ordering::Relaxed), 3);
    assert_eq!(request_bodies.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn read_back_final_attempt_returns_partial_with_exact_missing_attributes() {
    for final_read_populated in [false, true] {
        let request_bodies = Arc::new(Mutex::new(Vec::new()));
        let describe_calls = Arc::new(AtomicUsize::new(0));
        let mock = MockAws::builder()
            .with_service(ScriptedAcmService {
                request_result: AcmRequestResult::Success,
                request_bodies: Arc::clone(&request_bodies),
                describe_dvo: Mutex::new(VecDeque::from([
                    AcmDescribeResult::Incomplete,
                    if final_read_populated {
                        AcmDescribeResult::Populated
                    } else {
                        AcmDescribeResult::PartialDvo
                    },
                ])),
                describe_calls: Arc::clone(&describe_calls),
                update_calls: Arc::new(AtomicUsize::new(0)),
            })
            .build();
        let provider = provider_from_config(mock.sdk_config(REGION).await);
        let mut resource = acm_certificate_resource();
        resource.set_attr("validation_method", string("DNS"));
        let step = provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap();
        let TestCreateStep::Pending { token, .. } = step else {
            panic!("expected read-back pending: {step:?}")
        };
        let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
            TestIssuedCreate {
                id: resource.id.clone(),
                identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                sent: Sent::AllRequestAttributes,
            },
            token,
        );
        let step = provider
            .poll_create_step(
                &resource,
                poll,
                PollRequest {
                    finality: PollFinality::FinalAttempt,
                },
            )
            .await
            .unwrap();
        if final_read_populated {
            assert!(matches!(step, TestCreateStep::Success(_)));
        } else {
            let TestCreateStep::Partial {
                state,
                reason,
                missing_attributes,
            } = step
            else {
                panic!("final attempt must be terminal: {step:?}")
            };
            assert_eq!(state.identifier().as_str(), ACM_CERTIFICATE_ARN);
            assert_eq!(
                state._attributes.get("certificate_arn"),
                Some(&string(ACM_CERTIFICATE_ARN))
            );
            assert_eq!(
                state._attributes.get("domain_name"),
                Some(&string("example.com"))
            );
            assert_eq!(missing_attributes, ["domain_validation_options"]);
            assert!(reason.contains("domain_validation_options"));
            assert!(!reason.contains("wait"));
            assert!(!state._attributes.contains_key("domain_validation_options"));
            assert!(state._attributes.contains_key("status"));
        }
        assert_eq!(describe_calls.load(Ordering::Relaxed), 2);
        assert_eq!(request_bodies.lock().unwrap().len(), 1);
    }
}

async fn scripted_acm(describes: &[AcmDescribeResult]) -> (AwsProvider, Arc<ScriptedAcmService>) {
    let service = Arc::new(ScriptedAcmService {
        request_result: AcmRequestResult::Success,
        request_bodies: Arc::new(Mutex::new(Vec::new())),
        describe_dvo: Mutex::new(describes.iter().copied().collect()),
        describe_calls: Arc::new(AtomicUsize::new(0)),
        update_calls: Arc::new(AtomicUsize::new(0)),
    });
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    (provider_from_config(mock.sdk_config(REGION).await), service)
}

fn assert_permanent_read_back_partial(step: TestCreateStep, resource: &Resource, error: &str) {
    let TestCreateStep::Partial {
        state,
        reason,
        missing_attributes,
    } = step
    else {
        panic!("permanent read error must finish immediately as partial: {step:?}")
    };
    assert_eq!(state.identifier().as_str(), ACM_CERTIFICATE_ARN);
    for (name, value) in resource.resolved_attributes() {
        assert_eq!(state._attributes.get(&name), Some(&value), "{name}");
    }
    let schema =
        carina_provider_aws::schemas::generated::acm::certificate::acm_certificate_config().schema;
    let mut expected = schema.attributes().keys().cloned().collect::<Vec<_>>();
    expected.sort();
    assert_eq!(missing_attributes, expected);
    assert!(
        reason.contains(error),
        "error must remain in diagnostic: {reason}"
    );
}

fn expect_read_back_pending(step: TestCreateStep) -> AwsCreateToken {
    let TestCreateStep::Pending {
        token: token @ AwsCreateToken::ReadBack(_),
        identifier,
        ..
    } = step
    else {
        panic!("expected pending read-back: {step:?}")
    };
    assert_eq!(identifier.unwrap().as_str(), ACM_CERTIFICATE_ARN);
    token
}

#[tokio::test(start_paused = true)]
async fn permanent_read_back_error_finishes_partial_on_initial_step() {
    for (read, error) in [
        (AcmDescribeResult::AccessDenied, "AccessDeniedException"),
        (AcmDescribeResult::Error, "InvalidStateException"),
        (AcmDescribeResult::DecodeError, "failed"),
    ] {
        for method in ["DNS", "EMAIL"] {
            let (provider, service) = scripted_acm(&[read]).await;
            let mut resource = acm_certificate_resource();
            resource.set_attr("validation_method", string(method));
            let normalized = normalized_create_for_test(&resource).await;
            let step = provider
                .create_step(
                    &normalized,
                    TestCreateAttempt {
                        id: resource.id.clone(),
                    },
                )
                .await
                .unwrap();
            assert_permanent_read_back_partial(step, normalized.as_resource(), error);
            assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn permanent_read_back_error_finishes_partial_on_continuing_poll() {
    for (read, error) in [
        (AcmDescribeResult::AccessDenied, "AccessDeniedException"),
        (AcmDescribeResult::Error, "InvalidStateException"),
        (AcmDescribeResult::DecodeError, "failed"),
    ] {
        let (provider, service) = scripted_acm(&[AcmDescribeResult::Incomplete, read]).await;
        let mut resource = acm_certificate_resource();
        resource.set_attr("validation_method", string("DNS"));
        let token = expect_read_back_pending(
            provider
                .create_step(
                    &normalized_create_for_test(&resource).await,
                    TestCreateAttempt {
                        id: resource.id.clone(),
                    },
                )
                .await
                .unwrap(),
        );
        let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
            TestIssuedCreate {
                id: resource.id.clone(),
                identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                sent: Sent::AllRequestAttributes,
            },
            token,
        );
        let step = provider
            .poll_create_step(
                &resource,
                poll,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .unwrap();
        assert_permanent_read_back_partial(step, &resource, error);
        assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn permanent_read_back_error_finishes_partial_on_resume() {
    let (provider, service) = scripted_acm(&[
        AcmDescribeResult::Incomplete,
        AcmDescribeResult::AccessDenied,
    ])
    .await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let token = expect_read_back_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap(),
    );
    let resumed = AwsCreateResume::<TestResumeAttempt, TestResumedCreate>::Issued(
        TestResumedCreate {
            identifier: applied_identifier(ACM_CERTIFICATE_ARN),
        },
        token,
    );
    let step = provider
        .resume_create_step(
            &resource,
            resumed,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .unwrap();
    let TestResumeStep::Partial {
        identifier,
        attributes,
        missing_attributes,
        reason,
    } = step
    else {
        panic!("permanent error must finish resumed read-back immediately: {step:?}")
    };
    assert_permanent_read_back_partial(
        TestCreateStep::Partial {
            state: TestAppliedState::new(resource.id.clone(), identifier, attributes),
            missing_attributes,
            reason,
        },
        &resource,
        "AccessDeniedException",
    );
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 2);
    assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn permanent_update_read_back_error_finishes_partial_on_initial_step_and_poll() {
    for (poll, secret) in [(false, false), (true, false), (false, true), (true, true)] {
        let reads = if poll {
            vec![
                AcmDescribeResult::Incomplete,
                AcmDescribeResult::AccessDenied,
            ]
        } else {
            vec![AcmDescribeResult::AccessDenied]
        };
        let (provider, service) = scripted_acm(&reads).await;
        let mut resource = acm_certificate_resource();
        resource.set_attr("validation_method", string("DNS"));
        let mut from = carina_provider_aws::test_existing_state(
            resource.id.clone(),
            resource.resolved_attributes(),
        )
        .to_snapshot();
        if secret {
            from.attributes.insert(
                "domain_name".into(),
                carina_core::value::redact_secrets_in_value(&Value::Deferred(
                    carina_core::resource::DeferredValue::Secret(Box::new(string("example.com"))),
                ))
                .expect("persisted secret domain name"),
            );
        }
        let tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "Env".to_string(),
            string("test"),
        )])));
        let patch = UpdatePatch {
            ops: vec![PatchOp::Add {
                key: "tags".to_string(),
                value: tags.clone(),
            }],
        };
        let identifier = applied_identifier(ACM_CERTIFICATE_ARN);
        let step = provider
            .update_step(&resource.id, &identifier, &from, &patch)
            .await
            .unwrap();
        let step = if poll {
            let OperationStep::Pending { pending, .. } = step else {
                panic!("expected pending: {step:?}")
            };
            resource.set_attr("tags", tags.clone());
            provider
                .poll_update_step(
                    &carina_provider_aws::UpdateResource::new(&from, &patch),
                    &identifier,
                    pending,
                    PollRequest {
                        finality: PollFinality::Continue,
                    },
                )
                .await
                .unwrap()
        } else {
            step
        };
        let OperationStep::Done(done) = step else {
            panic!("permanent error must finish update read-back immediately: {step:?}")
        };
        let diagnostic = done.diagnostic().unwrap();
        let schema =
            carina_provider_aws::schemas::generated::acm::certificate::acm_certificate_config()
                .schema;
        let mut expected = schema.attributes().keys().cloned().collect::<Vec<_>>();
        expected.sort();
        assert_eq!(diagnostic.missing_attributes(), expected);
        assert!(diagnostic.reason().contains("AccessDeniedException"));
        let carina_core::provider::UpdateOutcomeKind::PartialSuccess { state, .. } = done.kind()
        else {
            panic!("expected partial update")
        };
        assert_eq!(state.identifier(), &identifier);
        assert_eq!(state.attributes().get("tags"), Some(&tags.to_snapshot()));
        assert_eq!(
            state.attributes().get("domain_name"),
            from.attributes.get("domain_name"),
            "partial updates must retain unused historical snapshots, including secret hashes"
        );
        assert_eq!(
            service.describe_calls.load(Ordering::Relaxed),
            if poll { 2 } else { 1 }
        );
        assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
        assert!(service.request_bodies.lock().unwrap().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn transient_read_back_service_error_stays_pending_then_completes() {
    for read in [AcmDescribeResult::Throttled, AcmDescribeResult::ServerError] {
        for method in ["DNS", "EMAIL"] {
            // Exhaust the SDK's three attempts before the next read-back step.
            let (provider, service) =
                scripted_acm(&[read, read, read, AcmDescribeResult::Populated]).await;
            let mut resource = acm_certificate_resource();
            resource.set_attr("validation_method", string(method));
            let token = expect_read_back_pending(
                provider
                    .create_step(
                        &normalized_create_for_test(&resource).await,
                        TestCreateAttempt {
                            id: resource.id.clone(),
                        },
                    )
                    .await
                    .unwrap(),
            );
            let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
                TestIssuedCreate {
                    id: resource.id.clone(),
                    identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                    sent: Sent::AllRequestAttributes,
                },
                token,
            );
            let step = provider
                .poll_create_step(
                    &resource,
                    poll,
                    PollRequest {
                        finality: PollFinality::Continue,
                    },
                )
                .await
                .unwrap();
            assert!(matches!(step, TestCreateStep::Success(_)), "{step:?}");
            assert_eq!(service.describe_calls.load(Ordering::Relaxed), 4);
            assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
        }
    }
}

#[derive(Debug)]
struct ReadBackDispatchFailure {
    inner: SharedHttpConnector,
    attempts: Arc<AtomicUsize>,
}

impl aws_smithy_runtime_api::client::http::HttpConnector for ReadBackDispatchFailure {
    fn call(
        &self,
        request: aws_smithy_runtime_api::client::orchestrator::HttpRequest,
    ) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        if request.headers().get("x-amz-target") == Some("CertificateManager.DescribeCertificate")
            && self.attempts.fetch_add(1, Ordering::Relaxed) < 3
        {
            return aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(async {
                Err(aws_smithy_runtime_api::client::result::ConnectorError::io(
                    std::io::Error::other("read connection reset").into(),
                ))
            });
        }
        self.inner.call(request)
    }
}

#[tokio::test(start_paused = true)]
async fn transient_read_back_dispatch_failure_stays_pending_then_completes() {
    let service = Arc::new(ScriptedAcmService {
        request_result: AcmRequestResult::Success,
        request_bodies: Arc::new(Mutex::new(Vec::new())),
        describe_dvo: Mutex::new(VecDeque::from([AcmDescribeResult::Populated])),
        describe_calls: Arc::new(AtomicUsize::new(0)),
        update_calls: Arc::new(AtomicUsize::new(0)),
    });
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let config = mock.sdk_config(REGION).await;
    let inner = config.http_client().unwrap().clone();
    let attempts = Arc::new(AtomicUsize::new(0));
    let selected_attempts = Arc::clone(&attempts);
    let client = http_client_fn(move |settings, components| {
        SharedHttpConnector::new(ReadBackDispatchFailure {
            inner: inner.http_connector(settings, components),
            attempts: Arc::clone(&selected_attempts),
        })
    });
    let provider = provider_from_config(config.into_builder().http_client(client).build());
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let token = expect_read_back_pending(
        provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap(),
    );
    let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
        TestIssuedCreate {
            id: resource.id.clone(),
            identifier: applied_identifier(ACM_CERTIFICATE_ARN),
            sent: Sent::AllRequestAttributes,
        },
        token,
    );
    let step = provider
        .poll_create_step(
            &resource,
            poll,
            PollRequest {
                finality: PollFinality::Continue,
            },
        )
        .await
        .unwrap();
    assert!(matches!(step, TestCreateStep::Success(_)), "{step:?}");
    assert_eq!(attempts.load(Ordering::Relaxed), 4);
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
    assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn acm_email_read_back_completes_without_dvo() {
    let (provider, service) = scripted_acm(&[AcmDescribeResult::Incomplete]).await;
    let resource = acm_certificate_resource();
    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(step, TestCreateStep::Success(_)));
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn acm_failed_read_back_is_terminal_failure_without_exhausting_budget() {
    let (provider, service) = scripted_acm(&[AcmDescribeResult::Failed]).await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let error = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap_err();
    let TestCreateVerdict::IdentifierReported(reported) = error else {
        panic!("FAILED preserves the certificate: {error:?}")
    };
    assert_eq!(reported.identifier.as_str(), ACM_CERTIFICATE_ARN);
    assert!(reported.error.to_string().contains("DOMAIN_NOT_ALLOWED"));
    assert!(!reported.error.to_string().contains("budget"));
    assert!(matches!(
        reported.observation,
        Observation::Read(ReadOutcome::Found { .. })
    ));
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
    assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn resume_read_back_final_attempt_is_partial_with_exact_missing_attributes() {
    let (provider, service) =
        scripted_acm(&[AcmDescribeResult::Incomplete, AcmDescribeResult::PartialDvo]).await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap();
    let TestCreateStep::Pending { token, .. } = step else {
        panic!("expected pending")
    };
    let resume = AwsCreateResume::<TestResumeAttempt, TestResumedCreate>::Issued(
        TestResumedCreate {
            identifier: applied_identifier(ACM_CERTIFICATE_ARN),
        },
        token,
    );
    let step = provider
        .resume_create_step(
            &resource,
            resume,
            PollRequest {
                finality: PollFinality::FinalAttempt,
            },
        )
        .await
        .unwrap();
    let TestResumeStep::Partial {
        identifier,
        attributes,
        reason,
        missing_attributes,
    } = step
    else {
        panic!("expected partial resume: {step:?}")
    };
    assert_eq!(identifier.as_str(), ACM_CERTIFICATE_ARN);
    assert_eq!(
        attributes.get("certificate_arn"),
        Some(&string(ACM_CERTIFICATE_ARN))
    );
    assert_eq!(attributes.get("domain_name"), Some(&string("example.com")));
    assert_eq!(missing_attributes, ["domain_validation_options"]);
    assert!(reason.contains("domain_validation_options"));
    assert!(attributes.contains_key("status"));
    assert!(!attributes.contains_key("domain_validation_options"));
    assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn update_read_back_recovers_or_finishes_partial_without_replaying_mutation() {
    for first in [AcmDescribeResult::Incomplete, AcmDescribeResult::NotFound] {
        for final_read in [AcmDescribeResult::PartialDvo, AcmDescribeResult::Populated] {
            let (provider, service) = scripted_acm(&[first, final_read]).await;
            let mut resource = acm_certificate_resource();
            resource.set_attr("validation_method", string("DNS"));
            let mut from = carina_provider_aws::test_existing_state(
                resource.id.clone(),
                resource.resolved_attributes(),
            )
            .to_snapshot();
            // A create-only secret is historical throughout mutation and polling.
            from.attributes.insert(
                "domain_name".into(),
                carina_core::value::redact_secrets_in_value(&Value::Deferred(
                    carina_core::resource::DeferredValue::Secret(Box::new(string("example.com"))),
                ))
                .expect("persisted secret domain name"),
            );
            let tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Env".to_string(),
                string("test"),
            )])));
            let patch = UpdatePatch {
                ops: vec![PatchOp::Add {
                    key: "tags".to_string(),
                    value: tags,
                }],
            };
            let identifier = applied_identifier(ACM_CERTIFICATE_ARN);
            let step = provider
                .update_step(&resource.id, &identifier, &from, &patch)
                .await
                .unwrap();
            let OperationStep::Pending { pending, phase, .. } = step else {
                panic!("expected update read-back pending: {step:?}")
            };
            assert_eq!(phase, carina_provider_protocol::PendingPhase::ReadBack);
            assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
            let step = provider
                .poll_update_step(
                    &carina_provider_aws::UpdateResource::new(&from, &patch),
                    &identifier,
                    pending,
                    PollRequest {
                        finality: PollFinality::FinalAttempt,
                    },
                )
                .await
                .unwrap();
            let OperationStep::Done(done) = step else {
                panic!("final update poll cannot be pending")
            };
            if matches!(final_read, AcmDescribeResult::Populated) {
                assert!(done.diagnostic().is_none());
            } else {
                let diagnostic = done.diagnostic().expect("incomplete DVO is partial");
                assert_eq!(
                    diagnostic.missing_attributes(),
                    &["domain_validation_options".to_string()]
                );
            }
            assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
            assert_eq!(service.describe_calls.load(Ordering::Relaxed), 2);
            assert!(service.request_bodies.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn read_back_final_attempt_from_operation_is_terminal() {
    for read in ["not_found", "read_error", "available"] {
        let (provider, service) = provider_with_ec2(&["available", read]).await;
        let resource = nat_gateway_resource();
        let token = expect_create_pending(
            provider
                .create_step(
                    &normalized_create_for_test(&resource).await,
                    TestCreateAttempt {
                        id: resource.id.clone(),
                    },
                )
                .await
                .unwrap(),
        );
        let step = provider
            .poll_create_step(
                &resource,
                poll_attempt(&resource, token),
                PollRequest {
                    finality: PollFinality::FinalAttempt,
                },
            )
            .await
            .unwrap();
        if read == "available" {
            assert!(matches!(step, TestCreateStep::Success(_)));
        } else {
            let TestCreateStep::Partial {
                state,
                missing_attributes,
                reason,
            } = step
            else {
                panic!("final operation poll cannot begin a pending read-back")
            };
            assert_eq!(state.identifier().as_str(), NAT_GATEWAY_ID);
            let schema =
                carina_provider_aws::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config()
                    .schema;
            let mut expected = schema.attributes().keys().cloned().collect::<Vec<_>>();
            expected.sort();
            assert_eq!(missing_attributes, expected);
            assert!(!reason.contains("wait"));
        }
        assert_eq!(service.create_nat_gateway_calls(), 1);
        assert_eq!(service.create_tags_calls(), 1);
        assert_eq!(service.describe_nat_gateways_calls(), 2);
    }
}

#[tokio::test]
async fn update_read_back_continue_polls_only_reads() {
    let (provider, service) = scripted_acm(&[
        AcmDescribeResult::Incomplete,
        AcmDescribeResult::PartialDvo,
        AcmDescribeResult::Populated,
    ])
    .await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let from = carina_provider_aws::test_existing_state(
        resource.id.clone(),
        resource.resolved_attributes(),
    );
    let patch = UpdatePatch {
        ops: vec![PatchOp::Add {
            key: "tags".to_string(),
            value: Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Env".to_string(),
                string("test"),
            )]))),
        }],
    };
    let identifier = applied_identifier(ACM_CERTIFICATE_ARN);
    let mut step = provider
        .update_step(&resource.id, &identifier, &from, &patch)
        .await
        .unwrap();
    for _ in 0..2 {
        let OperationStep::Pending { pending, phase, .. } = step else {
            panic!("expected pending read-back")
        };
        assert_eq!(phase, carina_provider_protocol::PendingPhase::ReadBack);
        step = provider
            .poll_update_step(
                &carina_provider_aws::UpdateResource::new(&from.to_snapshot(), &patch),
                &identifier,
                pending,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .unwrap();
    }
    assert!(matches!(step, OperationStep::Done(_)));
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn read_back_rejects_resource_and_identifier_mismatch_before_reading() {
    for field in ["resource_type", "identifier"] {
        let (provider, service) = scripted_acm(&[AcmDescribeResult::Incomplete]).await;
        let mut resource = acm_certificate_resource();
        resource.set_attr("validation_method", string("DNS"));
        let step = provider
            .create_step(
                &normalized_create_for_test(&resource).await,
                TestCreateAttempt {
                    id: resource.id.clone(),
                },
            )
            .await
            .unwrap();
        let TestCreateStep::Pending { token, .. } = step else {
            panic!("expected pending")
        };
        let mut json = serde_json::to_value(token).unwrap();
        json[field] = serde_json::json!("wrong-route");
        let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
            TestIssuedCreate {
                id: resource.id.clone(),
                identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                sent: Sent::AllRequestAttributes,
            },
            serde_json::from_value(json).unwrap(),
        );
        let verdict = provider
            .poll_create_step(
                &resource,
                poll,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(verdict.provider_error().variant_name(), "internal");
        assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
        assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn failed_certificate_tag_update_succeeds_initial_and_poll() {
    for first in [AcmDescribeResult::Failed, AcmDescribeResult::Incomplete] {
        let (provider, service) = scripted_acm(&[first, AcmDescribeResult::Failed]).await;
        let mut resource = acm_certificate_resource();
        resource.set_attr("validation_method", string("DNS"));
        let from = carina_provider_aws::test_existing_state(
            resource.id.clone(),
            resource.resolved_attributes(),
        );
        let patch = UpdatePatch {
            ops: vec![PatchOp::Add {
                key: "tags".to_string(),
                value: Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                    "Env".to_string(),
                    string("test"),
                )]))),
            }],
        };
        let identifier = applied_identifier(ACM_CERTIFICATE_ARN);
        let step = provider
            .update_step(&resource.id, &identifier, &from, &patch)
            .await
            .unwrap();
        let done = match step {
            OperationStep::Done(done) => done,
            OperationStep::Pending { pending, .. } => {
                let OperationStep::Done(done) = provider
                    .poll_update_step(
                        &carina_provider_aws::UpdateResource::new(&from.to_snapshot(), &patch),
                        &identifier,
                        pending,
                        PollRequest {
                            finality: PollFinality::FinalAttempt,
                        },
                    )
                    .await
                    .unwrap()
                else {
                    panic!("final update must finish")
                };
                done
            }
            other => panic!("update must succeed: {other:?}"),
        };
        assert_eq!(
            done.diagnostic().unwrap().missing_attributes(),
            ["domain_validation_options"]
        );
        assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
        assert!(service.request_bodies.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn acm_http_read_back_completes_without_dvo() {
    let (provider, service) = scripted_acm(&[AcmDescribeResult::Incomplete]).await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("HTTP"));
    let step = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(step, TestCreateStep::Success(_)));
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn acm_failed_read_back_resume_stays_terminal() {
    let (provider, service) =
        scripted_acm(&[AcmDescribeResult::Incomplete, AcmDescribeResult::Failed]).await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let TestCreateStep::Pending { token, .. } = provider
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("expected read-back")
    };
    let resumed = AwsCreateResume::<TestResumeAttempt, TestResumedCreate>::Issued(
        TestResumedCreate {
            identifier: applied_identifier(ACM_CERTIFICATE_ARN),
        },
        token,
    );
    let error = provider
        .resume_create_step(
            &resource,
            resumed,
            PollRequest {
                finality: PollFinality::FinalAttempt,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, TestResumeVerdict::IdentifierReported(_)),
        "FAILED create stays terminal: {error:?}"
    );
    let TestResumeVerdict::IdentifierReported(error) = error else {
        unreachable!()
    };
    assert!(error.to_string().contains("DOMAIN_NOT_ALLOWED"));
    assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn acm_final_partial_after_poll_retains_request_and_arn() {
    for final_read in [AcmDescribeResult::NotFound, AcmDescribeResult::Error] {
        for resume in [false, true] {
            let (provider, service) = scripted_acm(&[
                AcmDescribeResult::Incomplete,
                AcmDescribeResult::Incomplete,
                final_read,
            ])
            .await;
            let mut resource = acm_certificate_resource();
            resource.set_attr("validation_method", string("DNS"));
            let TestCreateStep::Pending { token, .. } = provider
                .create_step(
                    &normalized_create_for_test(&resource).await,
                    TestCreateAttempt {
                        id: resource.id.clone(),
                    },
                )
                .await
                .unwrap()
            else {
                panic!("expected pending")
            };
            let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
                TestIssuedCreate {
                    id: resource.id.clone(),
                    identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                    sent: Sent::AllRequestAttributes,
                },
                token,
            );
            let TestCreateStep::Pending { token, .. } = provider
                .poll_create_step(
                    &resource,
                    poll,
                    PollRequest {
                        finality: PollFinality::Continue,
                    },
                )
                .await
                .unwrap()
            else {
                panic!("expected continuing read-back")
            };
            let attributes = if resume {
                let resumed = AwsCreateResume::<TestResumeAttempt, TestResumedCreate>::Issued(
                    TestResumedCreate {
                        identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                    },
                    token,
                );
                let TestResumeStep::Partial { attributes, .. } = provider
                    .resume_create_step(
                        &resource,
                        resumed,
                        PollRequest {
                            finality: PollFinality::FinalAttempt,
                        },
                    )
                    .await
                    .unwrap()
                else {
                    panic!("expected terminal partial")
                };
                attributes
            } else {
                let poll = AwsCreatePoll::<TestPollAttempt, TestIssuedCreate>::Issued(
                    TestIssuedCreate {
                        id: resource.id.clone(),
                        identifier: applied_identifier(ACM_CERTIFICATE_ARN),
                        sent: Sent::AllRequestAttributes,
                    },
                    token,
                );
                let TestCreateStep::Partial { state, .. } = provider
                    .poll_create_step(
                        &resource,
                        poll,
                        PollRequest {
                            finality: PollFinality::FinalAttempt,
                        },
                    )
                    .await
                    .unwrap()
                else {
                    panic!("expected terminal partial")
                };
                state._attributes
            };
            assert_eq!(attributes.get("domain_name"), Some(&string("example.com")));
            assert_eq!(
                attributes.get("certificate_arn"),
                Some(&string(ACM_CERTIFICATE_ARN))
            );
            assert_eq!(service.describe_calls.load(Ordering::Relaxed), 3);
            assert_eq!(service.request_bodies.lock().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn read_back_resume_rederives_current_requirements_and_missing_attributes() {
    let (first, _) = scripted_acm(&[AcmDescribeResult::NotFound]).await;
    let mut resource = acm_certificate_resource();
    let TestCreateStep::Pending { token, .. } = first
        .create_step(
            &normalized_create_for_test(&resource).await,
            TestCreateAttempt {
                id: resource.id.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("expected pending EMAIL read-back")
    };
    // The continuation contains no declaration or schema snapshot; fresh DNS
    // requirements apply even when the previous request did not await DVO.
    let serialized = serde_json::to_value(token).unwrap();
    assert_eq!(
        serialized
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["identifier", "phase", "resource_type"]
    );
    let token = serde_json::from_value(serialized).unwrap();
    let (fresh, service) = scripted_acm(&[AcmDescribeResult::Incomplete]).await;
    resource.set_attr("validation_method", string("DNS"));
    let resumed = AwsCreateResume::<TestResumeAttempt, TestResumedCreate>::Issued(
        TestResumedCreate {
            identifier: applied_identifier(ACM_CERTIFICATE_ARN),
        },
        token,
    );
    let TestResumeStep::Partial {
        missing_attributes, ..
    } = fresh
        .resume_create_step(
            &resource,
            resumed,
            PollRequest {
                finality: PollFinality::FinalAttempt,
            },
        )
        .await
        .unwrap()
    else {
        panic!("current DNS declaration must await DVO")
    };
    assert_eq!(missing_attributes, ["domain_validation_options"]);
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
    assert!(service.request_bodies.lock().unwrap().is_empty());
}

#[tokio::test]
async fn acm_failed_update_read_back_resumes_without_reapplying_tags() {
    let (first, first_service) = scripted_acm(&[AcmDescribeResult::Incomplete]).await;
    let mut resource = acm_certificate_resource();
    resource.set_attr("validation_method", string("DNS"));
    let from = carina_provider_aws::test_existing_state(
        resource.id.clone(),
        resource.resolved_attributes(),
    );
    let tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([(
        "Env".to_string(),
        string("test"),
    )])));
    resource.set_attr("tags", tags.clone());
    let patch = UpdatePatch {
        ops: vec![PatchOp::Add {
            key: "tags".to_string(),
            value: tags,
        }],
    };
    let identifier = applied_identifier(ACM_CERTIFICATE_ARN);
    let OperationStep::Pending { pending, .. } = first
        .update_step(&resource.id, &identifier, &from, &patch)
        .await
        .unwrap()
    else {
        panic!("expected update read-back")
    };
    let token = serde_json::to_string(&pending).unwrap();
    assert_eq!(first_service.update_calls.load(Ordering::Relaxed), 1);
    drop(first);

    let (fresh, service) = scripted_acm(&[AcmDescribeResult::Failed]).await;
    let OperationStep::Done(done) = fresh
        .poll_update_step(
            &carina_provider_aws::UpdateResource::new(&from.to_snapshot(), &patch),
            &identifier,
            serde_json::from_str(&token).unwrap(),
            PollRequest {
                finality: PollFinality::FinalAttempt,
            },
        )
        .await
        .unwrap()
    else {
        panic!("FAILED status must not fail the resumed update")
    };
    assert_eq!(
        done.diagnostic().unwrap().missing_attributes(),
        ["domain_validation_options"]
    );
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 0);
    assert_eq!(service.describe_calls.load(Ordering::Relaxed), 1);
    assert!(service.request_bodies.lock().unwrap().is_empty());
}
