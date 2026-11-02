//! Native driver and executor coverage using an in-process ACM service.
#![cfg(not(target_arch = "wasm32"))]
#![recursion_limit = "256"]

use std::collections::HashMap;
use std::future::ready;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use carina_core::binding_index::{BindingValueSource, ResolvedBindings};
use carina_core::data_source_value::ExecutableDataSourceValues;
use carina_core::effect::{Effect, PlanOp};
use carina_core::executor::ProviderCheckInputs;
use carina_core::executor::{
    self, Applied, ExecutionCheckpointSink, ExecutionEvent, ExecutionObserver, ExecutionOutcome,
    ExecutionResult, LiveExecutionInput,
};
use carina_core::plan::Plan;
use carina_core::provider::{
    AppliedIdentifier, BoxFuture, CreateAttempt, CreateFailure, CreateOutcome, CreateRequest,
    DeleteRequest, Provider, ProviderReadyDataSource, ProviderResult, ReadBackEnd, ReadOutcome,
    ReadRequest, ResumeCreateRequest, ResumeFailure, ResumeOutcome, Sent, UpdateOutcome,
    UpdateRequest,
};
use carina_core::resource::{
    ConcreteValue, Current, ResolvedResourceId, Resource, ResourceId, State, Value,
    absolute_binding_address_for_test,
};
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, SchemaRegistry};
use carina_core::shutdown::ShutdownToken;
use carina_provider_aws::{AwsNormalizer, AwsProvider, AwsProviderFactory, BoundedSdkConfig};
use carina_provider_protocol::types::{OperationConfig, ReadBackBudget};
use winterbaume_core::MockAws;

#[path = "common/acm_read_back.rs"]
mod acm_read_back;
use acm_read_back::{ARN, AcmService};

/// Only the dependent sink is synthetic. ACM operations and polling run through
/// the production AWS provider; the executor prepares and resolves every request.
struct FixtureProvider {
    aws: AwsProvider,
    sleeps: Mutex<Vec<Duration>>,
    ends: Mutex<Vec<ReadBackEnd>>,
    sinks: Mutex<HashMap<String, Value>>,
    suspend_create: bool,
}
impl Provider for FixtureProvider {
    fn name(&self) -> &str {
        "aws"
    }
    fn read(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        request: ReadRequest,
    ) -> BoxFuture<'_, ProviderResult<ReadOutcome>> {
        self.aws.read(id, identifier, request)
    }
    fn read_data_source(
        &self,
        resource: &ProviderReadyDataSource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        self.aws.read_data_source(resource)
    }
    fn create(
        &self,
        id: &ResourceId,
        request: CreateRequest,
        attempt: CreateAttempt,
    ) -> BoxFuture<'_, Result<CreateOutcome, CreateFailure>> {
        let id = id.clone();
        Box::pin(async move {
            if self.suspend_create {
                let token = serde_json::json!({"phase":"read_back", "identifier":ARN, "resource_type":"acm.Certificate"});
                let pending = attempt
                    .issued(
                        AppliedIdentifier::new(ARN).unwrap(),
                        Sent::AllRequestAttributes,
                    )
                    .pending_read_back(
                        carina_core::provider::PendingToken::new(token.to_string()),
                        5,
                    );
                assert_eq!(
                    pending.phase(),
                    carina_provider_protocol::PendingPhase::ReadBack
                );
                let carina_core::provider::Poll::Issued(issued) = pending.into_poll() else {
                    panic!("read-back must have an identifier")
                };
                return Err(carina_provider_aws::AwsIssuedCreate::preserve(
                    issued,
                    serde_json::from_value(token).unwrap(),
                    carina_core::provider::ProviderError::api_error_caused_by(
                        "temporary read-back failure",
                        std::io::Error::other("connection reset"),
                    ),
                ));
            }
            if id.resource_type.as_str() == "test.Dependent" {
                let attrs = request.resource().as_resource().resolved_attributes();
                self.sinks
                    .lock()
                    .unwrap()
                    .insert(id.identity().unwrap().to_string(), attrs["value"].clone());
                return Ok(attempt
                    .issued(
                        AppliedIdentifier::new(format!("sink-{}", id.identity().unwrap())).unwrap(),
                        Sent::AllRequestAttributes,
                    )
                    .succeeded(attrs));
            }
            let outcome = Box::pin(self.aws.create_with_poll_sleep_for_test(
                request.resource().as_resource(),
                attempt,
                |duration| {
                    self.sleeps.lock().unwrap().push(duration);
                    ready(())
                },
            ))
            .await?;
            self.ends.lock().unwrap().push(outcome.read_back_end());
            Ok(outcome)
        })
    }
    fn resume_create(
        &self,
        request: ResumeCreateRequest,
    ) -> BoxFuture<'_, Result<ResumeOutcome, ResumeFailure>> {
        Box::pin(
            self.aws
                .resume_with_poll_sleep_for_test(request, |duration| {
                    self.sleeps.lock().unwrap().push(duration);
                    ready(())
                }),
        )
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
            self.aws
                .update_with_poll_sleep_for_test(
                    &id,
                    &identifier,
                    request.from(),
                    request.patch(),
                    |duration| {
                        self.sleeps.lock().unwrap().push(duration);
                        ready(())
                    },
                )
                .await
        })
    }
    fn delete(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        request: DeleteRequest,
    ) -> BoxFuture<'_, ProviderResult<()>> {
        self.aws.delete(id, identifier, request)
    }
    fn required_permissions(&self, _id: &ResourceId, _op: PlanOp) -> ProviderResult<Vec<String>> {
        Ok(vec![])
    }
}

#[derive(Default)]
struct Observer(Mutex<Vec<String>>);
impl ExecutionObserver for Observer {
    fn on_event(&self, event: &ExecutionEvent<'_>) {
        if let ExecutionEvent::EffectFailed { failure, .. } = event {
            self.0.lock().unwrap().push(failure.to_string());
        }
    }
}

async fn fixture(populated: &[bool], read_back_seconds: u64) -> (FixtureProvider, Arc<AcmService>) {
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
    let config = BoundedSdkConfig::for_test(mock.sdk_config("us-east-1").await);
    let aws = AwsProvider::from_sdk_config("us-east-1".to_string(), vec![], vec![], &config)
        .with_operation_config_for_test(
            "acm.Certificate",
            OperationConfig {
                read_back: ReadBackBudget::from_secs(read_back_seconds),
                ..Default::default()
            },
        );
    (
        FixtureProvider {
            aws,
            sleeps: Mutex::new(vec![]),
            ends: Mutex::new(vec![]),
            sinks: Mutex::new(HashMap::new()),
            suspend_create: false,
        },
        service,
    )
}
fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}
fn certificate() -> Resource {
    let mut resource = Resource::with_provider("aws", "acm.Certificate", "cert", None);
    resource.binding = Some(absolute_binding_address_for_test("cert"));
    resource.set_attr("domain_name", string("example.com"));
    resource.set_attr(
        "validation_method",
        Value::Concrete(ConcreteValue::enum_identifier(
            "aws.acm.Certificate.ValidationMethod.dns",
        )),
    );
    resource
}
fn dependent(name: &'static str, attribute: &str) -> Resource {
    let mut resource = Resource::with_provider("aws", "test.Dependent", name, None);
    resource.binding = Some(absolute_binding_address_for_test(name));
    resource
        .dependencies
        .value_bindings
        .insert(absolute_binding_address_for_test("cert"));
    let reference = if attribute == "domain_name" {
        Value::resource_ref("cert", attribute, vec![]).resolve_root_for_test()
    } else {
        use carina_core::resource::{AccessPath, DeferredValue, PathSegment, Subscript};
        Value::Deferred(DeferredValue::ResourceRef {
            path: AccessPath::with_segments(
                "cert",
                "domain_validation_options",
                vec![
                    PathSegment::Subscript {
                        index: Subscript::Int { index: 0 },
                    },
                    PathSegment::Field {
                        name: "resource_record".to_string(),
                    },
                    PathSegment::Field {
                        name: "name".to_string(),
                    },
                ],
            )
            .resolve_root_for_test(),
        })
    };
    resource.set_attr("value", reference);
    resource
}
fn schemas() -> SchemaRegistry {
    let mut registry = carina_provider_aws::schemas::schema_registry();
    registry.insert(
        "aws",
        ResourceSchema::new("test.Dependent")
            .attribute(AttributeSchema::required("value", AttributeType::string()).build())
            .validate()
            .unwrap(),
    );
    registry
}
async fn execute(
    provider: &FixtureProvider,
    resources: Vec<Resource>,
    bindings: ResolvedBindings,
    current: HashMap<ResourceId, Current>,
    observer: &Observer,
) -> ExecutionResult {
    let mut plan = Plan::new();
    let schemas = schemas();
    // A refreshed certificate can be outside this plan while its dependents run.
    let mut graph_resources = resources.clone();
    let source = certificate();
    if !graph_resources
        .iter()
        .any(|resource| resource.id == source.id)
    {
        graph_resources.push(source);
    }
    let graph =
        carina_core::resolved_graph::ExpandedBindingIndex::for_test_resources(&graph_resources)
            .expect("test resource bindings should form a valid graph");
    let resource_origins = graph_resources
        .iter()
        .map(|resource| {
            (
                ResolvedResourceId::new(resource.id.clone()),
                executor::UnresolvedResource::from_pre_resolve(resource.clone()),
            )
        })
        .collect();
    for resource in resources {
        let resource = executor::normalized::apply_desired_normalization(
            resource,
            &[],
            &AwsNormalizer,
            &schemas,
        )
        .await
        .expect("normalize fixture values before constructing the plan");
        let resource = graph
            .resolve_resource(resource.as_resource().clone(), &schemas)
            .expect("resolve fixture dependencies before constructing the plan");
        plan.add(Effect::Create(resource));
    }
    let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
        vec![Box::new(AwsProviderFactory)];
    let input = LiveExecutionInput {
        provider_context: carina_core::parser::ProviderContext::clone(
            &carina_core::parser::LoadedProviderContext::empty_for_test(),
        ),
        plan: &plan,
        provider_check_inputs: ProviderCheckInputs::PlanNormalized {
            resource_origins: &resource_origins,
            data_source_origins: &[],
        },
        compositions: &[],
        bindings,
        managed_current: current,
        data_source_values: ExecutableDataSourceValues::empty(),
        normalizer: &AwsNormalizer,
        provider_configs: &[],
        factories: &factories,
        schemas: &schemas,
        parallelism: NonZeroUsize::new(1).unwrap(),
        precompleted_success_count: 0,
        checkpoint_sink: ExecutionCheckpointSink::noop(),
    };
    match Box::pin(executor::execute_live_plan_for_test(
        provider,
        input,
        observer,
        ShutdownToken::running(),
    ))
    .await
    {
        ExecutionOutcome::Completed(result) => result,
        ExecutionOutcome::Cancelled(_) => panic!("test did not cancel"),
    }
}

#[tokio::test]
async fn read_back_completion_reaches_same_apply_dependent() {
    let (provider, service) = fixture(&[false, true], 120).await;
    let dependent = dependent(
        "record",
        "domain_validation_options[0].resource_record.name",
    );
    let observer = Observer::default();
    let result = execute(
        &provider,
        vec![certificate(), dependent],
        ResolvedBindings::default(),
        HashMap::new(),
        &observer,
    )
    .await;
    assert_eq!(
        result.failure_count(),
        0,
        "{:?}",
        observer.0.lock().unwrap()
    );
    assert_eq!(result.partial_count, 0);
    assert_eq!(result.success_count, 2);
    assert_eq!(
        provider.sinks.lock().unwrap()["record"],
        string("_abc.example.com.")
    );
    assert_eq!(service.requests.load(Ordering::Relaxed), 1);
    assert_eq!(service.describes.load(Ordering::Relaxed), 2);
    assert_eq!(*provider.sleeps.lock().unwrap(), [Duration::from_secs(5)]);
}

#[tokio::test]
async fn exhausted_read_back_is_retried_by_next_apply() {
    let (provider, service) = fixture(&[false, false, true], 1).await;
    let source = certificate();
    let id = source.id.clone();
    let observer = Observer::default();
    let result = execute(
        &provider,
        vec![
            source,
            dependent("present", "domain_name"),
            dependent(
                "missing",
                "domain_validation_options[0].resource_record.name",
            ),
        ],
        ResolvedBindings::default(),
        HashMap::new(),
        &observer,
    )
    .await;
    assert_eq!(result.partial_count, 1);
    assert_eq!(result.success_count, 1);
    assert_eq!(
        result.failure_count(),
        1,
        "{:?}",
        observer.0.lock().unwrap()
    );
    let Applied::Complete { state, .. } = &result.applied_states[&id] else {
        panic!("partial create must retain applied state")
    };
    assert_eq!(state.identifier.as_deref(), Some(ARN));
    let marker = state
        .partial_read
        .as_ref()
        .expect("partial marker must persist");
    assert_eq!(
        marker
            .missing_attributes()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["domain_validation_options"]
    );
    let errors = observer.0.lock().unwrap().join("\n");
    assert!(errors.contains("configured budget"), "{errors}");
    assert!(!errors.contains("wait"), "{errors}");
    assert_eq!(*provider.ends.lock().unwrap(), [ReadBackEnd::HostDeadline]);

    // The next apply refreshes the saved identifier before planning dependents.
    let fresh = provider
        .read(&id, &AppliedIdentifier::new(ARN).unwrap(), ReadRequest)
        .await
        .unwrap()
        .into_state();
    assert!(fresh.partial_read.is_none());
    let mut bindings = ResolvedBindings::default();
    bindings.set("cert", fresh.attributes.clone(), BindingValueSource::Local);
    let next_observer = Observer::default();
    let next = execute(
        &provider,
        vec![dependent(
            "missing",
            "domain_validation_options[0].resource_record.name",
        )],
        bindings,
        HashMap::from([(id, Current::from_state(fresh))]),
        &next_observer,
    )
    .await;
    assert_eq!(
        next.success_count,
        1,
        "{:?}",
        next_observer.0.lock().unwrap()
    );
    assert_eq!(next.failure_count(), 0);
    assert_eq!(next.partial_count, 0);
    assert_eq!(
        provider.sinks.lock().unwrap()["missing"],
        string("_abc.example.com.")
    );
    assert_eq!(
        service.requests.load(Ordering::Relaxed),
        1,
        "refresh must not recreate the certificate"
    );
}

#[tokio::test]
async fn native_read_back_uses_schema_budget() {
    let (provider, service) = fixture(&[false, false], 1).await;
    let observer = Observer::default();
    let result = execute(
        &provider,
        vec![certificate()],
        ResolvedBindings::default(),
        HashMap::new(),
        &observer,
    )
    .await;
    assert_eq!(result.partial_count, 1, "{:?}", observer.0.lock().unwrap());
    let sleeps = provider.sleeps.lock().unwrap();
    assert_eq!(sleeps.len(), 1);
    assert!(sleeps[0] > Duration::ZERO && sleeps[0] <= Duration::from_secs(1));
    assert_eq!(service.describes.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn native_final_partial_retains_host_deadline() {
    let (provider, _) = fixture(&[false, false], 1).await;
    let result = execute(
        &provider,
        vec![certificate()],
        ResolvedBindings::default(),
        HashMap::new(),
        &Observer::default(),
    )
    .await;
    assert_eq!(result.partial_count, 1);
    assert_eq!(*provider.ends.lock().unwrap(), [ReadBackEnd::HostDeadline]);
}

#[tokio::test]
async fn native_update_read_back_uses_schema_budget_and_retains_host_deadline() {
    carina_core::validation::deferred_types::with_session_for_test_async(
        &carina_core::parser::LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            let (provider, service) = fixture(&[false, false], 1).await;
            let mut resource = certificate();
            let mut current = resource.resolved_attributes();
            current.insert("validation_method".to_string(), string("DNS"));
            let from = carina_provider_aws::test_existing_state(resource.id.clone(), current);
            resource.set_attr(
                "tags",
                Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::from([(
                    "Env".to_string(),
                    string("test"),
                )]))),
            );
            let bindings = ResolvedBindings::default();
            let module_gate = executor::ModuleConstraintGate::from_live_compositions(&[]);
            let schemas = schemas();
            let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
                vec![Box::new(AwsProviderFactory)];
            let preparation = executor::ProviderPreparationContext::new(
                &bindings,
                &module_gate,
                &[],
                &AwsNormalizer,
                &factories,
                &schemas,
                type_checks,
            );
            let request = executor::prepare_update_request(
                resource.clone(),
                from,
                &["tags".to_string()],
                &preparation,
            )
            .await
            .unwrap();
            let outcome = provider
                .update(&resource.id, &AppliedIdentifier::new(ARN).unwrap(), request)
                .await
                .unwrap();
            assert_eq!(outcome.read_back_end(), ReadBackEnd::HostDeadline);
            assert_eq!(
                outcome.diagnostic().unwrap().missing_attributes(),
                ["domain_validation_options"]
            );
            let sleeps = provider.sleeps.lock().unwrap();
            assert_eq!(sleeps.len(), 1);
            assert!(sleeps[0] <= Duration::from_secs(1));
            assert_eq!(service.mutations.load(Ordering::Relaxed), 1);
            assert_eq!(service.requests.load(Ordering::Relaxed), 0);
            assert_eq!(service.describes.load(Ordering::Relaxed), 2);
        },
    )
    .await
}

async fn checked_create(resource: Resource) -> CreateRequest {
    carina_core::validation::deferred_types::with_session_for_test_async(
        &carina_core::parser::LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            let bindings = ResolvedBindings::default();
            let module_gate = executor::ModuleConstraintGate::from_live_compositions(&[]);
            let schemas = schemas();
            let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
                vec![Box::new(AwsProviderFactory)];
            let preparation = executor::ProviderPreparationContext::new(
                &bindings,
                &module_gate,
                &[],
                &AwsNormalizer,
                &factories,
                &schemas,
                type_checks,
            );
            executor::prepare_create_request(resource, &preparation)
                .await
                .unwrap()
        },
    )
    .await
}

#[tokio::test]
async fn native_read_back_transient_failure_preserves_continuation() {
    use carina_core::provider::CreateFailureKind;
    use carina_plugin_sdk::PhasedToken;
    use carina_provider_aws::AwsCreateToken;

    let (mut provider, service) = fixture(&[false], 120).await;
    provider.suspend_create = true;
    let resource = certificate();
    let request = checked_create(resource.clone()).await;
    let (call, _cancellation) = executor::dispatch_create(&provider, &resource.id, request);
    let CreateFailureKind::InFlight { pending, error } = call.await.unwrap_err().into_kind() else {
        panic!("a transient poll failure must preserve read-back")
    };
    let token: AwsCreateToken =
        serde_json::from_str(pending.token().expose_for_provider_call()).unwrap();
    assert_eq!(
        token.phase(),
        carina_provider_protocol::PendingPhase::ReadBack
    );
    assert_eq!(pending.identifier().unwrap().as_str(), ARN);
    assert!(
        matches!(token, AwsCreateToken::ReadBack(continuation) if continuation.identifier().as_str() == ARN)
    );
    assert!(
        error
            .to_wire()
            .cause
            .as_deref()
            .unwrap()
            .contains("connection reset")
    );
    assert_eq!(service.requests.load(Ordering::Relaxed), 0);
    assert_eq!(service.describes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn native_resume_read_back_uses_schema_budget_without_replaying_create() {
    let (mut first, _) = fixture(&[false], 1).await;
    first.suspend_create = true;
    let resource = certificate();
    let id = resource.id.clone();
    let request = checked_create(resource.clone()).await;
    let (call, _cancellation) = executor::dispatch_create(&first, &id, request);
    let error = call.await.unwrap_err();
    let carina_core::provider::CreateFailureKind::InFlight { pending, .. } = error.into_kind()
    else {
        panic!("expected persisted read-back")
    };
    drop(first);
    let (fresh, service) = fixture(&[false, false], 1).await;
    let resume = ResumeCreateRequest::for_test(pending, checked_create(resource).await).unwrap();
    let outcome = fresh.resume_create(resume).await.unwrap();
    let carina_core::provider::ResumeOutcomeKind::PartialSuccess(partial) = outcome.kind() else {
        panic!("final resumed read must be partial")
    };
    assert_eq!(partial.identifier().as_str(), ARN);
    assert_eq!(
        partial.diagnostic().missing_attributes(),
        ["domain_validation_options"]
    );
    let sleeps = fresh.sleeps.lock().unwrap();
    assert_eq!(sleeps.len(), 1);
    assert!(sleeps[0] > Duration::ZERO && sleeps[0] <= Duration::from_secs(1));
    assert_eq!(service.requests.load(Ordering::Relaxed), 0);
    assert_eq!(service.describes.load(Ordering::Relaxed), 2);
}

#[test]
fn deferred_dvo_references_warn_for_email_and_budget_limited_dns() {
    use carina_core::parser::ParsedFile;
    use carina_core::resource::{
        AccessPath, AuthoredResource, DeferredValue, PathSegment, Subscript,
    };
    use carina_core::validation::deferred_populate::validate_deferred_populate_refs;
    for method in ["EMAIL", "DNS"] {
        let mut cert: AuthoredResource =
            Resource::with_provider("aws", "acm.Certificate", "cert", None);
        cert.binding = Some("cert".into());
        cert.set_attr(
            "validation_method",
            Value::Concrete(ConcreteValue::String(method.into())),
        );
        let mut record: AuthoredResource =
            Resource::with_provider("aws", "route53.RecordSet", "record", None);
        record.binding = Some("record".into());
        record.set_attr(
            "name",
            Value::Deferred(DeferredValue::ResourceRef {
                path: AccessPath::with_segments(
                    "cert",
                    "domain_validation_options",
                    vec![
                        PathSegment::Subscript {
                            index: Subscript::Int { index: 0 },
                        },
                        PathSegment::Field {
                            name: "resource_record".into(),
                        },
                        PathSegment::Field {
                            name: "name".into(),
                        },
                    ],
                ),
            }),
        );
        let mut parsed = ParsedFile::default();
        parsed.resources = vec![cert, record];
        let diagnostics = validate_deferred_populate_refs(
            &parsed,
            &carina_provider_aws::schemas::schema_registry(),
        );
        assert_eq!(diagnostics.len(), 1, "{method}: {diagnostics:?}");
        assert_eq!(diagnostics[0].target_binding.as_str(), "cert");
        assert!(diagnostics[0].unresolved_path.contains("resource_record"));
    }
}
