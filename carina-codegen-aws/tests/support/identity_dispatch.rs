//! Exercise the exact SDK function called by the WASM guest export.
use carina_core::schema::{ValidatedConfigSchema, ValidatedResourceSchema};
use carina_plugin_sdk::types::*;
use carina_plugin_sdk::{
    CarinaProvider, CreateAttempt, CreateStep, CreateVerdict, DeleteOutcome, Never, Pending,
    PendingDelete, PlanOp, Poll, PollVerdict, ProviderSchemaCache, Resume, ResumeStep,
    ResumeVerdict, dispatch_custom_type_validation, provider_schema_to_json,
};
use std::collections::BTreeMap;

struct SchemaProvider {
    include_every_schema: bool,
}
impl CarinaProvider for SchemaProvider {
    type CreateToken = Never;
    type UpdateToken = Never;
    type DeleteToken = Never;
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "aws".into(),
            display_name: "Generated AWS schema test".into(),
            capabilities: vec![],
            version: "test".into(),
        }
    }
    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        super::generated::configs()
            .into_iter()
            .filter(|config| {
                self.include_every_schema
                    || matches!(
                        config.resource_type_name,
                        "s3.Bucket" | "iam.Role" | "ec2.Subnet"
                    )
            })
            .map(|config| {
                config
                    .schema
                    .validate()
                    .expect("generated AWS schema admission")
            })
            .collect()
    }
    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        ValidatedConfigSchema::default()
    }
    fn reference_forms(&self) -> carina_core::schema::ReferenceForms {
        carina_core::schema::ReferenceForms::new()
    }
    fn assignability_edges(&self) -> Vec<carina_core::schema::AssignabilityEdge> {
        vec![]
    }
    fn validate_config(&self, _attrs: &BTreeMap<String, Value>) -> Result<(), ProviderError> {
        panic!("schema-only test provider")
    }
    fn initialize(
        &mut self,
        _attrs: &BTreeMap<String, Value>,
    ) -> Result<ReferenceScope, ProviderError> {
        panic!("schema-only test provider")
    }
    fn read(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: ReadRequest,
    ) -> Result<ReadOutcome, ProviderError> {
        panic!("schema-only test provider")
    }
    fn read_data_source(&self, _resource: &Resource) -> Result<State, ProviderError> {
        panic!("schema-only test provider")
    }
    fn create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _attempt: CreateAttempt,
    ) -> Result<CreateStep<Self::CreateToken>, CreateVerdict<Self::CreateToken>> {
        panic!("schema-only test provider")
    }
    fn poll_create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _poll: Poll<Self::CreateToken>,
        _poll_request: PollRequest,
    ) -> Result<CreateStep<Self::CreateToken>, PollVerdict<Self::CreateToken>> {
        panic!("schema-only test provider")
    }
    fn resume_create(
        &self,
        _id: &ResourceId,
        _request: CreateRequest,
        _resume: Resume<Self::CreateToken>,
        _poll_request: PollRequest,
    ) -> Result<ResumeStep<Self::CreateToken>, ResumeVerdict<Self::CreateToken>> {
        panic!("schema-only test provider")
    }
    fn update(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: UpdateRequest,
    ) -> Result<carina_plugin_sdk::UpdateOutcome<Self::UpdateToken>, ProviderError> {
        panic!("schema-only test provider")
    }
    fn poll_update(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: UpdateRequest,
        _pending: Pending<Self::UpdateToken>,
        _poll_request: PollRequest,
    ) -> Result<carina_plugin_sdk::UpdateOutcome<Self::UpdateToken>, ProviderError> {
        panic!("schema-only test provider")
    }
    fn delete(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: DeleteRequest,
    ) -> Result<DeleteOutcome<Self::DeleteToken>, ProviderError> {
        panic!("schema-only test provider")
    }
    fn poll_delete(
        &self,
        _id: &ResourceId,
        _identifier: &AppliedIdentifier,
        _request: DeleteRequest,
        _pending: PendingDelete<Self::DeleteToken>,
    ) -> Result<DeleteOutcome<Self::DeleteToken>, ProviderError> {
        panic!("schema-only test provider")
    }
    fn required_permissions(
        &self,
        _id: &ResourceId,
        _op: PlanOp,
    ) -> Result<Vec<String>, ProviderError> {
        panic!("schema-only test provider")
    }
}

fn cache() -> ProviderSchemaCache {
    ProviderSchemaCache::from_provider(&SchemaProvider {
        include_every_schema: false,
    })
    .expect("actual generated AWS dispatch table")
}

fn wire_identity(row: carina_aws_types::AwsType) -> TypeIdentity {
    let identity = row.identity();
    TypeIdentity {
        provider: identity.provider.unwrap_or_default(),
        segments: identity.segments,
        kind: identity.kind,
    }
}

#[test]
fn guest_dispatch_retains_only_semantic_validation() {
    use carina_aws_types::AwsType;
    let cache = cache();
    let identity = wire_identity(AwsType::IamRoleArn);
    let bad = "arn:aws:iam::123:role/a";
    let good = "arn:aws:iam::123456789012:role/a";
    let error = dispatch_custom_type_validation(&cache, &identity, bad).unwrap_err();
    assert!(error.to_string().contains("account"), "{error}");
    dispatch_custom_type_validation(&cache, &identity, good).unwrap();
    // Portable-only rows need no executable validator over the wire.
    for row in [AwsType::Route53HostedZoneId, AwsType::VpcId] {
        assert!(!row.has_validator());
        dispatch_custom_type_validation(&cache, &wire_identity(row), row.valid_sample()).unwrap();
    }
}

#[test]
fn wire_retains_identity_and_member_constraints_distinctly() {
    let cache = cache();
    for row in [
        carina_aws_types::AwsType::Route53HostedZoneId,
        carina_aws_types::AwsType::VpcId,
        carina_aws_types::AwsType::IamRoleArn,
    ] {
        let ty = row.attribute_type();
        let schema =
            ValidatedConfigSchema::validate(BTreeMap::from([("value".into(), ty)])).unwrap();
        let wire =
            carina_core::protocol_convert::attribute_type_to_protocol(&schema.parts()["value"])
                .unwrap();
        let AttributeType::String {
            identity_layer,
            member_layer,
        } = wire
        else {
            panic!("String")
        };
        assert_eq!(identity_layer.identity, Some(row.identity().to_string()));
        assert!(identity_layer.pattern.is_some());
        assert!(identity_layer.validate.is_none());
        assert!(member_layer.pattern.is_none() && member_layer.length.is_none());
    }
    let role = cache
        .schemas()
        .iter()
        .find(|s| s.parts().resource_type == "iam.Role")
        .unwrap();
    let wire = carina_core::protocol_convert::attribute_type_to_protocol(
        role.parts().attributes["description"].attr_type(),
    )
    .unwrap();
    let AttributeType::String {
        identity_layer,
        member_layer,
    } = wire
    else {
        panic!("String")
    };
    assert!(identity_layer.pattern.is_none() && identity_layer.length.is_none());
    assert!(member_layer.pattern.is_some());
    assert_eq!(member_layer.length, Some((Some(0), Some(1000))));
}

#[test]
fn full_guest_schema_cache_admits() {
    ProviderSchemaCache::from_provider(&SchemaProvider {
        include_every_schema: true,
    })
    .expect("the real guest admits all generated schemas together before custom-type dispatch");
}

/// Model the host boundary with the actual wire-decoded schemas and the exact
/// guest dispatch function. No local callback survives the wire round trip.
struct TransportFactory {
    guest: ProviderSchemaCache,
    host: carina_core::schema::ValidatedProviderSchemaWitness,
}
impl carina_core::provider::ProviderFactory for TransportFactory {
    fn name(&self) -> &str {
        "aws"
    }
    fn display_name(&self) -> &str {
        "AWS guest dispatch test"
    }
    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        self.host.resources().to_vec()
    }
    fn schema_witness(&self) -> carina_core::schema::ValidatedProviderSchemaWitness {
        self.host.clone()
    }
    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        self.host.config().clone()
    }
    fn validate_config(
        &self,
        _: &carina_core::provider::CanonicalProviderConfigAttributes,
        _: &carina_core::provider::ProviderValidationInputs,
    ) -> carina_core::provider::ProviderResult<()> {
        Ok(())
    }
    fn validate_custom_type(
        &self,
        identity: &carina_core::schema::TypeIdentity,
        value: &carina_core::provider::CanonicalCustomTypeValue,
        _: &carina_core::provider::ProviderValidationInputs,
    ) -> carina_core::provider::ProviderResult<()> {
        let wire = TypeIdentity {
            provider: identity.provider.clone().unwrap_or_default(),
            segments: identity.segments.clone(),
            kind: identity.kind.clone(),
        };
        dispatch_custom_type_validation(&self.guest, &wire, value.as_str()).map_err(|error| {
            carina_core::provider::HostError::invalid_input(error.to_string()).into()
        })
    }
    fn create_provider(
        &self,
        _: &carina_core::provider::ProviderSession,
        _: carina_core::provider::InitializeOutcomeMint,
    ) -> carina_core::provider::BoxFuture<
        '_,
        carina_core::provider::ProviderResult<carina_core::provider::ProviderInitialization>,
    > {
        Box::pin(async { panic!("schema-only test provider") })
    }
}

#[test]
fn wire_host_enforces_structure_then_dispatches_semantics() {
    let guest = cache();
    let wire = serde_json::from_str(&provider_schema_to_json(guest.witness()).unwrap()).unwrap();
    let host = carina_core::protocol_convert::protocol_to_provider_schema_witness(
        &wire,
        ValidatedConfigSchema::default(),
    )
    .unwrap();
    let factory = TransportFactory { guest, host };
    let cases = [
        (
            "s3.Bucket",
            "hosted_zone_id",
            "/hostedzone/Z0",
            "Z012345",
            false,
        ),
        (
            "ec2.Subnet",
            "vpc_id",
            "vpc-ABCDEF12",
            "vpc-abcdef12",
            false,
        ),
        (
            "iam.Role",
            "arn",
            "arn:aws:iam::123:role/a",
            "arn:aws:iam::123456789012:role/a",
            true,
        ),
    ];
    for (resource, attribute, bad, _, semantic) in cases {
        let schema = factory
            .host
            .resources()
            .iter()
            .find(|s| s.parts().resource_type == resource)
            .unwrap();
        assert_eq!(
            schema
                .effective_attribute(attribute)
                .unwrap()
                .validate(&text(bad))
                .is_ok(),
            semantic
        );
    }
    let cases = cases.map(|(resource, attribute, bad, good, _)| {
        let schema = factory
            .host
            .resources()
            .iter()
            .find(|s| s.parts().resource_type == resource)
            .unwrap();
        (
            schema.parts().attributes[attribute].attr_type().clone(),
            bad,
            good,
        )
    });
    let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> = vec![Box::new(factory)];
    let loaded = carina_core::provider::loaded_provider_snapshot_from_factories(
        std::sync::Arc::new(factories),
        carina_core::schema::SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::new([]),
    );
    for (ty, bad, good) in cases {
        let declaration = carina_core::parser::IdentifiedCustomTypeDeclaration::new(&ty).unwrap();
        let target = carina_core::parser::CustomTypeValidationTarget::Declaration {
            owner: "aws",
            declaration,
        };
        let bad = text(bad);
        let good = text(good);
        assert!(
            carina_core::parser::validate_custom_type_value(
                loaded.provider_context().custom_type_resolver(),
                target,
                &known(&bad)
            )
            .is_err()
        );
        carina_core::parser::validate_custom_type_value(
            loaded.provider_context().custom_type_resolver(),
            target,
            &known(&good),
        )
        .unwrap();
    }
}

fn text(s: &str) -> carina_core::resource::Value {
    carina_core::resource::Value::Concrete(carina_core::resource::ConcreteValue::String(s.into()))
}
fn known(value: &carina_core::resource::Value) -> carina_core::resource::knowledge::KnownValue<'_> {
    value
        .knowledge(&carina_core::resource::knowledge::KnowledgeContext::validation())
        .unwrap()
        .into_classification()
        .into_known()
        .unwrap()
}
