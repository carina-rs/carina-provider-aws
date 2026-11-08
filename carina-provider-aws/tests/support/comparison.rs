struct ComparisonOnlyProvider;

impl carina_core::provider::Provider for ComparisonOnlyProvider {
    fn name(&self) -> &str {
        "aws-comparison-only"
    }

    fn read(
        &self,
        _id: &carina_core::resource::ResourceId,
        _identifier: &carina_core::provider::AppliedIdentifier,
        _request: carina_core::provider::ReadRequest,
    ) -> carina_core::provider::BoxFuture<
        '_,
        carina_core::provider::ProviderResult<carina_core::provider::ReadOutcome>,
    > {
        panic!("comparison-only provider must not dispatch read")
    }

    fn read_data_source(
        &self,
        _resource: &carina_core::provider::ProviderReadyDataSource,
    ) -> carina_core::provider::BoxFuture<
        '_,
        carina_core::provider::ProviderResult<carina_core::resource::State>,
    > {
        panic!("comparison-only provider must not dispatch data-source read")
    }

    fn create(
        &self,
        _id: &carina_core::resource::ResourceId,
        _request: carina_core::provider::CreateRequest,
        _attempt: carina_core::provider::CreateAttempt,
    ) -> carina_core::provider::BoxFuture<
        '_,
        Result<carina_core::provider::CreateOutcome, carina_core::provider::CreateFailure>,
    > {
        panic!("comparison-only provider must not dispatch create")
    }

    fn resume_create(
        &self,
        _request: carina_core::provider::ResumeCreateRequest,
    ) -> carina_core::provider::BoxFuture<
        '_,
        Result<carina_core::provider::ResumeOutcome, carina_core::provider::ResumeFailure>,
    > {
        panic!("comparison-only provider must not dispatch resume-create")
    }

    fn update(
        &self,
        _id: &carina_core::resource::ResourceId,
        _identifier: &carina_core::provider::AppliedIdentifier,
        _request: carina_core::provider::UpdateRequest,
        _attempt: carina_core::provider::UpdateAttempt,
    ) -> carina_core::provider::BoxFuture<
        '_,
        carina_core::provider::ProviderResult<carina_core::provider::UpdateOutcome>,
    > {
        panic!("comparison-only provider must not dispatch update")
    }

    fn delete(
        &self,
        _attempt: carina_core::provider::DeleteAttempt,
    ) -> carina_core::provider::BoxFuture<'_, carina_core::provider::ProviderResult<()>> {
        panic!("comparison-only provider must not dispatch delete")
    }

    fn required_permissions(
        &self,
        _id: &carina_core::resource::ResourceId,
        _op: carina_core::effect::PlanOp,
    ) -> carina_core::provider::ProviderResult<Vec<String>> {
        panic!("comparison-only provider must not dispatch permission lookup")
    }
}

/// Build a scope-unavailable provider router for downstream test comparisons.
#[doc(hidden)]
pub fn test_comparison_router() -> carina_core::provider::ProviderRouter {
    let mut router = carina_core::provider::ProviderRouter::new();
    router.add_provider(
        "aws".to_string(),
        carina_core::test_utils::provider_text::initialized(Box::new(ComparisonOnlyProvider)),
    );
    router
}

/// Compare a provider test fixture that declares no comparison references.
#[doc(hidden)]
#[track_caller]
pub fn test_diff(
    desired: &carina_core::resource::ResolvedResource,
    current: &carina_core::resource::State<impl carina_core::resource::StateValueLeaf>,
    saved: &carina_core::provider::LiftedSavedAttrs,
    previous_explicit: Option<&carina_core::explicit::ExplicitFields>,
    comparison: carina_core::provider::ResolvedComparison<'_>,
) -> carina_core::differ::Diff {
    comparison
        .context()
        .with_references(
            &carina_core::references::prepare_empty_for_tests(),
            &desired.id,
            |context| {
                carina_core::differ::diff(
                    desired,
                    &current.to_snapshot(),
                    saved,
                    previous_explicit,
                    comparison,
                    context,
                )
            },
        )
        .expect("reference-free comparison fixture")
        .expect("fixture comparison should succeed")
}
