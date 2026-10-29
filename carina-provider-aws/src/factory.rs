//! AWS Provider factory implementation

use std::collections::BTreeMap;

use indexmap::IndexMap;

use carina_core::provider::{
    BoxFuture, CanonicalProviderConfigAttributes, ProviderError, ProviderFactory,
    ProviderNormalizer, ProviderReadyConfig, ProviderResult,
};
use carina_core::resource::Value;
use carina_core::schema::{ValidatedConfigSchema, ValidatedResourceSchema};

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

    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        let mut types = BTreeMap::new();
        types.insert("region".to_string(), carina_aws_types::aws_region());
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
        ValidatedConfigSchema::validate(types).expect("AWS provider config schema must be valid")
    }

    fn validate_config(
        &self,
        attributes: &CanonicalProviderConfigAttributes,
    ) -> ProviderResult<()> {
        // Cross-account guardrail: when `assume_role.role_arn` parses to
        // an account id that is not in `allowed_account_ids` (when that
        // list is configured), refuse the configuration. Avoids the
        // foot-gun of an assume-role silently landing in the wrong AWS
        // account (aws#342).
        use crate::services::sts::account_guard::extract_string_list;
        use crate::services::sts::assume_role::{check_cross_account, extract_assume_role};
        let assume_role = extract_assume_role(attributes.get("assume_role"))
            .map_err(ProviderError::invalid_input)?;
        if let Some(ar) = &assume_role {
            let allowed = extract_string_list(attributes.get("allowed_account_ids"));
            check_cross_account(&ar.role_arn, &allowed).map_err(ProviderError::invalid_input)?;
        }
        Ok(())
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

    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
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
            StructField::required("role_arn", AttributeType::string())
                .with_description("IAM role ARN to assume.")
                .build(),
            StructField::optional("session_name", AttributeType::string())
                .with_description("STS session name to associate with the assumed-role session.")
                .build(),
            StructField::optional("external_id", AttributeType::string())
                .with_description("External ID required by the trust policy of the assumed role.")
                .build(),
            StructField::optional("duration", AttributeType::duration())
                .with_description("Assumed-role session duration (e.g., 30min, 1h, 15s).")
                .build(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::ConcreteValue;

    #[test]
    fn extract_region_reads_provider_ready_canonical_enum() {
        let factory = AwsProviderFactory;
        let config = factory
            .provider_config_attribute_types()
            .rootless_type("region")
            .expect("region provider config type");
        let region =
            config.canonicalize_state(Value::Concrete(ConcreteValue::String("us-west-2".into())));
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
    fn availability_zone_witness_uses_catalog_identity() {
        let identity = carina_aws_types::AwsType::AvailabilityZone.identity();
        let zone_schema = carina_core::schema::ValidatedConfigSchema::validate(BTreeMap::from([(
            "availability_zone".to_string(),
            carina_aws_types::availability_zone(),
        )]))
        .expect("availability zone schema must be valid");
        let zone = zone_schema
            .rootless_type("availability_zone")
            .expect("availability zone type")
            .canonicalize_state(Value::Concrete(ConcreteValue::String(
                "ap-northeast-1a".into(),
            )));
        assert!(matches!(
            &zone,
            Value::Concrete(ConcreteValue::CanonicalEnum(value))
                if value.api_value() == "ap-northeast-1a"
        ));

        let dispatch = carina_core::provider::CustomTypeDispatchTable::build(&[], &zone_schema)
            .expect("availability-zone config schema must build a dispatch table");

        dispatch
            .dispatch(&identity, "ap-northeast-1a")
            .expect("canonical API spelling should validate");
        assert!(
            dispatch.dispatch(&identity, "not-an-az").is_err(),
            "the host entry point must find the structured AZ validator"
        );
    }
}
