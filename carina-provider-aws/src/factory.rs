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
        types.insert("assume_role".to_string(), assume_role_attribute_type());
        types
    }

    fn validate_config(&self, attributes: &IndexMap<String, Value>) -> Result<(), String> {
        // Cross-account guardrail: when `assume_role.role_arn` parses to
        // an account id that is not in `allowed_account_ids` (when that
        // list is configured), refuse the configuration. Avoids the
        // foot-gun of an assume-role silently landing in the wrong AWS
        // account (aws#342).
        use crate::services::sts::account_guard::extract_string_list;
        use crate::services::sts::assume_role::{check_cross_account, extract_assume_role};
        let assume_role = extract_assume_role(attributes.get("assume_role"))?;
        if let Some(ar) = &assume_role {
            let allowed = extract_string_list(attributes.get("allowed_account_ids"));
            check_cross_account(&ar.role_arn, &allowed)?;
        }
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
        use crate::services::sts::assume_role::extract_assume_role;
        let region = self.extract_region(attributes);
        let allowed = extract_string_list(attributes.get("allowed_account_ids"));
        let forbidden = extract_string_list(attributes.get("forbidden_account_ids"));
        let assume_role = match extract_assume_role(attributes.get("assume_role")) {
            Ok(v) => v,
            Err(e) => {
                return Box::pin(async move {
                    Err(carina_core::provider::ProviderError::invalid_input(e))
                });
            }
        };
        Box::pin(async move {
            Ok(Box::new(
                AwsProvider::new_with_account_guard(&region, allowed, forbidden, assume_role).await,
            ) as Box<dyn carina_core::provider::Provider>)
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

/// Schema for the provider-level `assume_role` block. Mirrors the
/// Terraform AWS provider's `assume_role` (MVP field set: `role_arn`,
/// `session_name`, `external_id`, `duration`). When present, the
/// provider chains an `sts:AssumeRole` call on top of the ambient
/// credential chain (aws#342).
fn assume_role_attribute_type() -> carina_core::schema::AttributeType {
    use carina_core::schema::{AttributeType, StructField};
    AttributeType::Struct {
        name: "AssumeRole".to_string(),
        fields: vec![
            StructField::new("role_arn", AttributeType::String)
                .required()
                .with_description("IAM role ARN to assume."),
            StructField::new("session_name", AttributeType::String)
                .with_description("STS session name to associate with the assumed-role session."),
            StructField::new("external_id", AttributeType::String)
                .with_description("External ID required by the trust policy of the assumed role."),
            StructField::new("duration", AttributeType::Duration)
                .with_description("Assumed-role session duration (e.g., 30min, 1h, 15s)."),
        ],
    }
}
