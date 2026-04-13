use std::collections::HashMap;

mod convert;
use carina_plugin_sdk::CarinaProvider;
use carina_provider_protocol::types as proto;

use carina_core::provider::{Provider, ProviderError as CoreProviderError, ProviderNormalizer};
use carina_core::resource::Value as CoreValue;
use carina_core::schema::ResourceSchema;

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
        proto::ProviderError {
            message: e.to_string(),
            resource_id: e
                .resource_id
                .as_ref()
                .map(convert::core_to_proto_resource_id),
            is_timeout: e.is_timeout,
        }
    }

    fn provider(&self) -> &AwsProvider {
        self.provider
            .as_ref()
            .expect("Provider not initialized; call initialize() first")
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
        types.insert(
            "region".to_string(),
            proto::AttributeType::StringEnum {
                name: "Region".to_string(),
                values: carina_aws_types::REGIONS
                    .iter()
                    .map(|(code, _)| code.to_string())
                    .collect(),
                namespace: Some("aws".to_string()),
            },
        );
        types
    }

    fn validate_config(&self, _attrs: &HashMap<String, proto::Value>) -> Result<(), String> {
        // Region format/value validation is handled by the host via
        // `provider_config_attribute_types`. No provider-specific semantic
        // checks are needed beyond that for now.
        Ok(())
    }

    fn initialize(&mut self, attrs: &HashMap<String, proto::Value>) -> Result<(), String> {
        let core_attrs = convert::proto_to_core_value_map(attrs);
        let region = if let Some(CoreValue::String(region)) = core_attrs.get("region") {
            carina_core::utils::convert_region_value(region)
        } else {
            "ap-northeast-1".to_string()
        };
        let provider = self.runtime.block_on(AwsProvider::new(&region));
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

    fn enum_aliases(&self) -> HashMap<String, HashMap<String, HashMap<String, String>>> {
        carina_provider_aws::schemas::generated::build_enum_aliases_map()
    }

    fn validate_custom_type(&self, type_name: &str, value: &str) -> Result<(), String> {
        use carina_provider_aws::schemas::types::aws_validators;
        use std::sync::OnceLock;
        type ValidatorMap = HashMap<String, Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>>;
        static VALIDATORS: OnceLock<ValidatorMap> = OnceLock::new();
        let validators = VALIDATORS.get_or_init(aws_validators);
        if let Some(validator) = validators.get(type_name) {
            validator(value)
        } else {
            Ok(())
        }
    }

    fn read(
        &self,
        id: &proto::ResourceId,
        identifier: Option<&str>,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let result = self
            .runtime
            .block_on(self.provider().read(&core_id, identifier));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn read_data_source(
        &self,
        resource: &proto::Resource,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_resource = convert::proto_to_core_resource(resource);
        let result = self
            .runtime
            .block_on(self.provider().read_data_source(&core_resource));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn create(&self, resource: &proto::Resource) -> Result<proto::State, proto::ProviderError> {
        let core_resource = convert::proto_to_core_resource(resource);
        let result = self
            .runtime
            .block_on(self.provider().create(&core_resource));
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn update(
        &self,
        id: &proto::ResourceId,
        identifier: &str,
        from: &proto::State,
        to: &proto::Resource,
    ) -> Result<proto::State, proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let core_from = convert::proto_to_core_state(from);
        let core_to = convert::proto_to_core_resource(to);
        let result = self.runtime.block_on(
            self.provider()
                .update(&core_id, identifier, &core_from, &core_to),
        );
        match result {
            Ok(state) => Ok(convert::core_to_proto_state(&state)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn delete(
        &self,
        id: &proto::ResourceId,
        identifier: &str,
        lifecycle: &proto::LifecycleConfig,
    ) -> Result<(), proto::ProviderError> {
        let core_id = convert::proto_to_core_resource_id(id);
        let core_lifecycle = carina_core::resource::LifecycleConfig {
            force_delete: lifecycle.force_delete,
            create_before_destroy: lifecycle.create_before_destroy,
            prevent_destroy: lifecycle.prevent_destroy,
        };
        let result = self.runtime.block_on(self.provider().delete(
            &core_id,
            identifier,
            &core_lifecycle,
        ));
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
        self.normalizer.normalize_desired(&mut core_resources);
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
        let core_tags = convert::proto_to_core_value_map(default_tags);
        let core_schemas: HashMap<String, ResourceSchema> = proto_schemas
            .iter()
            .map(|s| (s.resource_type.clone(), convert::proto_to_core_schema(s)))
            .collect();
        self.normalizer
            .merge_default_tags(&mut core_resources, &core_tags, &core_schemas);
        *resources = core_resources
            .iter()
            .map(convert::core_to_proto_resource)
            .collect();
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
    use carina_plugin_sdk::types::ValidatorType;

    #[test]
    fn schemas_include_tags_validator_for_tagged_resources() {
        let provider = AwsProcessProvider::new();
        let schemas = provider.schemas();
        let bucket = schemas
            .iter()
            .find(|s| s.resource_type == "aws.s3.bucket")
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
                .find(|s| s.resource_type == format!("aws.{}", untagged.resource_type_name))
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
            schemas.iter().any(|s| s.resource_type == "aws.iam.role"),
            "aws.iam.role schema should be registered"
        );
        assert!(
            schemas
                .iter()
                .any(|s| s.resource_type == "aws.logs.log_group"),
            "aws.logs.log_group schema should be registered"
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_vpc_id() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("vpc_id", "vpc-12345678")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_vpc_id() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("vpc_id", "subnet-12345678")
                .is_err()
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("arn", "arn:aws:s3:::my-bucket")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_invalid_arn() {
        let provider = AwsProcessProvider::new();
        assert!(provider.validate_custom_type("arn", "not-an-arn").is_err());
    }

    #[test]
    fn validate_custom_type_passes_unknown_type() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("unknown_type", "any-value")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_accepts_valid_iam_role_arn() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("iam_role_arn", "arn:aws:iam::123456789012:role/my-role")
                .is_ok()
        );
    }

    #[test]
    fn validate_custom_type_rejects_iam_policy_arn_for_role() {
        let provider = AwsProcessProvider::new();
        assert!(
            provider
                .validate_custom_type("iam_role_arn", "arn:aws:iam::123456789012:policy/my-policy")
                .is_err()
        );
    }
}
