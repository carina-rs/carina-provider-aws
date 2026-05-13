//! AWS Provider factory implementation

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{BoxFuture, ProviderFactory, ProviderNormalizer};
use carina_core::resource::{ConcreteValue, Value};

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
        // Region codes carry hyphens (`ap-northeast-1`) but the DSL
        // identifier form replaces them with underscores
        // (`ap_northeast_1`). Materialize the alias pairs as data so the
        // mapping survives the WASM-component boundary — a `fn` pointer
        // would not (carina#2831).
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
            carina_core::schema::AttributeType::StringEnum {
                name: "Region".to_string(),
                values: region_values,
                namespace: Some("aws".to_string()),
                dsl_aliases: region_dsl_aliases,
            },
        );
        types.insert(
            "allowed_account_ids".to_string(),
            carina_core::schema::AttributeType::List {
                inner: Box::new(carina_core::schema::AttributeType::String),
                ordered: false,
            },
        );
        types.insert(
            "forbidden_account_ids".to_string(),
            carina_core::schema::AttributeType::List {
                inner: Box::new(carina_core::schema::AttributeType::String),
                ordered: false,
            },
        );
        types
    }

    fn validate_config(&self, _attributes: &IndexMap<String, Value>) -> Result<(), String> {
        // Region format/value validation is handled by the host via
        // `provider_config_attribute_types`. No provider-specific semantic
        // checks are needed beyond that for now.
        Ok(())
    }

    fn extract_region(&self, attributes: &IndexMap<String, Value>) -> String {
        if let Some(Value::Concrete(ConcreteValue::String(region))) = attributes.get("region") {
            return carina_core::utils::convert_region_value(region);
        }
        "ap-northeast-1".to_string()
    }

    fn create_provider(
        &self,
        _binding: Option<&str>,
        attributes: &IndexMap<String, Value>,
    ) -> BoxFuture<
        '_,
        carina_core::provider::ProviderResult<Box<dyn carina_core::provider::Provider>>,
    > {
        // `_binding` is intentionally unused: the AWS factory does not
        // cache instances, so each call already produces an
        // independent `AwsProvider`. The host uses the binding name
        // as a cache key in `WasmProviderFactory`; for in-process
        // factories the constructed-fresh shape is enough.
        use crate::services::sts::account_guard::extract_string_list;
        let region = self.extract_region(attributes);
        let allowed = extract_string_list(attributes.get("allowed_account_ids"));
        let forbidden = extract_string_list(attributes.get("forbidden_account_ids"));
        Box::pin(async move {
            Ok(
                Box::new(AwsProvider::new_with_account_guard(&region, allowed, forbidden).await)
                    as Box<dyn carina_core::provider::Provider>,
            )
        })
    }

    fn create_normalizer(
        &self,
        _binding: Option<&str>,
        _attributes: &IndexMap<String, Value>,
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
