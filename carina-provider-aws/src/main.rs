#[cfg(any(target_arch = "wasm32", test))]
mod convert;

#[cfg(any(target_arch = "wasm32", test))]
use std::collections::HashMap;

#[cfg(any(target_arch = "wasm32", test))]
use carina_plugin_sdk::{
    CarinaProvider, CreateAttempt as SdkCreateAttempt, CreateStep as SdkCreateStep,
    CreateVerdict as SdkCreateVerdict, DeleteOutcome as SdkDeleteOutcome, FoundRead,
    IssuedCreate as SdkIssuedCreate, Never, Pending, Poll as SdkPoll,
    PollAttempt as SdkPollAttempt, PollVerdict as SdkPollVerdict, Resume as SdkResume,
    ResumeAttempt as SdkResumeAttempt, ResumeStep as SdkResumeStep,
    ResumeVerdict as SdkResumeVerdict, ResumedCreate as SdkResumedCreate,
    UpdateOutcome as SdkUpdateOutcome,
};
#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_protocol::OperationStep;
#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_protocol::types as proto;

#[cfg(any(target_arch = "wasm32", test))]
use carina_core::provider::{
    AppliedIdentifier as CoreAppliedIdentifier, AppliedState as CoreAppliedState,
    DeleteRequest as CoreDeleteRequest, Observation as CoreObservation, PatchOp as CorePatchOp,
    Provider, ProviderError as CoreProviderError, ProviderNormalizer,
    ReadRequest as CoreReadRequest, Sent as CoreSent, UpdatePatch as CoreUpdatePatch,
};
#[cfg(any(target_arch = "wasm32", test))]
use carina_core::resource::Value as CoreValue;
#[cfg(any(target_arch = "wasm32", test))]
use carina_core::schema::SchemaRegistry;

#[cfg(any(target_arch = "wasm32", test))]
use carina_provider_aws::{
    AwsCreateAttempt, AwsCreatePoll, AwsCreateResume, AwsCreateToken, AwsDeleteToken,
    AwsIssuedCreate, AwsNormalizer, AwsPollAttempt, AwsProvider, AwsResumeAttempt,
    AwsResumedCreate,
};

#[cfg(any(target_arch = "wasm32", test))]
const CREATE_POLL_AFTER_SECS: u32 = 5;

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
        CoreObservation::Read(outcome) => {
            proto::Observation::Read(convert::core_to_proto_read_outcome(outcome))
        }
        CoreObservation::ReadFailed(error) => proto::Observation::ReadFailed(error.to_wire()),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsCreateAttempt for GuestCreateAttempt {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkCreateVerdict;
    type Issued = GuestIssuedCreate;

    fn untracked(self, error: CoreProviderError) -> Self::Verdict {
        self.0.untracked(error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier, sent: CoreSent) -> Self::Issued {
        GuestIssuedCreate(self.0.issued(identifier, sent))
    }

    fn pending(
        self,
        token: AwsCreateToken,
        sent: CoreSent,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, sent, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsPollAttempt for GuestPollAttempt {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkPollVerdict;
    type Issued = GuestPolledIssuedCreate;

    fn untracked(self, error: CoreProviderError) -> Self::Verdict {
        self.0.untracked(error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier) -> Self::Issued {
        GuestPolledIssuedCreate(self.0.issued(identifier))
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsIssuedCreate for GuestPolledIssuedCreate {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkPollVerdict;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn succeeded(self, attributes: HashMap<String, CoreValue>) -> Self::Step {
        self.0
            .succeeded(convert::core_to_proto_value_map(&attributes))
    }

    fn partial(
        self,
        attributes: HashMap<String, CoreValue>,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self::Step {
        self.0.partial(
            convert::core_to_proto_value_map(&attributes),
            proto::PartialReadDiagnostic {
                reason,
                missing_attributes,
            },
        )
    }

    fn failed(self, observation: CoreObservation, error: CoreProviderError) -> Self::Verdict {
        self.0
            .failed(observation_to_proto(observation), error.to_wire())
            .into()
    }

    fn discarded(self, error: CoreProviderError) -> Self::Verdict {
        self.0.discarded(error.to_wire()).into()
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsIssuedCreate for GuestIssuedCreate {
    type Step = SdkCreateStep<AwsCreateToken>;
    type Verdict = SdkCreateVerdict;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn succeeded(self, attributes: HashMap<String, CoreValue>) -> Self::Step {
        self.0
            .succeeded(convert::core_to_proto_value_map(&attributes))
    }

    fn partial(
        self,
        attributes: HashMap<String, CoreValue>,
        reason: String,
        missing_attributes: Vec<String>,
    ) -> Self::Step {
        self.0.partial(
            convert::core_to_proto_value_map(&attributes),
            proto::PartialReadDiagnostic {
                reason,
                missing_attributes,
            },
        )
    }

    fn failed(self, observation: CoreObservation, error: CoreProviderError) -> Self::Verdict {
        self.0
            .failed(observation_to_proto(observation), error.to_wire())
    }

    fn discarded(self, error: CoreProviderError) -> Self::Verdict {
        self.0.discarded(error.to_wire())
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsResumeAttempt for GuestResumeAttempt {
    type Step = SdkResumeStep<AwsCreateToken>;
    type Verdict = SdkResumeVerdict;
    type Resumed = GuestResumedCreate;

    fn untracked(self, error: CoreProviderError) -> Self::Verdict {
        self.0.untracked(error.to_wire())
    }

    fn undetermined(self, error: CoreProviderError) -> Self::Verdict {
        self.0.undetermined(error.to_wire())
    }

    fn issued(self, identifier: CoreAppliedIdentifier) -> Self::Resumed {
        GuestResumedCreate(self.0.issued(identifier))
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl AwsResumedCreate for GuestResumedCreate {
    type Step = SdkResumeStep<AwsCreateToken>;
    type Verdict = SdkResumeVerdict;

    fn identifier(&self) -> &CoreAppliedIdentifier {
        self.0.identifier()
    }

    fn completed(self, state: CoreAppliedState) -> Self::Step {
        self.0.completed(FoundRead::from_applied_state(
            convert::core_to_proto_applied_state(&state),
        ))
    }

    fn failed(self, observation: CoreObservation, error: CoreProviderError) -> Self::Verdict {
        self.0
            .failed(observation_to_proto(observation), error.to_wire())
    }

    fn discarded(self, error: CoreProviderError) -> Self::Verdict {
        self.0.discarded(error.to_wire())
    }

    fn pending(
        self,
        token: AwsCreateToken,
        poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(self.0.pending(token, poll_after_secs))
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
        let mut registry = SchemaRegistry::new();
        for schema in carina_provider_aws::schemas::all_schemas() {
            registry.insert("aws", schema);
        }
        registry
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl CarinaProvider for AwsProcessProvider {
    type CreateToken = AwsCreateToken;
    type UpdateToken = Never;
    type DeleteToken = AwsDeleteToken;

    fn info(&self) -> proto::ProviderInfo {
        proto::ProviderInfo {
            name: "aws".into(),
            display_name: "AWS provider".into(),
            capabilities: self.capabilities(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn schemas(&self) -> Vec<proto::ResourceSchema> {
        carina_provider_aws::schemas::generated::configs()
            .iter()
            .map(|config| {
                let mut schema = convert::core_to_proto_schema(&config.schema);
                if config.has_tags {
                    schema
                        .validators
                        .push(proto::ValidatorType::TagsKeyValueCheck);
                }
                schema
            })
            .collect()
    }

    fn provider_config_attribute_types(&self) -> HashMap<String, proto::AttributeType> {
        let mut types = HashMap::new();
        // Region codes carry hyphens; the DSL form replaces them with
        // underscores. Materialize the alias pairs so the WASM-component
        // boundary preserves the mapping (carina#2832 / aws#247).
        let region_values: Vec<String> = carina_aws_types::REGIONS
            .iter()
            .map(|(code, _)| code.to_string())
            .collect();
        let region_dsl_aliases: Vec<(String, String)> = region_values
            .iter()
            .filter_map(|code| {
                let dsl = code.replace('-', "_");
                if dsl == *code {
                    None
                } else {
                    Some((code.clone(), dsl))
                }
            })
            .collect();
        types.insert(
            "region".to_string(),
            proto::AttributeType::StringEnum {
                name: "Region".to_string(),
                values: region_values,
                // proto's StringEnum still carries the flat `namespace`
                // string for the wire form; only the core form was
                // restructured in carina#3222.
                namespace: Some("aws".to_string()),
                dsl_aliases: region_dsl_aliases,
            },
        );
        types.insert(
            "allowed_account_ids".to_string(),
            proto::AttributeType::List {
                element_type: Box::new(proto_string_type()),
                ordered: false,
                length: None,
                validate: None,
            },
        );
        types.insert(
            "forbidden_account_ids".to_string(),
            proto::AttributeType::List {
                element_type: Box::new(proto_string_type()),
                ordered: false,
                length: None,
                validate: None,
            },
        );
        types.insert("assume_role".to_string(), assume_role_attribute_type());
        types
    }

    fn validate_config(&self, attrs: &HashMap<String, proto::Value>) -> Result<(), String> {
        // Region format/value validation is handled by the host via
        // `provider_config_attribute_types`. The provider-specific check
        // we add here is the cross-account guardrail: when `assume_role`
        // is present, its `role_arn` must target an account that is
        // listed in `allowed_account_ids` (when that list is configured).
        use carina_provider_aws::services::sts::account_guard::extract_string_list;
        use carina_provider_aws::services::sts::assume_role::{
            check_cross_account, extract_assume_role,
        };
        let core_attrs = convert::proto_to_core_value_map(attrs);
        let assume_role = extract_assume_role(core_attrs.get("assume_role"))?;
        if let Some(ar) = &assume_role {
            let allowed = extract_string_list(core_attrs.get("allowed_account_ids"));
            check_cross_account(&ar.role_arn, &allowed)?;
        }
        Ok(())
    }

    fn initialize(&mut self, attrs: &HashMap<String, proto::Value>) -> Result<(), String> {
        use carina_provider_aws::services::sts::account_guard::extract_string_list;
        use carina_provider_aws::services::sts::assume_role::extract_assume_role;
        let core_attrs: indexmap::IndexMap<_, _> = convert::proto_to_core_value_map(attrs)
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
        Ok(())
    }

    fn config_completions(&self) -> HashMap<String, Vec<proto::CompletionValue>> {
        HashMap::from([(
            "region".to_string(),
            carina_aws_types::region_completions("aws")
                .into_iter()
                .map(|c| proto::CompletionValue {
                    value: c.value,
                    description: c.description,
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

    fn validate_custom_type(
        &self,
        identity: &carina_plugin_sdk::types::TypeIdentity,
        value: &str,
    ) -> Result<(), String> {
        let core_identity = carina_core::schema::TypeIdentity::new(
            (!identity.provider.is_empty()).then_some(identity.provider.as_str()),
            identity.segments.iter().map(String::as_str),
            identity.kind.as_str(),
        );
        carina_provider_aws::schemas::config::validate_aws_custom_type(&core_identity, value)
    }

    fn read(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        _request: proto::ReadRequest,
    ) -> Result<proto::ReadOutcome, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let result =
            self.runtime
                .block_on(self.provider().read(&core_id, identifier, CoreReadRequest));
        match result {
            Ok(outcome) => Ok(convert::core_to_proto_read_outcome(outcome)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn read_data_source(
        &self,
        resource: &proto::Resource,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_data_source = convert::proto_to_core_data_source(resource);
        let result = self
            .runtime
            .block_on(self.provider().read_data_source_resource(&core_data_source));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        attempt: SdkCreateAttempt,
    ) -> Result<SdkCreateStep<Self::CreateToken>, SdkCreateVerdict> {
        let core_resource = convert::proto_to_core_resource(&request.resource);
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
            Err(error) => return Err(attempt.untracked(Self::convert_error(error))),
        };
        self.runtime.block_on(
            self.provider()
                .create_step(core_resource.as_resource(), GuestCreateAttempt(attempt)),
        )
    }

    fn poll_create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        poll: SdkPoll<Self::CreateToken>,
    ) -> Result<SdkCreateStep<Self::CreateToken>, SdkPollVerdict> {
        let core_resource = convert::proto_to_core_resource(&request.resource);
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
                    SdkPoll::Unissued(attempt, _token) => Err(attempt.undetermined(error)),
                    SdkPoll::Issued(issued, token) => {
                        Ok(issued.pending(token, CREATE_POLL_AFTER_SECS))
                    }
                };
            }
        };
        let (token, poll) = match poll {
            SdkPoll::Unissued(attempt, token) => {
                (token, AwsCreatePoll::Unissued(GuestPollAttempt(attempt)))
            }
            SdkPoll::Issued(issued, token) => (
                token,
                AwsCreatePoll::Issued(GuestPolledIssuedCreate(issued)),
            ),
        };
        self.runtime.block_on(self.provider().poll_create_step(
            core_resource.as_resource(),
            token,
            poll,
        ))
    }

    fn resume_create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
        resume: SdkResume<Self::CreateToken>,
    ) -> Result<SdkResumeStep<Self::CreateToken>, SdkResumeVerdict> {
        let core_resource = convert::proto_to_core_resource(&request.resource);
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
                    SdkResume::Unissued(attempt, _token) => Err(attempt.undetermined(error)),
                    SdkResume::Issued(resumed, token) => {
                        Ok(resumed.pending(token, CREATE_POLL_AFTER_SECS))
                    }
                };
            }
        };
        let (token, resume) = match resume {
            SdkResume::Unissued(attempt, token) => (
                token,
                AwsCreateResume::Unissued(GuestResumeAttempt(attempt)),
            ),
            SdkResume::Issued(resumed, token) => {
                (token, AwsCreateResume::Issued(GuestResumedCreate(resumed)))
            }
        };
        self.runtime.block_on(self.provider().resume_create_step(
            core_resource.as_resource(),
            token,
            resume,
        ))
    }

    fn update(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        request: proto::UpdateRequest,
    ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let state_after_patch = request.state_after_patch();
        let core_from = convert::proto_to_core_persisted_state(&request.from).map_err(|error| {
            Self::convert_error(
                CoreProviderError::internal_caused_by(
                    "failed to decode persisted update state",
                    error,
                )
                .for_resource(core_id.clone()),
            )
        })?;
        let core_patch = CoreUpdatePatch {
            ops: request
                .patch
                .ops
                .iter()
                .map(|op| match op {
                    proto::PatchOp::Add { key, value } => CorePatchOp::Add {
                        key: key.clone(),
                        value: convert::proto_to_core_value(value),
                    },
                    proto::PatchOp::Replace { key, value } => CorePatchOp::Replace {
                        key: key.clone(),
                        value: convert::proto_to_core_value(value),
                    },
                    proto::PatchOp::Reassert { key, value } => CorePatchOp::Reassert {
                        key: key.clone(),
                        value: convert::proto_to_core_value(value),
                    },
                    proto::PatchOp::Remove { key } => CorePatchOp::Remove { key: key.clone() },
                })
                .collect(),
        };
        let result = self.runtime.block_on(self.provider().update_with_patch(
            &core_id,
            identifier,
            &core_from,
            &core_patch,
        ));
        match result {
            Ok(outcome) => Ok(SdkUpdateOutcome::Done(
                convert::core_to_proto_update_outcome(outcome, state_after_patch),
            )),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn poll_update(
        &self,
        _id: &proto::ResourceId,
        _identifier: &proto::AppliedIdentifier,
        _request: proto::UpdateRequest,
        pending: Pending<Self::UpdateToken>,
    ) -> Result<SdkUpdateOutcome<Self::UpdateToken>, proto::ProviderError> {
        match pending.token {}
    }

    fn delete(
        &self,
        id: &proto::ResourceId,
        identifier: &proto::AppliedIdentifier,
        request: proto::DeleteRequest,
    ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let core_directives = carina_core::resource::Directives {
            force_delete: request.directives.force_delete,
            create_before_destroy: request.directives.create_before_destroy,
            prevent_destroy: request.directives.prevent_destroy,
            depends_on: Vec::new(),
            provider_instance: None,
        };
        let core_request = CoreDeleteRequest {
            directives: core_directives,
        };
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
            }) => Ok(SdkDeleteOutcome::Pending(Pending {
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
        pending: Pending<Self::DeleteToken>,
    ) -> Result<SdkDeleteOutcome<Self::DeleteToken>, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let result = self
            .runtime
            .block_on(self.provider().poll_delete_step(&core_id, pending.token));
        match result {
            Ok(OperationStep::Done(())) => Ok(SdkDeleteOutcome::Deleted),
            Ok(OperationStep::Pending {
                pending,
                poll_after_secs,
            }) => Ok(SdkDeleteOutcome::Pending(Pending {
                token: pending,
                poll_after_secs,
            })),
            Ok(OperationStep::Verdict(error)) => Err(Self::convert_error(error)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn normalize_desired(
        &self,
        resources: Vec<proto::Resource>,
    ) -> Result<Vec<proto::Resource>, proto::ProviderError> {
        let mut core_resources: Vec<_> = resources
            .iter()
            .map(convert::proto_to_core_resource)
            .collect();
        // Guest-side: drive the now-async normalizer on the guest's own
        // outermost runtime — the same pattern the `Provider` CRUD
        // methods use here (`self.runtime.block_on(...)`). Not a nested
        // runtime: the host drives the WASM call, the guest drives its
        // internal async with this runtime (carina#3112 design Non-goal).
        self.runtime
            .block_on(self.normalizer.normalize_desired(&mut core_resources))
            .map_err(Self::convert_error)?;
        Ok(core_resources
            .iter()
            .map(convert::core_to_proto_resource)
            .collect())
    }
}

/// Schema for the provider-level `assume_role` block. Mirrors the
/// Terraform AWS provider's `assume_role` (MVP field set:
/// `role_arn`, `session_name`, `external_id`, `duration`). When
/// present, the provider chains an `sts:AssumeRole` call on top of
/// the ambient credential chain (aws#342).
#[cfg(any(target_arch = "wasm32", test))]
fn assume_role_attribute_type() -> proto::AttributeType {
    proto::AttributeType::Struct {
        name: "AssumeRole".to_string(),
        fields: vec![
            proto::StructField {
                name: "role_arn".to_string(),
                field_type: proto_string_type(),
                required: true,
                description: Some("IAM role ARN to assume.".to_string()),
                block_name: None,
                provider_name: None,
                read_only: false,
                write_only: false,
                deferred_populate: false,
            },
            proto::StructField {
                name: "session_name".to_string(),
                field_type: proto_string_type(),
                required: false,
                description: Some(
                    "STS session name to associate with the assumed-role session.".to_string(),
                ),
                block_name: None,
                provider_name: None,
                read_only: false,
                write_only: false,
                deferred_populate: false,
            },
            proto::StructField {
                name: "external_id".to_string(),
                field_type: proto_string_type(),
                required: false,
                description: Some(
                    "External ID required by the trust policy of the assumed role.".to_string(),
                ),
                block_name: None,
                provider_name: None,
                read_only: false,
                write_only: false,
                deferred_populate: false,
            },
            proto::StructField {
                name: "duration".to_string(),
                field_type: proto::AttributeType::Duration,
                required: false,
                description: Some(
                    "Assumed-role session duration (e.g., 30min, 1h, 15s).".to_string(),
                ),
                block_name: None,
                provider_name: None,
                read_only: false,
                write_only: false,
                deferred_populate: false,
            },
        ],
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn proto_string_type() -> proto::AttributeType {
    proto::AttributeType::String {
        pattern: None,
        length: None,
        validate: None,
        to_dsl: None,
        identity: None,
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
carina_plugin_sdk::export_provider!(AwsProcessProvider, http);

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_plugin_sdk::types::{TypeIdentity, ValidatorType};

    fn ident(segments: &[&str], kind: &str) -> TypeIdentity {
        TypeIdentity {
            provider: "aws".to_string(),
            segments: segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect(),
            kind: kind.to_string(),
        }
    }

    #[test]
    fn schemas_include_tags_validator_for_tagged_resources() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        let bucket = schemas
            .iter()
            .find(|s| s.resource_type == "s3.Bucket")
            .expect("s3.bucket schema should exist");
        assert!(
            bucket
                .validators
                .contains(&ValidatorType::TagsKeyValueCheck),
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
                .find(|s| s.resource_type == untagged.resource_type_name)
                .expect("untagged schema should exist");
            assert!(
                !schema
                    .validators
                    .contains(&ValidatorType::TagsKeyValueCheck),
                "untagged resource should not have TagsKeyValueCheck"
            );
        }
    }

    #[test]
    fn schemas_include_iam_role_and_logs_log_group() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        assert!(
            schemas.iter().any(|s| s.resource_type == "iam.Role"),
            "iam.Role schema should be registered"
        );
        assert!(
            schemas.iter().any(|s| s.resource_type == "logs.LogGroup"),
            "logs.LogGroup schema should be registered"
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_vpc_id() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident(&["ec2", "Vpc"], "Id"), "vpc-12345678")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_vpc_id() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident(&["ec2", "Vpc"], "Id"), "subnet-12345678")
                .is_err()
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident(&[], "Arn"), "arn:aws:s3:::my-bucket")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident(&[], "Arn"), "not-an-arn")
                .is_err()
        );
    }

    #[test]
    fn validate_custom_type_passes_unknown_type() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident(&[], "UnknownType"), "any-value")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_uses_structured_availability_zone_identity() {
        let provider = AwsProcessProvider::new();
        let identity = ident(&["AvailabilityZone"], "ZoneName");

        assert!(
            provider
                .validate_custom_type(&identity, "ap-northeast-1a")
                .is_ok()
        );
        assert!(
            provider
                .validate_custom_type(&identity, "not-an-az")
                .is_err(),
            "the guest provider entry point must find the structured AZ validator"
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
            let ty = types.get(attr).unwrap_or_else(|| {
                panic!("{attr} must be declared as a provider config attribute")
            });
            match ty {
                proto::AttributeType::List { element_type, .. } => match element_type.as_ref() {
                    proto::AttributeType::String { .. } => {}
                    other => {
                        panic!("{attr} must be List<String>, inner was {other:?}")
                    }
                },
                other => panic!("{attr} must be a List, was {other:?}"),
            }
        }
    }

    #[test]
    fn validate_custom_type_accepts_valid_iam_role_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(
                    &ident(&["iam", "Role"], "Arn"),
                    "arn:aws:iam::123456789012:role/my-role"
                )
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_iam_policy_arn_for_role() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(
                    &ident(&["iam", "Role"], "Arn"),
                    "arn:aws:iam::123456789012:policy/my-policy"
                )
                .is_err()
        );
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
            .get("assume_role")
            .expect("assume_role must be declared as a provider config attribute");
        match ty {
            proto::AttributeType::Struct { name, fields } => {
                assert_eq!(name, "AssumeRole");
                let role_arn = fields
                    .iter()
                    .find(|f| f.name == "role_arn")
                    .expect("assume_role.role_arn must be declared");
                assert!(role_arn.required, "role_arn must be required");
                assert!(matches!(
                    role_arn.field_type,
                    proto::AttributeType::String { .. }
                ));
                for opt in ["session_name", "external_id"] {
                    let f = fields
                        .iter()
                        .find(|f| f.name == opt)
                        .unwrap_or_else(|| panic!("assume_role.{opt} must be declared"));
                    assert!(!f.required, "assume_role.{opt} must be optional");
                    assert!(matches!(f.field_type, proto::AttributeType::String { .. }));
                }
                let duration = fields
                    .iter()
                    .find(|f| f.name == "duration")
                    .expect("assume_role.duration must be declared");
                assert!(!duration.required, "duration must be optional");
                assert!(
                    matches!(duration.field_type, proto::AttributeType::Duration),
                    "duration must be a Duration so DSL literals like `30min` are accepted; \
                     was {:?}",
                    duration.field_type
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
        let mut m = std::collections::HashMap::new();
        for (k, v) in items {
            m.insert((*k).to_string(), v.clone());
        }
        proto::Value::Map(m)
    }

    #[test]
    fn validate_config_accepts_assume_role_in_allowed_account() {
        let provider = AwsProcessProvider::new();
        let attrs = HashMap::from([
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
        let attrs = HashMap::from([
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
        let attrs = HashMap::from([(
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
        let attrs = HashMap::from([(
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
