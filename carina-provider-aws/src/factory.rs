//! AWS Provider factory implementation

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{
    BoxFuture, CanonicalCustomTypeValue, CanonicalProviderConfigAttributes, ProviderFactory,
    ProviderNormalizer, ProviderReadyConfig, ProviderResult,
};
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
            carina_core::schema::AttributeType::enum_(
                carina_core::schema::enum_identity("Region", Some("aws")),
                Some(region_values),
                region_dsl_aliases,
                None,
                None,
            ),
        );
        types.insert(
            "allowed_account_ids".to_string(),
            carina_core::schema::AttributeType::unordered_list(
                carina_core::schema::AttributeType::string(),
            ),
        );
        types.insert(
            "forbidden_account_ids".to_string(),
            carina_core::schema::AttributeType::unordered_list(
                carina_core::schema::AttributeType::string(),
            ),
        );
        types.insert("assume_role".to_string(), assume_role_attribute_type());
        types
    }

    fn validate_config(
        &self,
        attributes: &CanonicalProviderConfigAttributes,
    ) -> Result<(), String> {
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

    fn validate_custom_type(
        &self,
        identity: &carina_core::schema::TypeIdentity,
        value: &CanonicalCustomTypeValue,
    ) -> Result<(), String> {
        crate::schemas::config::validate_aws_custom_type(identity, value.as_str())
    }

    fn extract_region(&self, attributes: &IndexMap<String, Value>) -> String {
        carina_core::utils::extract_region_from_attrs(attributes, "ap-northeast-1")
    }

    fn create_provider(
        &self,
        _binding: Option<&str>,
        config: &ProviderReadyConfig,
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
        let attributes = config.attributes();
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
        _config: &ProviderReadyConfig,
    ) -> BoxFuture<'_, ProviderResult<Box<dyn ProviderNormalizer>>> {
        Box::pin(async { Ok(Box::new(AwsNormalizer) as Box<dyn ProviderNormalizer>) })
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
}

/// Schema for the provider-level `assume_role` block. Mirrors the
/// Terraform AWS provider's `assume_role` (MVP field set: `role_arn`,
/// `session_name`, `external_id`, `duration`). When present, the
/// provider chains an `sts:AssumeRole` call on top of the ambient
/// credential chain (aws#342).
fn assume_role_attribute_type() -> carina_core::schema::AttributeType {
    use carina_core::schema::{AttributeType, StructField};
    AttributeType::struct_(
        "AssumeRole".to_string(),
        vec![
            StructField::new("role_arn", AttributeType::string())
                .required()
                .with_description("IAM role ARN to assume."),
            StructField::new("session_name", AttributeType::string())
                .with_description("STS session name to associate with the assumed-role session."),
            StructField::new("external_id", AttributeType::string())
                .with_description("External ID required by the trust policy of the assumed role."),
            StructField::new("duration", AttributeType::duration())
                .with_description("Assumed-role session duration (e.g., 30min, 1h, 15s)."),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::ConcreteValue;
    use carina_core::schema::Schema;

    #[test]
    fn extract_region_reads_provider_ready_canonical_enum() {
        let factory = AwsProviderFactory;
        let region_type = factory
            .provider_config_attribute_types()
            .remove("region")
            .expect("region provider config type");
        let region = Schema::flat(region_type)
            .canonicalize_state(Value::Concrete(ConcreteValue::String("us-west-2".into())));
        assert!(matches!(
            &region,
            Value::Concrete(ConcreteValue::CanonicalEnum(value))
                if value.api_value() == "us-west-2"
        ));
        let attributes = IndexMap::from([("region".to_string(), region)]);

        assert_eq!(factory.extract_region(&attributes), "us-west-2");
    }

    #[test]
    fn extract_region_keeps_string_literal() {
        let factory = AwsProviderFactory;
        let attributes = IndexMap::from([(
            "region".to_string(),
            Value::Concrete(ConcreteValue::String("aws.Region.us_west_2".into())),
        )]);

        assert_eq!(factory.extract_region(&attributes), "aws.Region.us_west_2");
    }

    #[test]
    fn validate_custom_type_uses_structured_availability_zone_identity() {
        let identity = carina_aws_types::provider_bare_type(&["AvailabilityZone"], "ZoneName");
        let zone = Schema::flat(carina_aws_types::availability_zone()).canonicalize_state(
            Value::Concrete(ConcreteValue::String("ap-northeast-1a".into())),
        );
        assert!(matches!(
            &zone,
            Value::Concrete(ConcreteValue::CanonicalEnum(value))
                if value.api_value() == "ap-northeast-1a"
        ));

        let factories: Vec<Box<dyn ProviderFactory>> = vec![Box::new(AwsProviderFactory)];
        let validate = carina_core::executor::provider_custom_type_lookup(&factories);

        validate(&identity, &zone).expect("canonical API spelling should validate");
        assert!(
            validate(
                &identity,
                &Value::Concrete(ConcreteValue::String("not-an-az".into()))
            )
            .is_err(),
            "the host entry point must find the structured AZ validator"
        );
    }
}
