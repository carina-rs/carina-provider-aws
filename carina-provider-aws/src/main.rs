use std::collections::HashMap;

mod convert;
use carina_plugin_sdk::CarinaProvider;
use carina_provider_protocol::types as proto;

use carina_core::provider::{
    DeleteRequest as CoreDeleteRequest, PatchOp as CorePatchOp, PatchOpKind as CorePatchOpKind,
    Provider, ProviderError as CoreProviderError, ProviderNormalizer,
    ReadRequest as CoreReadRequest, UpdatePatch as CoreUpdatePatch,
    UpdateRequest as CoreUpdateRequest,
};
use carina_core::resource::{ConcreteValue, Value as CoreValue};
use carina_core::schema::SchemaRegistry;

use carina_provider_aws::AwsNormalizer;
use carina_provider_aws::AwsProvider;

struct AwsProcessProvider {
    runtime: tokio::runtime::Runtime,
    provider: Option<AwsProvider>,
    normalizer: AwsNormalizer,
}

impl Default for AwsProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

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
        let kind = match e {
            CoreProviderError::InvalidInput(_) => proto::ProviderErrorKind::InvalidInput,
            CoreProviderError::ApiError(_) => proto::ProviderErrorKind::ApiError,
            CoreProviderError::NotFound(_) => proto::ProviderErrorKind::NotFound,
            CoreProviderError::Timeout(_) => proto::ProviderErrorKind::Timeout,
            CoreProviderError::Internal(_) => proto::ProviderErrorKind::Internal,
        };
        let detail = e.detail();
        proto::ProviderError {
            kind,
            message: detail.message.clone(),
            resource_id: detail
                .resource_id
                .as_ref()
                .map(|id| convert::core_to_proto_resource_id(id)),
            cause: detail.cause.as_ref().map(|c| c.to_string()),
            provider_name: detail.provider_name.clone(),
            operation: detail.operation.clone(),
            status: detail.status,
            code: detail.code.clone(),
            request_id: detail.request_id.clone(),
        }
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

impl CarinaProvider for AwsProcessProvider {
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
        let core_attrs = convert::proto_to_core_value_map(attrs);
        let region = if let Some(CoreValue::Concrete(ConcreteValue::String(region))) =
            core_attrs.get("region")
        {
            carina_core::utils::convert_region_value(region)
        } else {
            "ap-northeast-1".to_string()
        };
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
    ) -> Vec<String> {
        Vec::new()
    }

    fn enum_aliases(&self) -> HashMap<String, HashMap<String, HashMap<String, String>>> {
        carina_provider_aws::schemas::generated::build_enum_aliases_map()
    }

    fn validate_custom_type(
        &self,
        identity: &carina_plugin_sdk::types::TypeIdentity,
        value: &str,
    ) -> Result<(), String> {
        use carina_provider_aws::schemas::config::aws_validators;
        use std::sync::OnceLock;
        type ValidatorMap = HashMap<String, Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>>;
        static VALIDATORS: OnceLock<ValidatorMap> = OnceLock::new();
        let validators = VALIDATORS.get_or_init(aws_validators);
        // Inner lookup map is still snake_case-keyed; convert the
        // structured identity's `kind` to snake at this single
        // boundary. Provider-axis collisions are already filtered by
        // the host before the call reaches us — the SDK only routes
        // identities scoped to this provider here.
        let key = carina_core::parser::pascal_to_snake(&identity.kind);
        if let Some(validator) = validators.get(&key) {
            validator(value)
        } else {
            Ok(())
        }
    }

    fn read(
        &self,
        id: &proto::ResourceId,
        identifier: Option<&str>,
        _request: proto::ReadRequest,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let result =
            self.runtime
                .block_on(self.provider().read(&core_id, identifier, CoreReadRequest));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
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
            .block_on(self.provider().read_data_source(&core_data_source));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn create(
        &self,
        _id: &proto::ResourceId,
        request: proto::CreateRequest,
    ) -> Result<proto::CreateOutcome, proto::ProviderError> {
        let core_resource = convert::proto_to_core_resource(&request.resource);
        let schemas = Self::schema_registry();
        let core_resource = self.runtime.block_on(
            carina_core::executor::normalized::apply_desired_normalization(
                core_resource,
                &[],
                &self.normalizer,
                &[],
                &schemas,
            ),
        );
        let result = self
            .runtime
            .block_on(self.provider().create_resource(core_resource.as_resource()));
        match result {
            Ok(outcome) => Ok(convert::core_to_proto_create_outcome(outcome)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn update(
        &self,
        id: &proto::ResourceId,
        identifier: &str,
        request: proto::UpdateRequest,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let core_from = convert::proto_to_core_state(&request.from);
        let core_patch = CoreUpdatePatch {
            ops: request
                .patch
                .ops
                .iter()
                .map(|op| CorePatchOp {
                    kind: match op.kind {
                        proto::PatchOpKind::Add => CorePatchOpKind::Add,
                        proto::PatchOpKind::Replace => CorePatchOpKind::Replace,
                        proto::PatchOpKind::Remove => CorePatchOpKind::Remove,
                    },
                    key: op.key.clone(),
                    value: op.value.as_ref().map(convert::proto_to_core_value),
                })
                .collect(),
        };
        let core_request = CoreUpdateRequest {
            from: core_from,
            patch: core_patch,
        };
        let result =
            self.runtime
                .block_on(self.provider().update(&core_id, identifier, core_request));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn delete(
        &self,
        id: &proto::ResourceId,
        identifier: &str,
        request: proto::DeleteRequest,
    ) -> Result<(), proto::ProviderError> {
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
        let result =
            self.runtime
                .block_on(self.provider().delete(&core_id, identifier, core_request));
        match result {
            Ok(()) => Ok(()),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn normalize_desired(&self, resources: Vec<proto::Resource>) -> Vec<proto::Resource> {
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
            .block_on(self.normalizer.normalize_desired(&mut core_resources));
        core_resources
            .iter()
            .map(convert::core_to_proto_resource)
            .collect()
    }

    fn merge_default_tags(
        &self,
        resources: &mut Vec<proto::Resource>,
        default_tags: &HashMap<String, proto::Value>,
        proto_schemas: &Vec<proto::ResourceSchema>,
    ) {
        let mut core_resources: Vec<_> = resources
            .iter()
            .map(convert::proto_to_core_resource)
            .collect();
        let core_tags: indexmap::IndexMap<String, _> = default_tags
            .iter()
            .map(|(k, v)| (k.clone(), convert::proto_to_core_value(v)))
            .collect();
        let mut registry = SchemaRegistry::new();
        for s in proto_schemas {
            registry.insert("aws", convert::proto_to_core_schema(s));
        }
        self.runtime.block_on(self.normalizer.merge_default_tags(
            &mut core_resources,
            &core_tags,
            &registry,
        ));
        *resources = core_resources
            .iter()
            .map(convert::core_to_proto_resource)
            .collect();
    }
}

/// Schema for the provider-level `assume_role` block. Mirrors the
/// Terraform AWS provider's `assume_role` (MVP field set:
/// `role_arn`, `session_name`, `external_id`, `duration`). When
/// present, the provider chains an `sts:AssumeRole` call on top of
/// the ambient credential chain (aws#342).
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
            },
        ],
    }
}

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
    carina_plugin_sdk::run(AwsProcessProvider::new());
}

#[cfg(target_arch = "wasm32")]
carina_plugin_sdk::export_provider!(AwsProcessProvider, http);

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_plugin_sdk::types::{TypeIdentity, ValidatorType};

    /// Test helper: build a structured identity whose dotted display
    /// round-trips back to the legacy snake-cased semantic name via
    /// `pascal_to_snake(identity.kind)`. The production
    /// `validate_custom_type` impl looks the validator up in
    /// `aws_validators`, which is still keyed on snake-cased semantic
    /// names — so a bare identity with the PascalCase kind hits the
    /// same map entry. Migrating `aws_validators` itself to a
    /// structured-identity map is S2.5c follow-up work.
    fn ident(legacy_snake: &str) -> TypeIdentity {
        TypeIdentity {
            provider: String::new(),
            segments: vec![],
            kind: carina_core::parser::snake_to_pascal(legacy_snake),
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
                .validate_custom_type(&ident("vpc_id"), "vpc-12345678")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_vpc_id() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident("vpc_id"), "subnet-12345678")
                .is_err()
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident("arn"), "arn:aws:s3:::my-bucket")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident("arn"), "not-an-arn")
                .is_err()
        );
    }

    #[test]
    fn validate_custom_type_passes_unknown_type() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type(&ident("unknown_type"), "any-value")
                .is_ok()
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
                    &ident("iam_role_arn"),
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
                    &ident("iam_role_arn"),
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
