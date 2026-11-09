//! Exercise the actual guest entry point through the SDK's fixed dispatch seam.
#![cfg(not(target_arch = "wasm32"))]
#![recursion_limit = "256"]

#[path = "common/acm_read_back.rs"]
mod acm_read_back;

#[allow(dead_code)]
mod guest {
    include!("../src/main.rs");

    use super::acm_read_back::{ARN, AcmService};
    use carina_plugin_sdk::{
        PendingPhase, PollFinality, dispatch_create, dispatch_poll_create, dispatch_resume_create,
    };
    use carina_provider_aws::BoundedSdkConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

    #[derive(Default)]
    struct RequestCounter {
        requests: AtomicUsize,
    }

    impl MockService for RequestCounter {
        fn service_name(&self) -> &str {
            "validation-counter"
        }

        fn url_patterns(&self) -> Vec<&str> {
            vec![r"^https://"]
        }

        fn handle(
            &self,
            _request: MockRequest,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = MockResponse> + Send + '_>>
        {
            self.requests.fetch_add(1, Ordering::Relaxed);
            Box::pin(std::future::ready(MockResponse::json(
                400,
                r#"{"__type":"InvalidParameterValue","message":"unexpected AWS request"}"#,
            )))
        }
    }

    fn validation_fixture() -> (AwsProcessProvider, Arc<RequestCounter>) {
        let service = Arc::new(RequestCounter::default());
        let mock = MockAws::builder()
            .with_service(Arc::clone(&service))
            .build();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = BoundedSdkConfig::for_test(runtime.block_on(mock.sdk_config("us-east-1")));
        let aws = AwsProvider::from_sdk_config("us-east-1".to_string(), vec![], vec![], &config);
        (
            AwsProcessProvider {
                runtime,
                provider: Some(aws),
                normalizer: AwsNormalizer,
                diagnostic_session: carina_core::provider::ProviderPlan::state_only()
                    .session("aws", None, None),
            },
            service,
        )
    }

    #[test]
    fn guest_create_runs_sqs_and_nat_cross_field_validators_before_send() {
        let (provider, service) = validation_fixture();
        let queue = |name: &str, fifo| {
            BTreeMap::from([
                ("queue_name".into(), proto::Value::String(name.into())),
                ("fifo_queue".into(), proto::Value::Bool(fifo)),
            ])
        };
        let redrive = |permission: &str, count: usize| {
            let fields = BTreeMap::from([
                (
                    "redrive_permission".into(),
                    proto::Value::String(permission.into()),
                ),
                (
                    "source_queue_arns".into(),
                    proto::Value::List(
                        (0..count)
                            .map(|i| {
                                proto::Value::String(format!(
                                    "arn:aws:sqs:us-east-1:123456789012:source-{i}"
                                ))
                            })
                            .collect(),
                    ),
                ),
            ]);
            BTreeMap::from([
                (
                    "queue_name".into(),
                    proto::Value::String("standard-queue".into()),
                ),
                ("redrive_allow_policy".into(), proto::Value::Map(fields)),
            ])
        };
        let throughput = |scope: &str| {
            let mut attributes = queue("valid.fifo", true);
            attributes.insert(
                "fifo_throughput_limit".into(),
                proto::Value::String("perMessageGroupId".into()),
            );
            attributes.insert(
                "deduplication_scope".into(),
                proto::Value::String(scope.into()),
            );
            attributes
        };
        for (resource_type, attributes, message) in [
            (
                "sqs.Queue",
                BTreeMap::from([
                    (
                        "queue_name".into(),
                        proto::Value::String("standard-queue".into()),
                    ),
                    ("fifo_queue".into(), proto::Value::Bool(false)),
                    (
                        "content_based_deduplication".into(),
                        proto::Value::Bool(true),
                    ),
                ]),
                "content_based_deduplication requires fifo_queue = true",
            ),
            (
                "sqs.Queue",
                queue("private-name", true),
                "queue_name must end with .fifo",
            ),
            (
                "sqs.Queue",
                throughput("queue"),
                "fifo_throughput_limit requires deduplication_scope = messageGroup",
            ),
            (
                "sqs.Queue",
                redrive("allowAll", 1),
                "redrive_allow_policy.source_queue_arns may only be set",
            ),
            (
                "sqs.Queue",
                redrive("byQueue", 0),
                "redrive_allow_policy.redrive_permission requires redrive_allow_policy.source_queue_arns",
            ),
            (
                "sqs.Queue",
                redrive("byQueue", 11),
                "redrive_allow_policy.source_queue_arns must contain at most 10 items",
            ),
            (
                "ec2.NatGateway",
                BTreeMap::from([(
                    "availability_mode".into(),
                    proto::Value::String("zonal".into()),
                )]),
                "subnet_id is required when availability_mode = zonal",
            ),
        ] {
            let id = proto::ResourceId {
                provider: "aws".into(),
                resource_type: resource_type.into(),
                identity: "invalid-create".into(),
            };
            let request = proto::CreateRequest {
                resource: proto::Resource {
                    id: id.clone(),
                    attributes: attributes.clone(),
                    directives: Default::default(),
                },
            };
            let verdict = dispatch_create(&provider, &id, request)
                .expect_err("guest create must reject invalid resolved inputs");
            assert_eq!(service.requests.load(Ordering::Relaxed), 0);
            let proto::CreateVerdict::NotCreated(error) = *verdict else {
                panic!("validation must report NotCreated: {verdict:?}");
            };
            let error = error.to_wire();
            assert_eq!(
                error.kind,
                carina_provider_protocol::types::ProviderErrorKind::InvalidInput
            );
            assert_eq!(error.resource_id.as_ref(), Some(&id));
            assert!(error.message.contains(message), "{error:?}");
            assert!(!error.message.contains("private-name"));
            assert!(!error.message.contains("arn:aws:sqs"));
            if resource_type == "sqs.Queue" {
                let identifier = proto::AppliedIdentifier::new(
                    "https://sqs.us-east-1.amazonaws.com/123456789012/validation-queue",
                )
                .unwrap();
                let request = proto::UpdateRequest {
                    from: proto::PersistedState {
                        id: id.clone(),
                        identifier: Some(identifier.as_str().into()),
                        exists: true,
                        attributes: attributes
                            .into_iter()
                            .map(|(key, value)| (key, value.into()))
                            .collect(),
                    },
                    patch: proto::UpdatePatch {
                        ops: vec![proto::PatchOp::Add {
                            key: "delay_seconds".into(),
                            value: proto::Value::Int(1),
                        }],
                    },
                };
                let error = provider
                    .update(&id, &identifier, request)
                    .expect_err(
                        "guest update must validate unchanged historical cross-field inputs",
                    )
                    .to_wire();
                assert_eq!(service.requests.load(Ordering::Relaxed), 0);
                assert_eq!(
                    error.kind,
                    carina_provider_protocol::types::ProviderErrorKind::InvalidInput
                );
                assert_eq!(error.resource_id.as_ref(), Some(&id));
                assert!(error.message.contains(message), "{error:?}");
            }
        }

        let mut attributes = redrive("byQueue", 1);
        attributes.extend(throughput("messageGroup"));
        let id = proto::ResourceId {
            provider: "aws".into(),
            resource_type: "sqs.Queue".into(),
            identity: "valid-create".into(),
        };
        let request = proto::CreateRequest {
            resource: proto::Resource {
                id: id.clone(),
                attributes,
                directives: Default::default(),
            },
        };
        let before = service.requests.load(Ordering::Relaxed);
        let verdict = dispatch_create(&provider, &id, request);
        assert!(
            service.requests.load(Ordering::Relaxed) > before,
            "valid combinations must pass validation and reach the mock AWS service: {verdict:?}"
        );
    }

    #[test]
    fn guest_update_rejects_resolved_sqs_fifo_conflicts_before_send() {
        let (provider, service) = validation_fixture();
        let id = proto::ResourceId {
            provider: "aws".into(),
            resource_type: "sqs.Queue".into(),
            identity: "resolved-queue".into(),
        };
        let identifier = proto::AppliedIdentifier::new(
            "https://sqs.us-east-1.amazonaws.com/123456789012/resolved-queue",
        )
        .unwrap();
        let request = proto::UpdateRequest {
            from: proto::PersistedState {
                id: id.clone(),
                identifier: Some(identifier.as_str().into()),
                exists: true,
                attributes: BTreeMap::from([
                    (
                        "queue_name".into(),
                        proto::Value::String("resolved-queue".into()).into(),
                    ),
                    ("fifo_queue".into(), proto::Value::Bool(false).into()),
                    (
                        "content_based_deduplication".into(),
                        proto::Value::Bool(false).into(),
                    ),
                ]),
            },
            patch: proto::UpdatePatch {
                ops: vec![proto::PatchOp::Replace {
                    key: "content_based_deduplication".into(),
                    value: proto::Value::Bool(true),
                }],
            },
        };
        let result = provider.update(&id, &identifier, request);
        assert_eq!(service.requests.load(Ordering::Relaxed), 0);
        let error = result
            .expect_err("guest update must validate the reconstructed target")
            .to_wire();
        assert_eq!(
            error.kind,
            carina_provider_protocol::types::ProviderErrorKind::InvalidInput
        );
        assert_eq!(error.resource_id.as_ref(), Some(&id));
        assert!(
            error
                .message
                .contains("content_based_deduplication requires fifo_queue = true"),
            "{error:?}"
        );
    }

    fn fixture(populated: &[bool]) -> (AwsProcessProvider, Arc<AcmService>, proto::CreateRequest) {
        let service = Arc::new(AcmService {
            populated: Mutex::new(populated.iter().copied().collect()),
            absent: std::sync::atomic::AtomicBool::new(false),
            requests: AtomicUsize::new(0),
            describes: AtomicUsize::new(0),
            mutations: AtomicUsize::new(0),
        });
        let mock = MockAws::builder()
            .with_service(Arc::clone(&service))
            .build();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = BoundedSdkConfig::for_test(runtime.block_on(mock.sdk_config("us-east-1")));
        let aws = AwsProvider::from_sdk_config("us-east-1".to_string(), vec![], vec![], &config);
        let provider = AwsProcessProvider {
            runtime,
            provider: Some(aws),
            normalizer: AwsNormalizer,
            diagnostic_session: carina_core::provider::ProviderPlan::state_only()
                .session("aws", None, None),
        };
        let request = proto::CreateRequest {
            resource: proto::Resource {
                id: proto::ResourceId {
                    provider: "aws".to_string(),
                    resource_type: "acm.Certificate".to_string(),
                    identity: "guest".to_string(),
                },
                attributes: BTreeMap::from([
                    (
                        "domain_name".to_string(),
                        proto::Value::String("example.com".to_string()),
                    ),
                    (
                        "validation_method".to_string(),
                        proto::Value::String("DNS".to_string()),
                    ),
                ]),
                directives: Default::default(),
            },
        };
        (provider, service, request)
    }

    // Inject a transient step failure at the real guest preservation adapters.
    // HTTP failures normally return Pending and would not exercise preserve.
    struct PreservingGuest {
        inner: AwsProcessProvider,
        initial: bool,
    }

    fn transient_error() -> CoreProviderError {
        CoreProviderError::api_error_caused_by(
            "temporary read-back failure",
            std::io::Error::other("connection reset"),
        )
    }

    impl CarinaProvider for PreservingGuest {
        type CreateToken = AwsCreateToken;
        type UpdateToken = AwsUpdateToken;
        type DeleteToken = AwsDeleteToken;

        fn info(&self) -> proto::ProviderInfo {
            self.inner.info()
        }
        fn schemas(&self) -> Vec<ValidatedResourceSchema> {
            self.inner.schemas()
        }
        fn reference_forms(&self) -> ReferenceForms {
            self.inner.reference_forms()
        }
        fn assignability_edges(&self) -> Vec<AssignabilityEdge> {
            self.inner.assignability_edges()
        }
        fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
            self.inner.provider_config_attribute_types()
        }
        fn validate_config(
            &self,
            attrs: &BTreeMap<String, proto::Value>,
        ) -> Result<(), proto::ProviderError> {
            self.inner.validate_config(attrs)
        }
        fn initialize(
            &mut self,
            attrs: &BTreeMap<String, proto::Value>,
        ) -> Result<proto::ReferenceScope, proto::ProviderError> {
            self.inner.initialize(attrs)
        }
        fn read(
            &self,
            id: &proto::ResourceId,
            identifier: &proto::AppliedIdentifier,
            _request: proto::ReadRequest,
        ) -> Result<proto::ReadOutcome, proto::ProviderError> {
            self.inner.read(id, identifier, _request)
        }
        fn read_data_source(
            &self,
            resource: &proto::Resource,
        ) -> Result<proto::State, proto::ProviderError> {
            self.inner.read_data_source(resource)
        }
        fn update(
            &self,
            id: &proto::ResourceId,
            identifier: &proto::AppliedIdentifier,
            request: proto::UpdateRequest,
        ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
            self.inner.update(id, identifier, request)
        }
        fn poll_update(
            &self,
            id: &proto::ResourceId,
            identifier: &proto::AppliedIdentifier,
            request: proto::UpdateRequest,
            pending: Pending<Self::UpdateToken>,
            poll_request: PollRequest,
        ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
            self.inner
                .poll_update(id, identifier, request, pending, poll_request)
        }
        fn delete(
            &self,
            id: &proto::ResourceId,
            identifier: &proto::AppliedIdentifier,
            request: proto::DeleteRequest,
        ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
            self.inner.delete(id, identifier, request)
        }
        fn poll_delete(
            &self,
            id: &proto::ResourceId,
            _identifier: &proto::AppliedIdentifier,
            _request: proto::DeleteRequest,
            pending: PendingDelete<Self::DeleteToken>,
        ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
            self.inner.poll_delete(id, _identifier, _request, pending)
        }
        fn required_permissions(
            &self,
            _id: &proto::ResourceId,
            _op: carina_plugin_sdk::PlanOp,
        ) -> Result<Vec<String>, proto::ProviderError> {
            self.inner.required_permissions(_id, _op)
        }
        fn create(
            &self,
            id: &proto::ResourceId,
            request: proto::CreateRequest,
            attempt: SdkCreateAttempt,
        ) -> Result<SdkCreateStep<AwsCreateToken>, SdkCreateVerdict<AwsCreateToken>> {
            if self.initial {
                let token = serde_json::from_value(serde_json::json!({
                    "phase": "read_back", "identifier": ARN, "resource_type": "acm.Certificate"
                }))
                .unwrap();
                return Err(GuestCreateAttempt(attempt)
                    .issued(
                        CoreAppliedIdentifier::new(ARN).unwrap(),
                        CoreSent::only(["domain_name"]),
                    )
                    .preserve(token, transient_error()));
            }
            self.inner.create(id, request, attempt)
        }

        fn poll_create(
            &self,
            _id: &proto::ResourceId,
            _request: proto::CreateRequest,
            poll: SdkPoll<AwsCreateToken>,
            _poll_request: PollRequest,
        ) -> Result<SdkCreateStep<AwsCreateToken>, SdkPollVerdict<AwsCreateToken>> {
            let SdkPoll::Issued(issued, token) = poll else {
                panic!("read-back must have an identifier")
            };
            Err(GuestPolledIssuedCreate(issued).preserve(token, transient_error()))
        }

        fn resume_create(
            &self,
            _id: &proto::ResourceId,
            _request: proto::CreateRequest,
            resume: SdkResume<AwsCreateToken>,
            _poll_request: PollRequest,
        ) -> Result<SdkResumeStep<AwsCreateToken>, SdkResumeVerdict<AwsCreateToken>> {
            let SdkResume::Issued(resumed, token) = resume else {
                panic!("read-back must have an identifier")
            };
            Err(GuestResumedCreate(resumed).preserve(token, transient_error()))
        }
    }

    #[test]
    fn guest_read_back_transient_failure_preserves_continuation() {
        for call in ["poll", "resume", "create"] {
            let (inner, service, request) = fixture(&[false]);
            let provider = PreservingGuest {
                inner,
                initial: call == "create",
            };
            let id = request.resource.id.clone();
            let pending = if provider.initial {
                let verdict = dispatch_create(&provider, &id, request).unwrap_err();
                let proto::CreateVerdict::InFlight(in_flight) = *verdict else {
                    panic!("a transient failure must preserve read-back: {verdict:?}")
                };
                assert!(
                    in_flight
                        .error
                        .to_wire()
                        .cause
                        .as_deref()
                        .unwrap()
                        .contains("connection reset")
                );
                assert_eq!(in_flight.pending.sent, CoreSent::only(["domain_name"]));
                in_flight.pending
            } else {
                let proto::CreateStep::Pending(pending) =
                    dispatch_create(&provider, &id, request.clone()).unwrap()
                else {
                    panic!("expected read-back")
                };
                let token = pending.token.clone();
                let sent = pending.sent.clone();
                if call == "poll" {
                    let verdict = dispatch_poll_create(
                        &provider,
                        &id,
                        request,
                        pending.progress,
                        pending.sent,
                        pending.token,
                        PollRequest {
                            finality: PollFinality::Continue,
                        },
                    )
                    .unwrap_err();
                    let proto::PollVerdict::InFlight(in_flight) = *verdict else {
                        panic!("a transient poll failure must preserve read-back: {verdict:?}")
                    };
                    assert!(
                        in_flight
                            .error
                            .to_wire()
                            .cause
                            .as_deref()
                            .unwrap()
                            .contains("connection reset")
                    );
                    assert_eq!(in_flight.pending.token, token);
                    assert_eq!(in_flight.pending.sent, sent);
                    in_flight.pending
                } else {
                    let verdict = dispatch_resume_create(
                        &provider,
                        &id,
                        request,
                        pending.progress,
                        pending.token,
                        PollRequest {
                            finality: PollFinality::Continue,
                        },
                    )
                    .unwrap_err();
                    let proto::ResumeVerdict::InFlight(in_flight) = *verdict else {
                        panic!("a transient resume failure must preserve read-back: {verdict:?}")
                    };
                    assert!(
                        in_flight
                            .error
                            .to_wire()
                            .cause
                            .as_deref()
                            .unwrap()
                            .contains("connection reset")
                    );
                    assert_eq!(in_flight.pending.token, token);
                    assert_eq!(in_flight.pending.progress.phase(), PendingPhase::ReadBack);
                    assert_eq!(
                        in_flight.pending.progress.identifier().unwrap().as_str(),
                        ARN
                    );
                    assert_eq!(in_flight.pending.poll_after_secs, 5);
                    assert_eq!(service.requests.load(Ordering::Relaxed), 1);
                    continue;
                }
            };
            assert_eq!(pending.progress.phase(), PendingPhase::ReadBack);
            assert_eq!(pending.progress.identifier().unwrap().as_str(), ARN);
            assert!(matches!(pending.token, AwsCreateToken::ReadBack(_)));
            assert_eq!(pending.poll_after_secs, 5);
            assert_eq!(
                service.requests.load(Ordering::Relaxed),
                usize::from(call != "create")
            );
        }
    }

    #[test]
    fn guest_read_back_create_and_resume_keep_phase_and_finality() {
        for populated in [false, true] {
            let (provider, service, request) = fixture(&[false, false, populated]);
            let id = request.resource.id.clone();
            let proto::CreateStep::Pending(pending) =
                dispatch_create(&provider, &id, request.clone()).unwrap()
            else {
                panic!("expected pending create")
            };
            assert_eq!(pending.progress.phase(), PendingPhase::ReadBack);
            assert_eq!(pending.progress.identifier().unwrap().as_str(), ARN);
            let proto::CreateStep::Pending(pending) = dispatch_poll_create(
                &provider,
                &id,
                request.clone(),
                pending.progress,
                pending.sent,
                pending.token,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .unwrap() else {
                panic!("expected continuing read-back")
            };
            assert_eq!(pending.progress.phase(), PendingPhase::ReadBack);
            let result = dispatch_resume_create(
                &provider,
                &id,
                request,
                pending.progress,
                pending.token,
                PollRequest {
                    finality: PollFinality::FinalAttempt,
                },
            )
            .unwrap();
            match result {
                proto::ResumeStep::Completed { identifier, .. } if populated => {
                    assert_eq!(identifier.as_str(), ARN)
                }
                proto::ResumeStep::PartialSuccess {
                    identifier,
                    diagnostic,
                    attributes,
                } if !populated => {
                    assert_eq!(identifier.as_str(), ARN);
                    assert_eq!(
                        diagnostic.missing_attributes(),
                        ["domain_validation_options"]
                    );
                    assert!(!attributes.contains_key("domain_validation_options"));
                }
                other => panic!("unexpected final resume: {other:?}"),
            }
            assert_eq!(service.requests.load(Ordering::Relaxed), 1);
            assert_eq!(service.describes.load(Ordering::Relaxed), 3);
        }
    }

    #[test]
    fn guest_read_back_final_create_poll_is_partial() {
        let (provider, service, request) = fixture(&[false, false]);
        let id = request.resource.id.clone();
        let proto::CreateStep::Pending(pending) =
            dispatch_create(&provider, &id, request.clone()).unwrap()
        else {
            panic!("expected pending")
        };
        let result = dispatch_poll_create(
            &provider,
            &id,
            request,
            pending.progress,
            pending.sent,
            pending.token,
            PollRequest {
                finality: PollFinality::FinalAttempt,
            },
        )
        .unwrap();
        let proto::CreateStep::PartialSuccess {
            identifier,
            diagnostic,
            ..
        } = result
        else {
            panic!("final poll must be partial")
        };
        assert_eq!(identifier.as_str(), ARN);
        assert_eq!(
            diagnostic.missing_attributes(),
            ["domain_validation_options"]
        );
        assert_eq!(service.requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn guest_read_back_update_polls_do_not_replay_patch() {
        let (provider, service, request) = fixture(&[false, false, true]);
        let id = request.resource.id;
        let identifier = proto::AppliedIdentifier::new(ARN).unwrap();
        let update = proto::UpdateRequest {
            from: proto::PersistedState {
                id: id.clone(),
                identifier: Some(ARN.to_string()),
                exists: true,
                attributes: request
                    .resource
                    .attributes
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            },
            patch: proto::UpdatePatch {
                ops: vec![proto::PatchOp::Add {
                    key: "tags".to_string(),
                    value: proto::Value::Map(BTreeMap::from([(
                        "Env".to_string(),
                        proto::Value::String("test".to_string()),
                    )])),
                }],
            },
        };
        let SdkUpdateOutcome::Pending(pending) =
            provider.update(&id, &identifier, update.clone()).unwrap()
        else {
            panic!("expected read-back")
        };
        assert_eq!(pending.phase(), PendingPhase::ReadBack);
        let SdkUpdateOutcome::Pending(pending) = provider
            .poll_update(
                &id,
                &identifier,
                update.clone(),
                pending,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .unwrap()
        else {
            panic!("expected continuing read-back")
        };
        assert_eq!(pending.phase(), PendingPhase::ReadBack);
        let result = provider
            .poll_update(
                &id,
                &identifier,
                update,
                pending,
                PollRequest {
                    finality: PollFinality::FinalAttempt,
                },
            )
            .unwrap();
        assert!(matches!(result, SdkUpdateOutcome::Done(_)));
        assert_eq!(service.requests.load(Ordering::Relaxed), 0);
        assert_eq!(service.mutations.load(Ordering::Relaxed), 1);
        assert_eq!(service.describes.load(Ordering::Relaxed), 3);
    }
    #[test]
    fn guest_final_partial_after_poll_keeps_request_attributes() {
        for resume in [false, true] {
            let (provider, service, request) = fixture(&[false]);
            let id = request.resource.id.clone();
            let proto::CreateStep::Pending(pending) =
                dispatch_create(&provider, &id, request.clone()).unwrap()
            else {
                panic!("expected pending")
            };
            let proto::CreateStep::Pending(pending) = dispatch_poll_create(
                &provider,
                &id,
                request.clone(),
                pending.progress,
                pending.sent,
                pending.token,
                PollRequest {
                    finality: PollFinality::Continue,
                },
            )
            .unwrap() else {
                panic!("expected continuing read-back")
            };
            service.absent.store(true, Ordering::Relaxed);
            let attributes = if resume {
                let proto::ResumeStep::PartialSuccess { attributes, .. } = dispatch_resume_create(
                    &provider,
                    &id,
                    request,
                    pending.progress,
                    pending.token,
                    PollRequest {
                        finality: PollFinality::FinalAttempt,
                    },
                )
                .unwrap() else {
                    panic!("expected terminal partial resume")
                };
                attributes
            } else {
                let proto::CreateStep::PartialSuccess { attributes, .. } = dispatch_poll_create(
                    &provider,
                    &id,
                    request,
                    pending.progress,
                    pending.sent,
                    pending.token,
                    PollRequest {
                        finality: PollFinality::FinalAttempt,
                    },
                )
                .unwrap() else {
                    panic!("expected terminal partial poll")
                };
                attributes
            };
            assert_eq!(
                attributes.get("domain_name"),
                Some(&proto::Value::String("example.com".into()))
            );
            assert_eq!(service.describes.load(Ordering::Relaxed), 3);
            assert_eq!(service.requests.load(Ordering::Relaxed), 1);
        }
    }
}
