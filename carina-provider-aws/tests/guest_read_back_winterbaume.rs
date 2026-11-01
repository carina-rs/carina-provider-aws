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
        PollFinality, dispatch_create, dispatch_poll_create, dispatch_resume_create,
    };
    use carina_provider_aws::BoundedSdkConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use winterbaume_core::MockAws;

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
                pending.progress.identifier().cloned(),
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
                pending.progress.identifier().cloned(),
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
                    assert_eq!(diagnostic.missing_attributes, ["domain_validation_options"]);
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
            pending.progress.identifier().cloned(),
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
        assert_eq!(diagnostic.missing_attributes, ["domain_validation_options"]);
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
        assert_eq!(pending.phase, PendingPhase::ReadBack);
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
        assert_eq!(pending.phase, PendingPhase::ReadBack);
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
                pending.progress.identifier().cloned(),
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
                    pending.progress.identifier().cloned(),
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
                    pending.progress.identifier().cloned(),
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
