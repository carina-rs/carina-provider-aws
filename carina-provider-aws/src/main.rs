#[cfg(any(target_arch = "wasm32", test))]
use std::collections::{BTreeMap, HashMap};

#[cfg(any(target_arch = "wasm32", test))]
use carina_plugin_sdk::{
    CarinaProvider, CreateAttempt as SdkCreateAttempt, CreateStep as SdkCreateStep,
    CreateVerdict as SdkCreateVerdict, DeleteOutcome as SdkDeleteOutcome, DesiredResource,
    FoundRead, IssuedCreate as SdkIssuedCreate, NormalizedResource, Pending, PendingDelete,
    Poll as SdkPoll, PollAttempt as SdkPollAttempt, PollRequest, PollVerdict as SdkPollVerdict,
    Resume as SdkResume, ResumeAttempt as SdkResumeAttempt, ResumeStep as SdkResumeStep,
    ResumeVerdict as SdkResumeVerdict, ResumedCreate as SdkResumedCreate,
    UpdateOutcome as SdkUpdateOutcome,
};
#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_protocol::OperationStep;
#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_protocol::types as proto;

#[cfg(any(target_arch = "wasm32", test))]
use carina_core::protocol_convert;
#[cfg(any(target_arch = "wasm32", test))]
use carina_core::provider::{
    AppliedIdentifier as CoreAppliedIdentifier, AppliedState as CoreAppliedState,
    DeleteRequest as CoreDeleteRequest, Observation as CoreObservation, Provider,
    ProviderDirectiveSchema as CoreProviderDirectiveSchema, ProviderError as CoreProviderError,
    ProviderFactory, ReadOutcome as CoreReadOutcome, ReadRequest as CoreReadRequest,
    Sent as CoreSent,
};
#[cfg(any(target_arch = "wasm32", test))]
use carina_core::resource::{
    DataSource as CoreDataSource, Directives as CoreDirectives, Resource as CoreResource,
    Value as CoreValue,
};
#[cfg(any(target_arch = "wasm32", test))]
use carina_core::schema::{
    AssignabilityEdge, ReferenceForms, SchemaKind, SchemaRegistry, ValidatedConfigSchema,
    ValidatedResourceSchema,
};

#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_aws::{
    AwsCreateAttempt, AwsCreateOperation, AwsCreatePoll, AwsCreateResume, AwsCreateToken,
    AwsDeleteToken, AwsIssuedCreate, AwsNormalizer, AwsPollAttempt, AwsProvider, AwsResumeAttempt,
    AwsResumedCreate, AwsUpdateToken, NonEmptyAttributeNames, UpdateResource,
};

#[cfg(any(target_arch = "wasm32", test))]
fn protocol_input_error(
    context: &'static str,
    error: protocol_convert::ProtocolConvertError,
) -> CoreProviderError {
    CoreProviderError::invalid_input_caused_by(context, error)
}

#[cfg(any(target_arch = "wasm32", test))]
fn protocol_output_error(
    context: &'static str,
    error: protocol_convert::ProtocolConvertError,
) -> CoreProviderError {
    CoreProviderError::internal_caused_by(context, error)
}

#[cfg(any(target_arch = "wasm32", test))]
fn wire_directives_to_core(directives: &proto::Directives) -> CoreDirectives {
    CoreDirectives::default()
        .with_force_delete(directives.force_delete)
        .with_create_before_destroy(directives.create_before_destroy)
        .with_prevent_destroy(directives.prevent_destroy)
}

#[cfg(any(target_arch = "wasm32", test))]
fn protocol_resource_to_core(
    resource: &proto::Resource,
) -> Result<CoreResource, protocol_convert::ProtocolConvertError> {
    let mut core = CoreResource::from_id(protocol_convert::protocol_to_resource_id(&resource.id));
    core.attributes = protocol_convert::protocol_to_core_value_map(&resource.attributes)?
        .into_iter()
        .collect();
    core.directives = wire_directives_to_core(&resource.directives);
    Ok(core)
}

#[cfg(any(target_arch = "wasm32", test))]
fn protocol_data_source_to_core(
    resource: &proto::Resource,
) -> Result<CoreDataSource, protocol_convert::ProtocolConvertError> {
    let id = protocol_convert::protocol_to_resource_id(&resource.id);
    let mut core = CoreDataSource::pending_with_provider(
        id.provider.clone(),
        id.resource_type.clone(),
        id.provider_instance.clone(),
    );
    core.id = id;
    core.attributes = protocol_convert::protocol_to_core_value_map(&resource.attributes)?
        .into_iter()
        .collect();
    core.directives = wire_directives_to_core(&resource.directives);
    Ok(core)
}

#[cfg(any(target_arch = "wasm32", test))]
fn read_outcome_to_proto(
    outcome: &CoreReadOutcome,
) -> Result<proto::ReadOutcome, CoreProviderError> {
    protocol_convert::try_read_outcome_to_protocol(outcome).map_err(|error| {
        let id = match outcome {
            CoreReadOutcome::NotFound { id } => id,
            CoreReadOutcome::Found { state } => state.id(),
        };
        protocol_output_error("failed to encode read result", error).for_resource(id.clone())
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn issued_create_attributes_to_proto(
    attributes: &HashMap<String, CoreValue>,
) -> Result<BTreeMap<String, proto::Value>, CoreProviderError> {
    protocol_convert::try_core_to_protocol_value_map(attributes)
        .map_err(|error| protocol_output_error("failed to encode create result", error))
}

#[cfg(any(target_arch = "wasm32", test))]
fn partial_create_attributes_to_proto(
    attributes: &HashMap<String, CoreValue>,
) -> Result<BTreeMap<String, proto::Value>, CoreProviderError> {
    protocol_convert::try_core_to_protocol_value_map(attributes)
        .map_err(|error| protocol_output_error("failed to encode partial create result", error))
}

#[cfg(any(target_arch = "wasm32", test))]
fn resumed_state_to_proto(
    state: &CoreAppliedState,
) -> Result<proto::AppliedState, CoreProviderError> {
    protocol_convert::try_applied_state_to_protocol(state)
        .map_err(|error| protocol_output_error("failed to encode resumed create result", error))
}

#[cfg(any(target_arch = "wasm32", test))]
fn update_outcome_to_proto(
    outcome: &carina_core::provider::UpdateOutcome,
    state_after_patch: proto::PersistedState,
    id: &carina_core::resource::ResourceId,
) -> Result<proto::UpdateOutcome, CoreProviderError> {
    protocol_convert::try_update_outcome_to_protocol(outcome, state_after_patch).map_err(|error| {
        protocol_output_error("failed to encode update result", error).for_resource(id.clone())
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn update_step_to_proto(
    result: carina_core::provider::ProviderResult<
        OperationStep<carina_core::provider::UpdateOutcome, CoreProviderError, AwsUpdateToken>,
    >,
    state_after_patch: proto::PersistedState,
    id: &carina_core::resource::ResourceId,
) -> Result<SdkUpdateOutcome<AwsUpdateToken>, CoreProviderError> {
    match result {
        Ok(OperationStep::Done(outcome)) => {
            update_outcome_to_proto(&outcome, state_after_patch, id).map(SdkUpdateOutcome::Done)
        }
        Ok(OperationStep::Pending {
            pending,
            poll_after_secs,
            ..
        }) => Ok(SdkUpdateOutcome::Pending(Pending::new(
            pending,
            poll_after_secs,
        ))),
        Ok(OperationStep::Verdict(error)) | Err(error) => Err(error),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
struct GuestCreateAttempt(SdkCreateAttempt);
#[cfg(any(target_arch = "wasm32", test))]
struct GuestPollAttempt(SdkPollAttempt);
#[cfg(any(target_arch = "wasm32", test))]
struct GuestIssuedCreate(SdkIssuedCreate);
#[cfg(any(target_arch = "wasm32", test))]
struct GuestPolledIssuedCreate(SdkIssuedCreate);
#[cfg(any(target_arch = "wasm32", test))]
struct GuestResumeAttempt(SdkResumeAttempt);
#[cfg(any(target_arch = "wasm32", test))]
struct GuestResumedCreate(SdkResumedCreate);

#[cfg(any(target_arch = "wasm32", test))]
fn observation_to_proto(
    observation: CoreObservation,
) -> proto::Observation<proto::ReadOutcome, proto::ProviderError> {
    match observation {
        CoreObservation::Read(outcome) => match read_outcome_to_proto(&outcome) {
            Ok(outcome) => proto::Observation::Read(outcome),
            Err(error) => proto::Observation::ReadFailed(error.to_wire()),
        },
        CoreObservation::ReadFailed(error) => proto::Observation::ReadFailed(error.to_wire()),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsCreateAttempt for GuestCreateAttempt {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkCreateVerdict<AwsCreateToken>;
    type Issued = GuestIssuedCreate;

    fn not_created(self, error: CoreProviderError) -> Self::Verdict {
        self.0.not_created(error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier, sent: CoreSent) -> Self::Issued {
        GuestIssuedCreate(self.0.issued(identifier, sent))
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        sent: CoreSent,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, sent, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsPollAttempt for GuestPollAttempt {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkPollVerdict<AwsCreateToken>;
    type Issued = GuestPolledIssuedCreate;

    fn not_created(self, error: CoreProviderError) -> Self::Verdict {
        self.0.not_created(error.to_wire())
    }

    fn preserve(self, token: AwsCreateOperation, error: CoreProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.0.preserve(token, poll_after_secs, error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier) -> Self::Issued {
        GuestPolledIssuedCreate(self.0.issued(identifier))
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsIssuedCreate for GuestPolledIssuedCreate {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkPollVerdict<AwsCreateToken>;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn succeeded(
        self,
        attributes: HashMap<String, CoreValue>,
    ) -> Result<Self::Step, Self::Verdict> {
        match issued_create_attributes_to_proto(&attributes) {
            Ok(attributes) => Ok(self.0.succeeded(attributes)),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error)
                    .into())
            }
        }
    }

    fn partial(
        self,
        attributes: HashMap<String, CoreValue>,
        reason: CoreProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        match partial_create_attributes_to_proto(&attributes) {
            Ok(attributes) => Ok(self.0.partial(
                attributes,
                proto::PartialReadDiagnostic {
                    reason: reason.to_string(),
                    missing_attributes: missing_attributes.into(),
                },
            )),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error)
                    .into())
            }
        }
    }

    fn identifier_reported(
        self,
        observation: CoreObservation,
        error: CoreProviderError,
    ) -> Self::Verdict {
        self.0
            .identifier_reported(observation_to_proto(observation), error.to_wire())
            .into()
    }

    fn no_resource(self, error: CoreProviderError) -> Self::Verdict {
        self.0.no_resource(error.to_wire()).into()
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }

    fn preserve(self, token: AwsCreateToken, error: CoreProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.0
            .preserve(token, poll_after_secs, error.to_wire())
            .into()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsIssuedCreate for GuestIssuedCreate {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkCreateVerdict<AwsCreateToken>;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn succeeded(
        self,
        attributes: HashMap<String, CoreValue>,
    ) -> Result<Self::Step, Self::Verdict> {
        match issued_create_attributes_to_proto(&attributes) {
            Ok(attributes) => Ok(self.0.succeeded(attributes)),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error))
            }
        }
    }

    fn partial(
        self,
        attributes: HashMap<String, CoreValue>,
        reason: CoreProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        match partial_create_attributes_to_proto(&attributes) {
            Ok(attributes) => Ok(self.0.partial(
                attributes,
                proto::PartialReadDiagnostic {
                    reason: reason.to_string(),
                    missing_attributes: missing_attributes.into(),
                },
            )),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error))
            }
        }
    }

    fn identifier_reported(
        self,
        observation: CoreObservation,
        error: CoreProviderError,
    ) -> Self::Verdict {
        self.0
            .identifier_reported(observation_to_proto(observation), error.to_wire())
    }

    fn no_resource(self, error: CoreProviderError) -> Self::Verdict {
        self.0.no_resource(error.to_wire())
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }

    fn preserve(self, token: AwsCreateToken, error: CoreProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.0.preserve(token, poll_after_secs, error.to_wire())
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsResumeAttempt for GuestResumeAttempt {
    type Step = SdkResumeStep<AwsCreateToken>;
    type Verdict = SdkResumeVerdict<AwsCreateToken>;
    type Resumed = GuestResumedCreate;

    fn not_created(self, error: CoreProviderError) -> Self::Verdict {
        self.0.not_created(error.to_wire())
    }

    fn preserve(self, token: AwsCreateOperation, error: CoreProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.0.preserve(token, poll_after_secs, error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier) -> Self::Resumed {
        GuestResumedCreate(self.0.issued(identifier))
    }

    fn pending(
        self,
        token: AwsCreateOperation,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsResumedCreate for GuestResumedCreate {
    type Step = SdkResumeStep<AwsCreateToken>;
    type Verdict = SdkResumeVerdict<AwsCreateToken>;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn completed(self, state: CoreAppliedState) -> Result<Self::Step, Self::Verdict> {
        match resumed_state_to_proto(&state) {
            Ok(state) => Ok(self.0.completed(FoundRead::from_applied_state(state))),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error))
            }
        }
    }

    fn partial(
        self,
        attributes: HashMap<String, CoreValue>,
        reason: CoreProviderError,
        missing_attributes: NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        match partial_create_attributes_to_proto(&attributes) {
            Ok(attributes) => Ok(self.0.partial(
                attributes,
                proto::PartialReadDiagnostic {
                    reason: reason.to_string(),
                    missing_attributes: missing_attributes.into(),
                },
            )),
            Err(error) => {
                let error = error.to_wire();
                Err(self
                    .0
                    .identifier_reported(proto::Observation::ReadFailed(error.clone()), error))
            }
        }
    }

    fn identifier_reported(
        self,
        observation: CoreObservation,
        error: CoreProviderError,
    ) -> Self::Verdict {
        self.0
            .identifier_reported(observation_to_proto(observation), error.to_wire())
    }

    fn no_resource(self, error: CoreProviderError) -> Self::Verdict {
        self.0.no_resource(error.to_wire())
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }

    fn preserve(self, token: AwsCreateToken, error: CoreProviderError) -> Self::Verdict {
        let poll_after_secs = token.poll_after_secs();
        self.0.preserve(token, poll_after_secs, error.to_wire())
    }
}

#[cfg(any(target_arch = "wasm32", test))]
struct AwsProcessProvider {
    runtime: tokio::runtime::Runtime,
    provider: Option<AwsProvider>,
    normalizer: AwsNormalizer,
}

#[cfg(any(target_arch = "wasm32", test))]
impl Default for AwsProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsProcessProvider {
    fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        #[cfg(target_arch = "wasm32")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("Failed to create tokio runtime");
        Self {
            runtime,
            provider: None,
            normalizer: AwsNormalizer,
        }
    }

    fn convert_error(e: CoreProviderError) -> proto::ProviderError {
        e.to_wire()
    }

    fn provider(&self) -> &AwsProvider {
        self.provider
            .as_ref()
            .expect("Provider not initialized; call initialize() first")
    }

    fn schema_registry() -> SchemaRegistry {
        carina_provider_aws::schemas::schema_registry()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl CarinaProvider for AwsProcessProvider {
    type CreateToken = AwsCreateToken;
    type UpdateToken = AwsUpdateToken;
    type DeleteToken = AwsDeleteToken;

    fn info(&self) -> proto::ProviderInfo {
        proto::ProviderInfo {
            name: "aws".into(),
            display_name: "AWS provider".into(),
            capabilities: self.capabilities(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        carina_provider_aws::schemas::all_schemas()
    }

    fn reference_forms(&self) -> ReferenceForms {
        ReferenceForms::new()
    }

    fn assignability_edges(&self) -> Vec<AssignabilityEdge> {
        carina_provider_aws::schemas::assignability_edges()
    }

    fn resolve_references(
        &self,
        requests: Vec<proto::ReferenceRequest>,
    ) -> Result<Vec<Result<proto::ReferenceOutcome, proto::ReferenceError>>, proto::ProviderError>
    {
        Ok(self
            .runtime
            .block_on(self.provider().resolve_references(requests)))
    }

    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        carina_provider_aws::AwsProviderFactory.provider_config_attribute_types()
    }

    fn validate_config(&self, attrs: &BTreeMap<String, proto::Value>) -> Result<(), String> {
        // Region format/value validation is handled by the host via
        // `provider_config_attribute_types`. The provider-specific check
        // we add here is the cross-account guardrail: when `assume_role`
        // is present, its `role_arn` must target an account that is
        // listed in `allowed_account_ids` (when that list is configured).
        use carina_provider_aws::services::sts::account_guard::extract_string_list;
        use carina_provider_aws::services::sts::assume_role::{
            check_cross_account, extract_assume_role,
        };
        let core_attrs = protocol_convert::protocol_to_core_value_map(attrs).map_err(|error| {
            protocol_input_error("failed to decode provider configuration", error).to_string()
        })?;
        let assume_role = extract_assume_role(core_attrs.get("assume_role"))?;
        if let Some(ar) = &assume_role {
            let allowed = extract_string_list(core_attrs.get("allowed_account_ids"));
            check_cross_account(&ar.role_arn, &allowed)?;
        }
        Ok(())
    }

    fn initialize(
        &mut self,
        attrs: &BTreeMap<String, proto::Value>,
    ) -> Result<proto::ReferenceScope, String> {
        use carina_provider_aws::services::sts::account_guard::extract_string_list;
        use carina_provider_aws::services::sts::assume_role::extract_assume_role;
        let core_attrs: indexmap::IndexMap<_, _> =
            protocol_convert::protocol_to_core_value_map(attrs)
                .map_err(|error| {
                    protocol_input_error("failed to decode provider configuration", error)
                        .to_string()
                })?
                .into_iter()
                .collect();
        let region = carina_core::utils::extract_region_from_attrs(&core_attrs, "ap-northeast-1");
        let allowed = extract_string_list(core_attrs.get("allowed_account_ids"));
        let forbidden = extract_string_list(core_attrs.get("forbidden_account_ids"));
        let assume_role = extract_assume_role(core_attrs.get("assume_role"))?;
        let provider = self.runtime.block_on(AwsProvider::new_with_account_guard(
            &region,
            allowed,
            forbidden,
            assume_role,
        ));
        // Run the account guard before we accept this provider — fails
        // fast (before any read/plan/apply) when the credentials in use
        // point at the wrong AWS account.
        self.runtime.block_on(provider.verify_account_id())?;
        self.provider = Some(provider);
        // No ReferenceForm table is declared in this change. Reporting an
        // available scope would therefore add no capability, and discovering
        // the account must not introduce an otherwise unnecessary STS call.
        Ok(proto::ReferenceScope::Unavailable)
    }

    fn config_completions(&self) -> HashMap<String, Vec<proto::CompletionValue>> {
        HashMap::from([(
            "region".to_string(),
            carina_aws_types::region_completions("aws")
                .into_iter()
                .map(|c| proto::CompletionValue {
                    value: c.value,
                    description: c.description.to_string(),
                })
                .collect(),
        )])
    }

    fn identity_attributes(&self) -> Vec<String> {
        vec!["region".to_string()]
    }

    fn required_permissions(
        &self,
        _id: &proto::ResourceId,
        _op: carina_plugin_sdk::PlanOp,
    ) -> Result<Vec<String>, proto::ProviderError> {
        Ok(Vec::new())
    }

    fn read(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        _request: proto::ReadRequest,
    ) -> Result<proto::ReadOutcome, proto::ProviderError> {
        let core_id = protocol_convert::protocol_to_resource_id(id);
        let result =
            self.runtime
                .block_on(self.provider().read(&core_id, identifier, CoreReadRequest));
        match result {
            Ok(outcome) => read_outcome_to_proto(&outcome).map_err(Self::convert_error),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn read_data_source(
        &self,
        resource: &proto::Resource,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_data_source = protocol_data_source_to_core(resource).map_err(|error| {
            Self::convert_error(
                protocol_input_error("failed to decode data source", error)
                    .for_resource(protocol_convert::protocol_to_resource_id(&resource.id)),
            )
        })?;
        let result = self
            .runtime
            .block_on(self.provider().read_data_source_resource(&core_data_source));
        match result {
            Ok(state) => protocol_convert::try_state_to_protocol(&state).map_err(|error| {
                Self::convert_error(
                    protocol_output_error("failed to encode data-source read result", error)
                        .for_resource(core_data_source.id.clone()),
                )
            }),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        attempt: SdkCreateAttempt,
    ) -> Result<SdkCreateStep<Self::CreateToken>, SdkCreateVerdict<Self::CreateToken>> {
        let core_resource = match protocol_resource_to_core(&request.resource) {
            Ok(resource) => resource,
            Err(error) => {
                let error = protocol_input_error("failed to decode create request", error)
                    .for_resource(protocol_convert::protocol_to_resource_id(
                        &request.resource.id,
                    ));
                return Err(attempt.not_created(Self::convert_error(error)));
            }
        };
        let schemas = Self::schema_registry();
        let core_resource = match self.runtime.block_on(
            carina_core::executor::normalized::apply_desired_normalization(
                core_resource,
                &[],
                &self.normalizer,
                &schemas,
            ),
        ) {
            Ok(resource) => resource,
            Err(error) => return Err(attempt.not_created(Self::convert_error(error))),
        };
        self.runtime.block_on(
            self.provider()
                .create_step(&core_resource, GuestCreateAttempt(attempt)),
        )
    }

    fn poll_create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        poll: SdkPoll<Self::CreateToken>,
        poll_request: PollRequest,
    ) -> Result<SdkCreateStep<Self::CreateToken>, SdkPollVerdict<Self::CreateToken>> {
        let core_resource = match protocol_resource_to_core(&request.resource) {
            Ok(resource) => resource,
            Err(error) => {
                let error = Self::convert_error(
                    protocol_input_error("failed to decode create poll request", error)
                        .for_resource(protocol_convert::protocol_to_resource_id(
                            &request.resource.id,
                        )),
                );
                return match poll {
                    SdkPoll::Unissued(attempt, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(attempt.preserve(token, poll_after_secs, error))
                    }
                    SdkPoll::Issued(issued, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(issued.preserve(token, poll_after_secs, error).into())
                    }
                };
            }
        };
        let schemas = Self::schema_registry();
        let core_resource = match self.runtime.block_on(
            carina_core::executor::normalized::apply_desired_normalization(
                core_resource,
                &[],
                &self.normalizer,
                &schemas,
            ),
        ) {
            Ok(resource) => resource,
            Err(error) => {
                let error = Self::convert_error(error);
                return match poll {
                    SdkPoll::Unissued(attempt, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(attempt.preserve(token, poll_after_secs, error))
                    }
                    SdkPoll::Issued(issued, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(issued.preserve(token, poll_after_secs, error).into())
                    }
                };
            }
        };
        let poll = match poll {
            SdkPoll::Unissued(attempt, token) => {
                AwsCreatePoll::Unissued(GuestPollAttempt(attempt), token)
            }
            SdkPoll::Issued(issued, token) => {
                AwsCreatePoll::Issued(GuestPolledIssuedCreate(issued), token)
            }
        };
        self.runtime.block_on(self.provider().poll_create_step(
            core_resource.as_resource(),
            poll,
            poll_request,
        ))
    }

    fn resume_create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        resume: SdkResume<Self::CreateToken>,
        poll_request: PollRequest,
    ) -> Result<SdkResumeStep<Self::CreateToken>, SdkResumeVerdict<Self::CreateToken>> {
        let core_resource = match protocol_resource_to_core(&request.resource) {
            Ok(resource) => resource,
            Err(error) => {
                let error = Self::convert_error(
                    protocol_input_error("failed to decode create resume request", error)
                        .for_resource(protocol_convert::protocol_to_resource_id(
                            &request.resource.id,
                        )),
                );
                return match resume {
                    SdkResume::Unissued(attempt, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(attempt.preserve(token, poll_after_secs, error))
                    }
                    SdkResume::Issued(resumed, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(resumed.preserve(token, poll_after_secs, error))
                    }
                };
            }
        };
        let schemas = Self::schema_registry();
        let core_resource = match self.runtime.block_on(
            carina_core::executor::normalized::apply_desired_normalization(
                core_resource,
                &[],
                &self.normalizer,
                &schemas,
            ),
        ) {
            Ok(resource) => resource,
            Err(error) => {
                let error = Self::convert_error(error);
                return match resume {
                    SdkResume::Unissued(attempt, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(attempt.preserve(token, poll_after_secs, error))
                    }
                    SdkResume::Issued(resumed, token) => {
                        let poll_after_secs = token.poll_after_secs();
                        Err(resumed.preserve(token, poll_after_secs, error))
                    }
                };
            }
        };
        let resume = match resume {
            SdkResume::Unissued(attempt, token) => {
                AwsCreateResume::Unissued(GuestResumeAttempt(attempt), token)
            }
            SdkResume::Issued(resumed, token) => {
                AwsCreateResume::Issued(GuestResumedCreate(resumed), token)
            }
        };
        self.runtime.block_on(self.provider().resume_create_step(
            core_resource.as_resource(),
            resume,
            poll_request,
        ))
    }

    fn update(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        request: proto::UpdateRequest,
    ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
        let core_id = protocol_convert::protocol_to_resource_id(id);
        let state_after_patch = request.state_after_patch();
        let core_from =
            protocol_convert::protocol_persisted_state_to_core(&request.from).map_err(|error| {
                Self::convert_error(
                    protocol_input_error("failed to decode persisted update state", error)
                        .for_resource(core_id.clone()),
                )
            })?;
        let core_patch =
            protocol_convert::protocol_update_patch_to_core(&request.patch).map_err(|error| {
                Self::convert_error(
                    protocol_input_error("failed to decode update patch", error)
                        .for_resource(core_id.clone()),
                )
            })?;
        let result = self.runtime.block_on(self.provider().update_step(
            &core_id,
            identifier,
            &core_from,
            &core_patch,
        ));
        update_step_to_proto(result, state_after_patch, &core_id).map_err(Self::convert_error)
    }

    fn poll_update(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        request: proto::UpdateRequest,
        pending: Pending<Self::UpdateToken>,
        poll_request: PollRequest,
    ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
        let core_id = protocol_convert::protocol_to_resource_id(id);
        let state_after_patch = request.state_after_patch();
        let state =
            protocol_convert::protocol_persisted_state_to_core(&request.from).map_err(|error| {
                Self::convert_error(
                    protocol_input_error("failed to decode read-back update state", error)
                        .for_resource(core_id.clone()),
                )
            })?;
        let patch =
            protocol_convert::protocol_update_patch_to_core(&request.patch).map_err(|error| {
                Self::convert_error(
                    protocol_input_error("failed to decode read-back update patch", error)
                        .for_resource(core_id.clone()),
                )
            })?;
        let resource = UpdateResource::new(&state, &patch);
        let result = self.runtime.block_on(self.provider().poll_update_step(
            &resource,
            identifier,
            pending.token,
            poll_request,
        ));
        update_step_to_proto(result, state_after_patch, &core_id).map_err(Self::convert_error)
    }

    fn delete(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        request: proto::DeleteRequest,
    ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
        let core_id = protocol_convert::protocol_to_resource_id(id);
        let schemas = Self::schema_registry();
        let Some(schema) = schemas.get("aws", &core_id.resource_type, SchemaKind::Resource) else {
            return Err(Self::convert_error(
                CoreProviderError::internal("missing schema while resolving delete directives")
                    .for_resource(core_id),
            ));
        };
        let directives = wire_directives_to_core(&request.directives);
        let core_request = CoreDeleteRequest::resolve(
            &directives,
            CoreProviderDirectiveSchema::Available(schema.schema().as_schema()),
        );
        let result = self.runtime.block_on(self.provider().delete_step(
            &core_id,
            identifier,
            &core_request,
        ));
        match result {
            Ok(OperationStep::Done(())) => Ok(SdkDeleteOutcome::Deleted),
            Ok(OperationStep::Pending {
                pending,
                poll_after_secs,
                ..
            }) => Ok(SdkDeleteOutcome::Pending(PendingDelete {
                token: pending,
                poll_after_secs,
            })),
            Ok(OperationStep::Verdict(error)) => Err(Self::convert_error(error)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn poll_delete(
        &self,
        id: &proto::ResourceId,
        _identifier: &proto::AppliedIdentifier,
        _request: proto::DeleteRequest,
        pending: PendingDelete<Self::DeleteToken>,
    ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
        let core_id = protocol_convert::protocol_to_resource_id(id);
        let result = self
            .runtime
            .block_on(self.provider().poll_delete_step(&core_id, pending.token));
        match result {
            Ok(OperationStep::Done(())) => Ok(SdkDeleteOutcome::Deleted),
            Ok(OperationStep::Pending {
                pending,
                poll_after_secs,
                ..
            }) => Ok(SdkDeleteOutcome::Pending(PendingDelete {
                token: pending,
                poll_after_secs,
            })),
            Ok(OperationStep::Verdict(error)) => Err(Self::convert_error(error)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn normalize_desired(
        &self,
        resources: Vec<DesiredResource>,
    ) -> Result<Vec<NormalizedResource>, proto::ProviderError> {
        let mut normalized = Vec::with_capacity(resources.len());
        for resource in resources {
            let mut desired = resource.into_normalization()?;
            self.normalizer
                .normalize_desired_view(&mut desired.view())
                .map_err(Self::convert_error)?;
            normalized.push(desired.finish()?);
        }
        Ok(normalized)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "carina-provider-aws no longer has a native provider transport; build the wasm32-wasip2 component"
    );
    std::process::exit(1);
}

#[cfg(target_arch = "wasm32")]
carina_plugin_sdk::export_provider!(
    AwsProcessProvider,
    aws,
    egress: ["**.amazonaws.com", "**.amazonaws.com.cn"],
);

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(test)]
mod tests {

    use super::*;
    use carina_core::resource::ConcreteValue;
    use carina_core::schema::{
        CompiledAttributeType, CustomValidator, RawShape, ResourceValidation,
        TypeIdentity as CoreTypeIdentity,
    };
    use carina_plugin_sdk::types::{TypeIdentity as WireTypeIdentity, ValidatorType};
    use carina_plugin_sdk::{ProviderSchemaCache, dispatch_custom_type_validation};

    #[test]
    fn native_and_wasm_guest_assignability_edges_match() {
        let native = carina_provider_aws::AwsProviderFactory.schema_witness();
        let guest = AwsProcessProvider::new();
        assert_eq!(native.assignability_edges(), guest.assignability_edges());
        assert!(native.assignability_edges().is_empty());

        // export_provider!'s schemas export uses this same cache and serializer.
        let cache = ProviderSchemaCache::from_provider(&guest).expect("guest schema cache");
        let json = carina_plugin_sdk::provider_schema_to_json(cache.witness())
            .expect("guest schema export");
        let exported: proto::ProviderSchemaEnvelope =
            serde_json::from_str(&json).expect("protocol 21 schema envelope");
        let native_wire = protocol_convert::provider_schema_to_protocol(&native)
            .expect("native schema projection");
        assert_eq!(
            exported.assignability_edges,
            native_wire.assignability_edges
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CallbackScalar {
        String,
        Int,
        Float,
    }

    struct CallbackBinding {
        scalar: CallbackScalar,
        callbacks: Vec<CustomValidator>,
    }

    fn wire_identity(identity: &CoreTypeIdentity) -> WireTypeIdentity {
        WireTypeIdentity {
            provider: identity.provider.clone().unwrap_or_default(),
            segments: identity.segments.clone(),
            kind: identity.kind.clone(),
        }
    }

    fn scalar_kind(attribute_type: &CompiledAttributeType) -> CallbackScalar {
        match attribute_type.raw_shape() {
            RawShape::String { .. } | RawShape::Reference { .. } => CallbackScalar::String,
            RawShape::Int { .. } => CallbackScalar::Int,
            RawShape::Float { .. } => CallbackScalar::Float,
            RawShape::Enum { base, .. } => scalar_kind(base),
            shape => panic!("callback identity must refine a scalar, got {shape:?}"),
        }
    }

    fn record_callback(
        bindings: &mut BTreeMap<CoreTypeIdentity, CallbackBinding>,
        identity: &CoreTypeIdentity,
        scalar: CallbackScalar,
        callback: &CustomValidator,
    ) {
        let binding = bindings
            .entry(identity.clone())
            .or_insert_with(|| CallbackBinding {
                scalar,
                callbacks: Vec::new(),
            });
        assert_eq!(
            binding.scalar, scalar,
            "identity {identity} has conflicting scalar kinds in validated witnesses"
        );
        binding.callbacks.push(callback.clone());
    }

    fn collect_callbacks(
        attribute_type: &CompiledAttributeType,
        bindings: &mut BTreeMap<CoreTypeIdentity, CallbackBinding>,
    ) {
        match attribute_type.raw_shape() {
            RawShape::String {
                identity, validate, ..
            }
            | RawShape::Int {
                identity, validate, ..
            }
            | RawShape::Float {
                identity, validate, ..
            } => {
                if let Some(callback) = validate {
                    let identity =
                        identity.expect("validated callback-bearing scalar must carry an identity");
                    record_callback(bindings, identity, scalar_kind(attribute_type), callback);
                }
            }
            RawShape::Enum {
                identity, validate, ..
            } => {
                if let Some(callback) = validate {
                    record_callback(bindings, identity, scalar_kind(attribute_type), callback);
                }
            }
            RawShape::List { element_type, .. } => collect_callbacks(element_type, bindings),
            RawShape::Map { key, value } => {
                collect_callbacks(key, bindings);
                collect_callbacks(value, bindings);
            }
            RawShape::Struct { fields, .. } => {
                for field in fields {
                    collect_callbacks(field.field_type(), bindings);
                }
            }
            RawShape::Union(members) => {
                for member in members {
                    collect_callbacks(member, bindings);
                }
            }
            RawShape::UnloadedSchemaType(unloaded) => panic!(
                "callback collection requires a loaded schema type for {}",
                unloaded.identity(),
            ),
            RawShape::Bool | RawShape::Duration | RawShape::Ref(_) | RawShape::Reference { .. } => {
            }
        }
    }

    fn bridge_probe(scalar: CallbackScalar, text: &str) -> CoreValue {
        match scalar {
            CallbackScalar::String => {
                CoreValue::Concrete(ConcreteValue::String(text.to_string().into()))
            }
            CallbackScalar::Int => CoreValue::Concrete(ConcreteValue::Int(
                text.parse().expect("integer bridge probe"),
            )),
            CallbackScalar::Float => CoreValue::Concrete(ConcreteValue::Float(
                text.parse().expect("float bridge probe"),
            )),
        }
    }

    fn probe_texts(scalar: CallbackScalar) -> &'static [&'static str] {
        match scalar {
            CallbackScalar::String => &["", "__carina_sdk_bridge_probe__", "not-an-arn", "0"],
            CallbackScalar::Int => &["0", "-1", "1", "9223372036854775807"],
            CallbackScalar::Float => &["0", "-1", "1", "1.5"],
        }
    }

    fn valid_bridge_sample(identity: &CoreTypeIdentity) -> &'static str {
        if let Some(aws_type) = carina_aws_types::AwsType::ALL
            .iter()
            .copied()
            .find(|aws_type| aws_type.identity() == *identity)
        {
            assert!(
                aws_type.has_validator(),
                "callback-bearing identity {identity} must be declared validated"
            );
            return aws_type.valid_sample();
        }

        assert!(
            identity.provider.is_none() && identity.segments.is_empty(),
            "validated identity {identity} is neither an AWS catalog row nor a core built-in"
        );
        match identity.kind.as_str() {
            "Email" => "user@example.com",
            "Ipv4Address" => "192.0.2.1",
            "Ipv4Cidr" => "10.0.0.0/16",
            "Ipv6Cidr" => "2001:db8::/32",
            kind => panic!("core built-in validator {kind} needs a bridge sample"),
        }
    }

    fn declared_validators(
        schema: &ValidatedResourceSchema,
    ) -> &[carina_provider_protocol::types::ValidatorType] {
        match schema.parts().validation {
            ResourceValidation::Declared(validators) => validators,
            ResourceValidation::None => &[],
        }
    }

    fn non_finite_attributes() -> HashMap<String, CoreValue> {
        HashMap::from([(
            "value".to_string(),
            CoreValue::Concrete(ConcreteValue::Float(f64::NAN)),
        )])
    }

    fn non_finite_state() -> CoreAppliedState {
        carina_provider_aws::non_finite_applied_state_for_protocol_test()
    }

    fn assert_typed_conversion_error(error: CoreProviderError, message: &str) {
        let error = error.to_wire();
        assert_eq!(error.kind, proto::ProviderErrorKind::Internal);
        assert_eq!(error.message, message);
        assert!(
            error
                .cause
                .as_deref()
                .is_some_and(|cause| cause.contains("non-finite float")),
            "typed protocol-conversion cause must be retained: {error:?}"
        );
    }

    #[test]
    fn non_finite_read_output_surfaces_as_a_provider_error() {
        let error = read_outcome_to_proto(&CoreReadOutcome::Found {
            state: non_finite_state(),
        })
        .expect_err("non-finite provider output must be rejected without panicking");
        assert_typed_conversion_error(error, "failed to encode read result");
    }

    #[test]
    fn non_finite_issued_create_output_surfaces_as_a_provider_error() {
        let error = issued_create_attributes_to_proto(&non_finite_attributes());
        assert_typed_conversion_error(
            error.expect_err("non-finite issued-create output must be rejected"),
            "failed to encode create result",
        );
    }

    #[test]
    fn non_finite_partial_create_output_surfaces_as_a_provider_error() {
        let error = partial_create_attributes_to_proto(&non_finite_attributes());
        assert_typed_conversion_error(
            error.expect_err("non-finite partial-create output must be rejected"),
            "failed to encode partial create result",
        );
    }

    #[test]
    fn non_finite_resumed_create_output_surfaces_as_a_provider_error() {
        let error = resumed_state_to_proto(&non_finite_state())
            .expect_err("non-finite resumed-create output must be rejected");
        assert_typed_conversion_error(error, "failed to encode resumed create result");
    }

    #[test]
    fn non_finite_update_output_surfaces_as_a_provider_error() {
        let state = non_finite_state();
        let id = state.id().clone();
        let outcome = carina_core::provider::UpdateOutcome::success(state);
        let state_after_patch = proto::PersistedState {
            id: protocol_convert::resource_id_to_protocol(&id),
            identifier: None,
            attributes: BTreeMap::new(),
            exists: true,
        };
        let error = update_outcome_to_proto(&outcome, state_after_patch, &id)
            .expect_err("non-finite update output must be rejected");
        assert_typed_conversion_error(error, "failed to encode update result");
    }

    #[test]
    fn schemas_include_tags_validator_for_tagged_resources() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        let bucket = schemas
            .iter()
            .find(|schema| schema.parts().resource_type == "s3.Bucket")
            .expect("s3.bucket schema should exist");
        assert!(
            declared_validators(bucket).contains(&ValidatorType::TagsKeyValueCheck),
            "s3.bucket should have TagsKeyValueCheck validator"
        );
    }

    #[test]
    fn schemas_exclude_tags_validator_for_untagged_resources() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        let configs = carina_provider_aws::schemas::generated::configs();
        if let Some(untagged) = configs.iter().find(|c| !c.has_tags) {
            let schema = schemas
                .iter()
                .find(|schema| schema.parts().resource_type == untagged.resource_type_name)
                .expect("untagged schema should exist");
            assert!(
                !declared_validators(schema).contains(&ValidatorType::TagsKeyValueCheck),
                "untagged resource should not have TagsKeyValueCheck"
            );
        }
    }

    #[test]
    fn schemas_include_iam_role_and_logs_log_group() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        assert!(
            schemas
                .iter()
                .any(|schema| schema.parts().resource_type == "iam.Role"),
            "iam.Role schema should be registered"
        );
        assert!(
            schemas
                .iter()
                .any(|schema| schema.parts().resource_type == "logs.LogGroup"),
            "logs.LogGroup schema should be registered"
        );
    }

    #[test]
    fn every_callback_bearing_witness_dispatches_through_the_sdk_bridge() {
        let provider = AwsProcessProvider::new();
        let expected_schema_count = carina_provider_aws::schemas::generated::configs().len();
        let cache = ProviderSchemaCache::from_provider(&provider)
            .expect("every AWS resource and config schema must pass v12 admission");

        assert_eq!(cache.schemas().len(), expected_schema_count);
        let mut bindings = BTreeMap::new();
        for schema in cache.schemas() {
            let parts = schema.parts();
            for attribute in parts.attributes.values() {
                collect_callbacks(attribute.attr_type(), &mut bindings);
            }
            for definition in parts.defs.values() {
                collect_callbacks(definition, &mut bindings);
            }
        }
        for attribute_type in cache.config().parts().values() {
            collect_callbacks(attribute_type, &mut bindings);
        }

        assert!(
            !bindings.is_empty(),
            "generated resource and config witnesses must expose callback-bearing identities"
        );
        for (identity, binding) in bindings {
            let sample = valid_bridge_sample(&identity);
            let sample_value = bridge_probe(binding.scalar, sample);
            assert!(
                binding
                    .callbacks
                    .iter()
                    .all(|callback| callback(&sample_value).is_ok()),
                "valid sample {sample:?} must pass every direct callback for {identity}"
            );
            dispatch_custom_type_validation(&cache, &wire_identity(&identity), sample)
                .unwrap_or_else(|error| {
                    panic!(
                        "valid sample {sample:?} must pass the SDK bridge for {identity}: {error}"
                    )
                });

            let mut rejected_by_both = false;
            for text in probe_texts(binding.scalar) {
                let value = bridge_probe(binding.scalar, text);
                let direct = binding
                    .callbacks
                    .iter()
                    .all(|callback| callback(&value).is_ok());
                let bridged =
                    dispatch_custom_type_validation(&cache, &wire_identity(&identity), text)
                        .is_ok();
                assert_eq!(
                    bridged, direct,
                    "SDK dispatch for {identity} diverged from its validated witness callbacks on {text:?}"
                );
                rejected_by_both |= !direct && !bridged;
            }
            assert!(
                rejected_by_both,
                "at least one invalid probe must be rejected directly and through the SDK bridge for {identity}"
            );
        }
    }

    #[test]
    fn iam_role_name_witness_dispatches_through_the_sdk_bridge() {
        let provider = AwsProcessProvider::new();
        let cache = ProviderSchemaCache::from_provider(&provider)
            .expect("AWS schemas must build a dispatch table");
        let row = carina_aws_types::AwsType::IamRoleName;
        let expected = row.identity();
        let role = cache
            .schemas()
            .iter()
            .find(|schema| schema.parts().resource_type == "iam.Role")
            .expect("generated Role schema");
        let RawShape::String {
            identity: Some(identity),
            pattern,
            length,
            validate: Some(_),
            ..
        } = role.parts().attributes["role_name"].attr_type().raw_shape()
        else {
            panic!("role_name must be the shared validated String");
        };
        assert_eq!(identity, &expected);
        assert_eq!(pattern, Some(r"^[A-Za-z0-9_+=,.@-]+\z"));
        assert_eq!(length, Some((Some(1), Some(64))));
        let identity = wire_identity(&expected);
        for valid in [
            row.valid_sample().to_string(),
            "AWSReservedSSO_AdministratorAccess_0123456789abcdef".to_string(),
            "AZaz09_+=,.@-".to_string(),
            "a".repeat(64),
        ] {
            dispatch_custom_type_validation(&cache, &identity, &valid)
                .unwrap_or_else(|error| panic!("valid role name {valid:?}: {error}"));
        }
        for invalid in [
            "".to_string(),
            "bad name".to_string(),
            "a".repeat(65),
            "valid\n".to_string(),
            "rôle".to_string(),
        ] {
            assert!(
                dispatch_custom_type_validation(&cache, &identity, &invalid).is_err(),
                "invalid role name {invalid:?}"
            );
        }
    }

    #[test]
    fn logs_log_group_retention_witness_dispatches_through_the_sdk_bridge() {
        let provider = AwsProcessProvider::new();
        let cache = ProviderSchemaCache::from_provider(&provider)
            .expect("AWS schemas must build a dispatch table");
        let row = carina_aws_types::AwsType::LogsLogGroupRetentionInDays;
        let expected = row.identity();
        let logs = cache
            .schemas()
            .iter()
            .find(|schema| schema.parts().resource_type == "logs.LogGroup")
            .expect("generated LogGroup schema");
        let RawShape::Int {
            identity: Some(identity),
            range: None,
            validate: Some(_),
        } = logs.parts().attributes["retention_in_days"]
            .attr_type()
            .raw_shape()
        else {
            panic!("retention must be the shared validated Int");
        };
        assert_eq!(identity, &expected);
        let identity = wire_identity(&expected);
        dispatch_custom_type_validation(&cache, &identity, "30")
            .expect("member must pass SDK dispatch as an integer");
        let error = dispatch_custom_type_validation(&cache, &identity, "2")
            .expect_err("non-member must fail SDK dispatch")
            .to_string();
        let values = row.allowed_int_values().expect("integer enum values");
        assert!(error.contains("allowed values"), "{error}");
        assert!(error.contains(&format!("{values:?}")), "{error}");
    }

    #[test]
    fn region_config_witness_dispatches_through_the_sdk_bridge() {
        let provider = AwsProcessProvider::new();
        let cache = ProviderSchemaCache::from_provider(&provider)
            .expect("AWS schemas must build a dispatch table");
        let expected = carina_aws_types::AwsType::Region.identity();
        let region = cache
            .config()
            .parts()
            .get("region")
            .expect("provider config must declare region");
        let RawShape::Enum { identity, .. } = region.raw_shape() else {
            panic!("provider config region must be an identity-bearing enum");
        };
        assert_eq!(identity, &expected);

        let identity = wire_identity(&expected);
        dispatch_custom_type_validation(&cache, &identity, "ap-northeast-1")
            .expect("a valid AWS Region must pass its witness callback");
        assert!(
            dispatch_custom_type_validation(&cache, &identity, "not-a-region").is_err(),
            "an invalid AWS Region must fail witness-derived dispatch"
        );
    }

    #[test]
    fn generated_availability_zone_witness_dispatches_through_the_sdk_bridge() {
        let provider = AwsProcessProvider::new();
        let cache = ProviderSchemaCache::from_provider(&provider)
            .expect("AWS schemas must build a dispatch table");
        let expected = carina_aws_types::AwsType::AvailabilityZone.identity();
        let subnet = cache
            .schemas()
            .iter()
            .find(|schema| schema.parts().resource_type == "ec2.Subnet")
            .expect("generated ec2.Subnet schema");
        let availability_zone = subnet
            .parts()
            .attributes
            .get("availability_zone")
            .expect("generated ec2.Subnet.availability_zone attribute");
        let RawShape::Enum { identity, .. } = availability_zone.attr_type().raw_shape() else {
            panic!("generated availability_zone must be an identity-bearing enum");
        };
        assert_eq!(identity, &expected);

        let identity = wire_identity(&expected);
        dispatch_custom_type_validation(&cache, &identity, "ap-northeast-1a")
            .expect("a valid AWS Availability Zone must pass its witness callback");
        assert!(
            dispatch_custom_type_validation(&cache, &identity, "not-an-az").is_err(),
            "an invalid AWS Availability Zone must fail witness-derived dispatch"
        );
    }

    #[test]
    fn provider_config_attribute_types_declares_account_guard_lists() {
        // The host validates `list(string)` shape against these
        // declarations before calling `initialize`. If the declarations
        // disappear, the host silently drops the attributes and the
        // guard becomes a no-op — this test prevents that regression.
        let provider = AwsProcessProvider::new();
        let types = provider.provider_config_attribute_types();
        for attr in ["allowed_account_ids", "forbidden_account_ids"] {
            let ty = types.parts().get(attr).unwrap_or_else(|| {
                panic!("{attr} must be declared as a provider config attribute")
            });
            match ty.raw_shape() {
                RawShape::List { element_type, .. }
                    if matches!(element_type.raw_shape(), RawShape::String { .. }) => {}
                other => panic!("{attr} must be a List, was {other:?}"),
            }
        }
    }

    #[test]
    fn provider_config_attribute_types_declares_assume_role_struct() {
        // The host validates the assume_role block's shape against this
        // Struct declaration before calling `initialize`. If the
        // declaration disappears or its required field changes, the
        // host stops surfacing schema-level errors for malformed
        // assume_role blocks — this test prevents that regression
        // (aws#342).
        let provider = AwsProcessProvider::new();
        let types = provider.provider_config_attribute_types();
        let ty = types
            .parts()
            .get("assume_role")
            .expect("assume_role must be declared as a provider config attribute");
        match ty.raw_shape() {
            RawShape::Struct { name, fields, .. } => {
                assert_eq!(name, "AssumeRole");
                let role_arn = fields
                    .iter()
                    .find(|field| field.name() == "role_arn")
                    .expect("assume_role.role_arn must be declared");
                assert!(
                    role_arn.parts().declared_input_mode.is_required(),
                    "role_arn must be required"
                );
                assert!(matches!(
                    role_arn.field_type().raw_shape(),
                    RawShape::String { .. }
                ));
                for opt in ["session_name", "external_id"] {
                    let f = fields
                        .iter()
                        .find(|field| field.name() == opt)
                        .unwrap_or_else(|| panic!("assume_role.{opt} must be declared"));
                    assert!(
                        !f.parts().declared_input_mode.is_required(),
                        "assume_role.{opt} must be optional"
                    );
                    assert!(matches!(
                        f.field_type().raw_shape(),
                        RawShape::String { .. }
                    ));
                }
                let duration = fields
                    .iter()
                    .find(|field| field.name() == "duration")
                    .expect("assume_role.duration must be declared");
                assert!(
                    !duration.parts().declared_input_mode.is_required(),
                    "duration must be optional"
                );
                assert!(
                    matches!(duration.field_type().raw_shape(), RawShape::Duration),
                    "duration must be a Duration so DSL literals like `30min` are accepted; \
                     was {:?}",
                    duration.field_type()
                );
            }
            other => panic!("assume_role must be a Struct, was {other:?}"),
        }
    }

    fn proto_str(s: &str) -> proto::Value {
        proto::Value::String(s.to_string())
    }

    fn proto_list_str(items: &[&str]) -> proto::Value {
        proto::Value::List(items.iter().copied().map(proto_str).collect())
    }

    fn proto_map(items: &[(&str, proto::Value)]) -> proto::Value {
        let mut m = BTreeMap::new();
        for (k, v) in items {
            m.insert((*k).to_string(), v.clone());
        }
        proto::Value::Map(m)
    }

    #[test]
    fn validate_config_accepts_assume_role_in_allowed_account() {
        let provider = AwsProcessProvider::new();
        let attrs = BTreeMap::from([
            (
                "allowed_account_ids".to_string(),
                proto_list_str(&["412038850359"]),
            ),
            (
                "assume_role".to_string(),
                proto_map(&[(
                    "role_arn",
                    proto_str("arn:aws:iam::412038850359:role/delegation"),
                )]),
            ),
        ]);
        provider
            .validate_config(&attrs)
            .expect("matching allowed_account_ids must validate");
    }

    #[test]
    fn validate_config_rejects_assume_role_outside_allowed_account() {
        let provider = AwsProcessProvider::new();
        let attrs = BTreeMap::from([
            (
                "allowed_account_ids".to_string(),
                proto_list_str(&["111111111111"]),
            ),
            (
                "assume_role".to_string(),
                proto_map(&[(
                    "role_arn",
                    proto_str("arn:aws:iam::412038850359:role/delegation"),
                )]),
            ),
        ]);
        let err = provider
            .validate_config(&attrs)
            .expect_err("cross-account role outside allowed_account_ids must fail");
        assert!(err.contains("412038850359"), "must name target: {err}");
        assert!(err.contains("111111111111"), "must name allow list: {err}");
    }

    #[test]
    fn validate_config_no_assume_role_is_noop() {
        let provider = AwsProcessProvider::new();
        let attrs = BTreeMap::from([(
            "allowed_account_ids".to_string(),
            proto_list_str(&["412038850359"]),
        )]);
        provider
            .validate_config(&attrs)
            .expect("validate_config without assume_role must be a no-op");
    }

    #[test]
    fn validate_config_assume_role_without_allow_list_is_no_guard() {
        // Per the guardrail design: when allowed_account_ids is empty,
        // we don't know which accounts are intended, so cross-account
        // assume-role is allowed. (STS itself remains the final
        // authority.)
        let provider = AwsProcessProvider::new();
        let attrs = BTreeMap::from([(
            "assume_role".to_string(),
            proto_map(&[(
                "role_arn",
                proto_str("arn:aws:iam::412038850359:role/delegation"),
            )]),
        )]);
        provider
            .validate_config(&attrs)
            .expect("without allowed_account_ids, the guard does not fire");
    }
}
