//! AWS Provider normalizer

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, BoxFuture, ProviderNormalizer, SavedAttrs, ready_noop};
use carina_core::resource::{Resource, ResourceId, State, Value};
use carina_core::schema::SchemaRegistry;

/// Schema extension for the AWS provider.
///
/// Handles provider-local desired-state normalization.
pub struct AwsNormalizer;

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(&'a self, resources: &'a mut [Resource]) -> BoxFuture<'a, ()> {
        // Bodies are pure (dns-name strip — no I/O); the trait is async only
        // so the WASM host impl can `.await` the guest directly (carina#3112).
        Box::pin(async move {
            crate::services::route53::record_set::normalize_record_set_dns_names(resources);
        })
    }

    fn normalize_state<'a>(
        &'a self,
        _current_states: &'a mut HashMap<ResourceId, State>,
    ) -> BoxFuture<'a, ()> {
        ready_noop()
    }

    fn hydrate_read_state<'a>(
        &'a self,
        _current_states: &'a mut HashMap<ResourceId, State>,
        _saved_attrs: &'a SavedAttrs,
    ) -> BoxFuture<'a, ()> {
        ready_noop()
    }

    fn merge_default_tags<'a>(
        &'a self,
        resources: &'a mut [Resource],
        default_tags: &'a IndexMap<String, Value>,
        registry: &'a SchemaRegistry,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            provider::merge_default_tags_for_provider("aws", resources, default_tags, registry);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::ConcreteValue;

    #[tokio::test]
    async fn normalize_desired_does_not_mutate_enum_typed_attributes() {
        let mut resource = Resource::with_provider("aws", "ec2.Subnet", "test-subnet", None);
        resource.set_attr(
            "availability_zone".to_string(),
            Value::Concrete(ConcreteValue::String("ap-northeast-1a".to_string())),
        );
        resource.set_attr(
            "private_dns_name_options_on_launch".to_string(),
            Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "hostname_type".to_string(),
                Value::Concrete(ConcreteValue::String("ip-name".to_string())),
            )]))),
        );
        let mut resources = vec![resource];

        AwsNormalizer.normalize_desired(&mut resources).await;

        assert_eq!(
            resources[0].get_attr("availability_zone"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ap-northeast-1a".to_string()
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
                "ip-name".to_string()
            )))
        );
    }

    #[tokio::test]
    async fn normalize_desired_still_strips_record_set_trailing_dot() {
        let mut resource = Resource::with_provider("aws", "route53.RecordSet", "test-rec", None);
        resource.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String("_abc.example.com.".to_string())),
        );
        let mut resources = vec![resource];

        AwsNormalizer.normalize_desired(&mut resources).await;

        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string()
            )))
        );
    }

    #[tokio::test]
    async fn normalize_state_keeps_raw_api_enum_spellings() {
        let mut inner = IndexMap::new();
        inner.insert(
            "hostname_type".to_string(),
            Value::Concrete(ConcreteValue::String("ip-name".to_string())),
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

        AwsNormalizer.normalize_state(&mut states).await;
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
                    "ip-name".to_string()
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
