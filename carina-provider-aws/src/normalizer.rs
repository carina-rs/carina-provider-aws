//! AWS Provider normalizer

use carina_core::provider::{
    BoxFuture, ProviderNormalizer, ProviderResult, SavedAttrs, StateTransformMap, ready_noop,
};
use carina_core::resource::ProviderResource;

/// Schema extension for the AWS provider.
///
/// Handles provider-local desired-state normalization.
pub struct AwsNormalizer;

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
    ) -> BoxFuture<'a, ProviderResult<()>> {
        // Bodies are pure (dns-name strip — no I/O); the trait is async only
        // so the WASM host impl can `.await` the guest directly (carina#3112).
        Box::pin(async move {
            crate::services::route53::record_set::normalize_record_set_dns_names(resources);
            for resource in resources.iter() {
                crate::services::acm::certificate::validate_acm_certificate_inputs(resource)?;
                crate::services::sqs::queue::validate_sqs_queue_inputs(resource)?;
            }
            Ok(())
        })
    }

    fn normalize_state<'a>(
        &'a self,
        _current_states: &'a mut StateTransformMap,
    ) -> BoxFuture<'a, ProviderResult<()>> {
        ready_noop()
    }

    fn hydrate_read_state<'a>(
        &'a self,
        _current_states: &'a mut StateTransformMap,
        _saved_attrs: &'a SavedAttrs,
    ) -> BoxFuture<'a, ProviderResult<()>> {
        ready_noop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use carina_core::provider::normalize_provider_states;
    use carina_core::resource::{ConcreteValue, Resource, State, Value};
    use indexmap::IndexMap;

    #[tokio::test]
    async fn normalize_desired_does_not_mutate_enum_typed_attributes() {
        let mut resource = Resource::with_provider("aws", "ec2.Subnet", "test-subnet", None);
        resource.set_attr(
            "availability_zone".to_string(),
            Value::Concrete(ConcreteValue::String("ap-northeast-1a".to_string().into())),
        );
        resource.set_attr(
            "private_dns_name_options_on_launch".to_string(),
            Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "hostname_type".to_string(),
                Value::Concrete(ConcreteValue::String("ip-name".to_string().into())),
            )]))),
        );
        let mut resources = vec![resource.into()];

        AwsNormalizer
            .normalize_desired(&mut resources)
            .await
            .expect("desired normalization should succeed");

        assert_eq!(
            resources[0].get_attr("availability_zone"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ap-northeast-1a".to_string().into()
            )))
        );
        let Some(Value::Concrete(ConcreteValue::Map(fields))) =
            resources[0].get_attr("private_dns_name_options_on_launch")
        else {
            panic!("expected private_dns_name_options_on_launch map");
        };
        assert_eq!(
            fields.get("hostname_type"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ip-name".to_string().into()
            )))
        );
    }

    #[tokio::test]
    async fn normalize_desired_still_strips_record_set_trailing_dot() {
        let mut resource = Resource::with_provider("aws", "route53.RecordSet", "test-rec", None);
        resource.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String(
                "_abc.example.com.".to_string().into(),
            )),
        );
        let mut resources = vec![resource.into()];

        AwsNormalizer
            .normalize_desired(&mut resources)
            .await
            .expect("desired normalization should succeed");

        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string().into()
            )))
        );
    }

    #[tokio::test]
    async fn normalize_desired_rejects_primary_domain_in_acm_sans() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".into())),
        );
        resource.set_attr(
            "subject_alternative_names".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("EXAMPLE.COM.".into()),
            )])),
        );
        let mut resources = vec![resource.into()];

        let error = AwsNormalizer
            .normalize_desired(&mut resources)
            .await
            .expect_err("ACM adds the primary domain to SANs automatically");
        assert!(error.to_string().contains("subject_alternative_names"));
        assert!(error.to_string().contains("domain_name"));
    }

    #[tokio::test]
    async fn normalize_state_keeps_raw_api_enum_spellings() {
        let mut inner = IndexMap::new();
        inner.insert(
            "hostname_type".to_string(),
            Value::Concrete(ConcreteValue::String("ip-name".to_string().into())),
        );
        inner.insert(
            "enable_resource_name_dns_a_record".to_string(),
            Value::Concrete(ConcreteValue::Bool(true)),
        );
        let attributes = HashMap::from([(
            "private_dns_name_options_on_launch".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        )]);
        let id = Resource::with_provider("aws", "ec2.Subnet", "test-subnet", None).id;
        let mut states = HashMap::from([(id.clone(), State::existing(id, attributes))]);

        normalize_provider_states(&AwsNormalizer, &mut states)
            .await
            .expect("state normalization should succeed");
        let state = states
            .values_mut()
            .next()
            .expect("state should survive normalize_state");
        let attributes = &mut state.attributes;

        if let Some(Value::Concrete(ConcreteValue::Map(fields))) =
            attributes.get("private_dns_name_options_on_launch")
        {
            assert_eq!(
                fields.get("hostname_type"),
                Some(&Value::Concrete(ConcreteValue::String(
                    "ip-name".to_string().into()
                )))
            );
            assert_eq!(
                fields.get("enable_resource_name_dns_a_record"),
                Some(&Value::Concrete(ConcreteValue::Bool(true)))
            );
        } else {
            panic!("Expected Value::Map for private_dns_name_options_on_launch");
        }
    }
}
