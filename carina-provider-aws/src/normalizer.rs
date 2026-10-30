//! AWS Provider normalizer

use carina_core::provider::{
    BoxFuture, ProviderNormalizer, ProviderResult, SavedAttrs, StateTransformMap, ready_noop,
};
use carina_core::resource::{ConcreteValue, ProviderResource, Resource, ResourceId, Value};

#[derive(Clone, Copy)]
enum DesiredAttributeVisibility {
    Complete,
    DeferredAttributesHidden,
}

/// What a provider-side cross-field check can prove about one input.
///
/// A WASM normalizer receives an ordinary-value projection. The host removes
/// every top-level attribute containing a deferred value before the call and
/// restores it afterward, so a missing wire attribute is not proof that the
/// user omitted it. Keeping that distinction in this type prevents individual
/// validators from accidentally treating transport-hidden values as absent.
pub(crate) enum CrossFieldValue<'a> {
    Known(&'a ConcreteValue),
    Absent,
    Deferred,
}

pub(crate) struct CrossFieldInputs<'a, S> {
    resource: &'a Resource<S>,
    visibility: DesiredAttributeVisibility,
}

impl<'a, S> CrossFieldInputs<'a, S> {
    pub(crate) fn complete(resource: &'a Resource<S>) -> Self {
        Self {
            resource,
            visibility: DesiredAttributeVisibility::Complete,
        }
    }

    fn for_normalizer(resource: &'a Resource<S>, visibility: DesiredAttributeVisibility) -> Self {
        Self {
            resource,
            visibility,
        }
    }

    pub(crate) fn is_resource(&self, provider: &str, resource_type: &str) -> bool {
        self.resource.id.provider == provider && self.resource.id.resource_type == resource_type
    }

    pub(crate) fn id(&self) -> &ResourceId {
        &self.resource.id
    }

    pub(crate) fn get(&self, name: &str) -> CrossFieldValue<'_> {
        match self.resource.get_attr(name) {
            Some(Value::Concrete(value)) => CrossFieldValue::Known(value),
            Some(Value::Deferred(_)) => CrossFieldValue::Deferred,
            None if matches!(self.visibility, DesiredAttributeVisibility::Complete) => {
                CrossFieldValue::Absent
            }
            None => CrossFieldValue::Deferred,
        }
    }
}

/// Schema extension for the AWS provider.
///
/// Handles provider-local desired-state normalization.
pub struct AwsNormalizer;

impl AwsNormalizer {
    fn normalize_desired_with_visibility<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
        visibility: DesiredAttributeVisibility,
    ) -> BoxFuture<'a, ProviderResult<()>> {
        Box::pin(async move {
            crate::services::route53::record_set::normalize_record_set_dns_names(resources);
            crate::services::acm::certificate::normalize_acm_certificate_inputs(resources);
            crate::services::s3::bucket_replication::normalize_delete_marker_replication(resources);
            crate::services::ec2::subnet::normalize_subnet_ipv6_cidr_blocks(resources)?;
            for resource in resources.iter() {
                let inputs = CrossFieldInputs::for_normalizer(resource, visibility);
                crate::services::ec2::nat_gateway::validate_nat_gateway_inputs(&inputs)?;
                crate::services::sqs::queue::validate_sqs_queue_inputs(&inputs)?;
            }
            Ok(())
        })
    }

    /// Normalize the partial ordinary-value projection used by the WASM
    /// desired-state protocol. Missing attributes at this boundary may be
    /// deferred values temporarily hidden by the host.
    #[doc(hidden)]
    pub fn normalize_wire_desired<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
    ) -> BoxFuture<'a, ProviderResult<()>> {
        self.normalize_desired_with_visibility(
            resources,
            DesiredAttributeVisibility::DeferredAttributesHidden,
        )
    }
}

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
    ) -> BoxFuture<'a, ProviderResult<()>> {
        // Bodies are pure (dns-name strip — no I/O); the trait is async only
        // so the WASM host impl can `.await` the guest directly (carina#3112).
        self.normalize_desired_with_visibility(resources, DesiredAttributeVisibility::Complete)
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
    use carina_core::resource::{ConcreteValue, Resource, Value};
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
    async fn normalize_desired_canonicalizes_acm_sans() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".into())),
        );
        resource.set_attr(
            "subject_alternative_names".to_string(),
            Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("EXAMPLE.COM.".into())),
                Value::Concrete(ConcreteValue::String("WWW.EXAMPLE.COM.".into())),
            ])),
        );
        let mut resources = vec![resource.into()];

        AwsNormalizer
            .normalize_desired(&mut resources)
            .await
            .expect("ACM SANs should be canonicalized");
        assert_eq!(
            resources[0].get_attr("subject_alternative_names"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("www.example.com".into()))
            ])))
        );
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
        let mut states = HashMap::from([(
            id.clone(),
            crate::read_model::test_existing_state(id, attributes),
        )]);

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
