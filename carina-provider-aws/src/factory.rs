//! AWS Provider factory implementation

use std::collections::HashMap;

use carina_core::provider::{BoxFuture, ProviderFactory, ProviderNormalizer};
use carina_core::resource::Value;

use crate::AwsProvider;
use crate::normalizer::AwsNormalizer;

/// Factory for creating and configuring the AWS Provider
pub struct AwsProviderFactory;

impl ProviderFactory for AwsProviderFactory {
    fn name(&self) -> &str {
        "aws"
    }

    fn display_name(&self) -> &str {
        "AWS provider"
    }

    fn provider_config_attribute_types(
        &self,
    ) -> HashMap<String, carina_core::schema::AttributeType> {
        let mut types = HashMap::new();
        types.insert(
            "region".to_string(),
            carina_core::schema::AttributeType::StringEnum {
                name: "Region".to_string(),
                values: carina_aws_types::REGIONS
                    .iter()
                    .map(|(code, _)| code.to_string())
                    .collect(),
                namespace: Some("aws".to_string()),
                to_dsl: Some(|s| s.replace('-', "_")),
            },
        );
        types
    }

    fn validate_config(&self, _attributes: &HashMap<String, Value>) -> Result<(), String> {
        // Region format/value validation is handled by the host via
        // `provider_config_attribute_types`. No provider-specific semantic
        // checks are needed beyond that for now.
        Ok(())
    }

    fn extract_region(&self, attributes: &HashMap<String, Value>) -> String {
        if let Some(Value::String(region)) = attributes.get("region") {
            return carina_core::utils::convert_region_value(region);
        }
        "ap-northeast-1".to_string()
    }

    fn create_provider(
        &self,
        attributes: &HashMap<String, Value>,
    ) -> BoxFuture<'_, Box<dyn carina_core::provider::Provider>> {
        let region = self.extract_region(attributes);
        Box::pin(async move {
            Box::new(AwsProvider::new(&region).await) as Box<dyn carina_core::provider::Provider>
        })
    }

    fn create_normalizer(
        &self,
        _attributes: &HashMap<String, Value>,
    ) -> BoxFuture<'_, Box<dyn ProviderNormalizer>> {
        Box::pin(async { Box::new(AwsNormalizer) as Box<dyn ProviderNormalizer> })
    }

    fn schemas(&self) -> Vec<carina_core::schema::ResourceSchema> {
        crate::schemas::all_schemas()
    }

    fn identity_attributes(&self) -> Vec<&str> {
        vec!["region"]
    }

    fn config_completions(
        &self,
    ) -> std::collections::HashMap<String, Vec<carina_core::schema::CompletionValue>> {
        std::collections::HashMap::from([(
            "region".to_string(),
            carina_aws_types::region_completions("aws"),
        )])
    }

    fn get_enum_alias_reverse(
        &self,
        resource_type: &str,
        attr_name: &str,
        value: &str,
    ) -> Option<String> {
        crate::schemas::generated::get_enum_alias_reverse(resource_type, attr_name, value)
            .map(|s| s.to_string())
    }
}
