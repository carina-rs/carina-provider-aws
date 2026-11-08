use std::collections::BTreeMap;

use carina_aws_types::{AwsType, elbv2_target_group_grpc_code, elbv2_target_group_http_code};
use carina_core::provider::{
    CustomTypeDispatchError, ProviderError as CoreProviderError, ProviderErrorKind,
};
use carina_core::schema::{
    AssignabilityEdge, AttributeSchema, ReferenceForms, ResourceSchema, ValidatedConfigSchema,
    ValidatedResourceSchema,
};
use carina_core::test_utils::provider_text::NativeWireFixture as _;
use carina_plugin_sdk::types::{
    AppliedIdentifier, CreateRequest, DeleteRequest, ProviderError, ProviderInfo, ReadOutcome,
    ReadRequest, ReferenceScope, Resource, ResourceId, State, TypeIdentity, UpdateRequest, Value,
};
use carina_plugin_sdk::{
    CarinaProvider, CreateAttempt, CreateStep, CreateVerdict, DeleteOutcome, Never, Pending,
    PendingDelete, PlanOp, Poll, PollRequest, PollVerdict, ProviderSchemaCache, Resume, ResumeStep,
    ResumeVerdict, UpdateOutcome, dispatch_custom_type_validation,
};

// The AWS provider has no ELBv2 resource. Admit the shared constructors in a
// schema-only provider to exercise the same SDK dispatch used by consumers.
struct MatcherProvider;

impl CarinaProvider for MatcherProvider {
    type CreateToken = Never;
    type UpdateToken = Never;
    type DeleteToken = Never;

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "aws".into(),
            display_name: "ELBv2 matcher test".into(),
            capabilities: vec![],
            version: "test".into(),
        }
    }

    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        vec![
            ResourceSchema::new("aws.elasticloadbalancingv2.TargetGroup")
                .attribute(
                    AttributeSchema::optional("http_code", elbv2_target_group_http_code()).build(),
                )
                .attribute(
                    AttributeSchema::optional("grpc_code", elbv2_target_group_grpc_code()).build(),
                )
                .validate()
                .expect("matcher constructors must pass schema admission"),
        ]
    }

    fn reference_forms(&self) -> ReferenceForms {
        ReferenceForms::new()
    }

    fn assignability_edges(&self) -> Vec<AssignabilityEdge> {
        Vec::new()
    }

    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        ValidatedConfigSchema::default()
    }

    fn validate_config(&self, _attrs: &BTreeMap<String, Value>) -> Result<(), ProviderError> {
        Ok(())
    }

    fn initialize(
        &mut self,
        _attrs: &BTreeMap<String, Value>,
    ) -> Result<ReferenceScope, ProviderError> {
        Ok(ReferenceScope::Unavailable)
    }

    fn read(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: ReadRequest,
    ) -> Result<ReadOutcome, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn read_data_source(&self, _resource: &Resource) -> Result<State, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _attempt: CreateAttempt,
    ) -> Result<CreateStep<Never>, CreateVerdict<Never>> {
        unreachable!("schema-only provider")
    }

    fn poll_create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _poll: Poll<Never>,
        _poll_request: PollRequest,
    ) -> Result<CreateStep<Never>, PollVerdict<Never>> {
        unreachable!("schema-only provider")
    }

    fn resume_create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _resume: Resume<Never>,
        _poll_request: PollRequest,
    ) -> Result<ResumeStep<Never>, ResumeVerdict<Never>> {
        unreachable!("schema-only provider")
    }

    fn update(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: UpdateRequest,
    ) -> Result<UpdateOutcome<Never>, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn poll_update(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: UpdateRequest,
        _pending: Pending<Never>,
        _poll_request: PollRequest,
    ) -> Result<UpdateOutcome<Never>, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn delete(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: DeleteRequest,
    ) -> Result<DeleteOutcome<Never>, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn poll_delete(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: DeleteRequest,
        _pending: PendingDelete<Never>,
    ) -> Result<DeleteOutcome<Never>, ProviderError> {
        unreachable!("schema-only provider")
    }

    fn required_permissions(
        &self,
        _id: &ResourceId,
        _op: PlanOp,
    ) -> Result<Vec<String>, ProviderError> {
        Ok(vec![])
    }
}

fn assert_dispatch(
    cache: &ProviderSchemaCache,
    row: AwsType,
    valid: &[&str],
    invalid: &[&str],
    range: &str,
) {
    let identity = row.identity();
    let wire_identity = TypeIdentity {
        provider: identity.provider.expect("AWS provider identity"),
        segments: identity.segments,
        kind: identity.kind,
    };
    for text in valid {
        dispatch_custom_type_validation(cache, &wire_identity, text)
            .unwrap_or_else(|error| panic!("{row:?} rejected valid {text:?}: {error}"));
    }
    for text in invalid {
        let error = dispatch_custom_type_validation(cache, &wire_identity, text)
            .expect_err("invalid matcher codes must fail SDK dispatch");
        let CustomTypeDispatchError::Validation(message) = error else {
            panic!("{text:?}: expected a validation error, got {error:?}");
        };
        let message = CoreProviderError::from_diagnostic(ProviderErrorKind::InvalidInput, *message)
            .to_wire()
            .message;
        assert!(
            message.contains("comma-separated items N or N-N"),
            "{text:?}: {message}"
        );
        assert!(message.contains(range), "{text:?}: {message}");
        assert!(message.contains("no leading zeros"), "{text:?}: {message}");
    }
}

#[test]
fn elbv2_matcher_catalog_rows_dispatch_through_the_sdk_bridge() {
    let cache = ProviderSchemaCache::from_provider(&MatcherProvider)
        .expect("matcher schemas must build an SDK dispatch table");
    assert_dispatch(
        &cache,
        AwsType::Elbv2TargetGroupHttpCode,
        &[
            "200",
            "599",
            "200,201-299,500-599",
            "200-299,302",
            "200-200",
        ],
        &[
            "0200",
            "200-0299",
            "200,0302",
            "199",
            "600",
            "599-200",
            "200,",
            "200, 201",
            "+200",
            "２００",
            "200\n",
        ],
        "200..=599",
    );
    assert_dispatch(
        &cache,
        AwsType::Elbv2TargetGroupGrpcCode,
        &["0", "99", "0,1-5,80-99", "0-5", "12", "0-0"],
        &[
            "00-05", "0,05", "080", "100", "-1", "99-0", "0,,1", "0, 1", "+0", "０", "0\n",
        ],
        "0..=99",
    );
}
