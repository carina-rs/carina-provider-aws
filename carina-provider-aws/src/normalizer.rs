//! AWS Provider normalizer

use std::collections::BTreeSet;

use carina_core::provider::{
    BoxFuture, ProviderNormalizer, ProviderResult, SavedAttrs, StateTransformMap, ready_noop,
};
use carina_core::resource::{ConcreteValue, ProviderResource, Resource, ResourceId, Value};

/// What a provider-side cross-field check can prove about one input.
///
/// Protocol v13 preserves deferred top-level attributes as explicit metadata
/// at the WASM boundary. The typed normalization view consults that metadata
/// without fabricating a core value, so a missing, non-deferred attribute is
/// genuinely absent.
pub(crate) enum CrossFieldValue<'a> {
    Known(&'a ConcreteValue),
    Absent,
    Deferred,
}

pub(crate) struct CrossFieldInputs<'a, S> {
    resource: &'a Resource<S>,
    host_deferred: Option<&'a BTreeSet<String>>,
}

impl<'a, S> CrossFieldInputs<'a, S> {
    fn at_desired_normalization(
        resource: &'a Resource<S>,
        host_deferred: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            resource,
            host_deferred: Some(host_deferred),
        }
    }

    pub(crate) fn resolved(resource: &'a Resource<S>) -> Self {
        Self {
            resource,
            host_deferred: None,
        }
    }

    pub(crate) fn is_resource(&self, provider: &str, resource_type: &str) -> bool {
        self.resource.id.provider == provider && self.resource.id.resource_type == resource_type
    }

    pub(crate) fn id(&self) -> &ResourceId {
        &self.resource.id
    }

    pub(crate) fn attr(&self, name: &str) -> CrossFieldValue<'_> {
        if self
            .host_deferred
            .is_some_and(|deferred| deferred.contains(name))
        {
            return CrossFieldValue::Deferred;
        }
        match self.resource.get_attr(name) {
            Some(Value::Concrete(value)) => CrossFieldValue::Known(value),
            Some(Value::Deferred(_)) => CrossFieldValue::Deferred,
            None => CrossFieldValue::Absent,
        }
    }
}

/// One resource admitted to desired normalization.
///
/// Host-withheld attributes remain metadata rather than fabricated core
/// values. Normalizers can mutate only attributes that are present and
/// concrete, so neither a host-deferred name nor an in-process deferred value
/// can be overwritten or used as a provider value.
pub(crate) struct DesiredNormalizationInput<'a, S> {
    resource: &'a mut Resource<S>,
    host_deferred: Option<&'a BTreeSet<String>>,
}

impl<'a, S> DesiredNormalizationInput<'a, S> {
    pub(crate) fn in_process(resource: &'a mut Resource<S>) -> Self {
        Self {
            resource,
            host_deferred: None,
        }
    }

    fn from_host(resource: &'a mut Resource<S>, host_deferred: &'a BTreeSet<String>) -> Self {
        Self {
            resource,
            host_deferred: Some(host_deferred),
        }
    }

    pub(crate) fn is_resource(&self, provider: &str, resource_type: &str) -> bool {
        self.resource.id.provider == provider && self.resource.id.resource_type == resource_type
    }

    pub(crate) fn id(&self) -> &ResourceId {
        &self.resource.id
    }

    fn is_host_deferred(&self, name: &str) -> bool {
        self.host_deferred
            .is_some_and(|deferred| deferred.contains(name))
    }

    pub(crate) fn known_attr(&self, name: &str) -> Option<&ConcreteValue> {
        if self.is_host_deferred(name) {
            return None;
        }
        match self.resource.get_attr(name) {
            Some(Value::Concrete(value)) => Some(value),
            Some(Value::Deferred(_)) | None => None,
        }
    }

    pub(crate) fn known_attr_mut(&mut self, name: &str) -> Option<&mut ConcreteValue> {
        if self.is_host_deferred(name) {
            return None;
        }
        match self.resource.get_attr_mut(name) {
            Some(Value::Concrete(value)) => Some(value),
            Some(Value::Deferred(_)) | None => None,
        }
    }

    fn cross_field_inputs(&self) -> CrossFieldInputs<'_, S> {
        match self.host_deferred {
            Some(host_deferred) => {
                CrossFieldInputs::at_desired_normalization(self.resource, host_deferred)
            }
            None => CrossFieldInputs::resolved(self.resource),
        }
    }
}

fn normalize_desired_input<S>(input: &mut DesiredNormalizationInput<'_, S>) -> ProviderResult<()> {
    crate::services::route53::record_set::normalize_record_set_dns_names(input);
    crate::services::acm::certificate::normalize_acm_certificate_inputs(input);
    crate::services::s3::bucket_replication::normalize_delete_marker_replication(input);
    crate::services::ec2::subnet::normalize_subnet_ipv6_cidr_blocks(input)?;

    let cross_fields = input.cross_field_inputs();
    crate::services::ec2::nat_gateway::validate_nat_gateway_inputs(&cross_fields)?;
    crate::services::sqs::queue::validate_sqs_queue_inputs(&cross_fields)?;
    Ok(())
}

/// Schema extension for the AWS provider.
///
/// Handles provider-local desired-state normalization.
pub struct AwsNormalizer;

impl AwsNormalizer {
    /// Normalize a protocol-v13 desired projection whose deferred top-level
    /// values remain owned by the host.
    #[doc(hidden)]
    pub fn normalize_desired_with_host_deferred(
        &self,
        resources: &mut [ProviderResource],
        deferred_by_resource: &[BTreeSet<String>],
    ) -> ProviderResult<()> {
        if resources.len() != deferred_by_resource.len() {
            return Err(carina_core::provider::ProviderError::internal(format!(
                "desired normalization received {} resources but {} deferred-name sets",
                resources.len(),
                deferred_by_resource.len()
            )));
        }

        for (resource, host_deferred) in resources.iter_mut().zip(deferred_by_resource) {
            let mut input = DesiredNormalizationInput::from_host(resource, host_deferred);
            normalize_desired_input(&mut input)?;
        }
        Ok(())
    }
}

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
    ) -> BoxFuture<'a, ProviderResult<()>> {
        // Bodies are pure (dns-name strip — no I/O); the trait is async for
        // compatibility with the core normalizer interface.
        Box::pin(async move {
            for resource in resources {
                let mut input = DesiredNormalizationInput::in_process(resource);
                normalize_desired_input(&mut input)?;
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
    use carina_core::resource::{ConcreteValue, Resource, Value};
    use indexmap::IndexMap;

    #[test]
    fn host_deferred_metadata_is_deferred_without_a_core_value_or_write_handle() {
        let mut resource: Resource =
            Resource::with_provider("aws", "ec2.NatGateway", "deferred-subnet", None);
        let deferred = BTreeSet::from(["subnet_id".to_string()]);
        assert!(!resource.attributes.contains_key("subnet_id"));

        {
            let mut input = DesiredNormalizationInput::from_host(&mut resource, &deferred);
            assert!(matches!(
                input.cross_field_inputs().attr("subnet_id"),
                CrossFieldValue::Deferred
            ));
            assert!(input.known_attr_mut("subnet_id").is_none());
        }

        assert!(
            !resource.attributes.contains_key("subnet_id"),
            "host-deferred metadata must not fabricate a core value"
        );
    }

    #[test]
    fn host_deferred_rules_do_not_receive_replication_defaults() {
        let resource: Resource = Resource::with_provider(
            "aws",
            "s3.BucketReplicationConfiguration",
            "deferred-rules",
            None,
        );
        let mut resources = vec![ProviderResource::from(resource)];
        let deferred = vec![BTreeSet::from(["rules".to_string()])];

        AwsNormalizer
            .normalize_desired_with_host_deferred(&mut resources, &deferred)
            .expect("a host-deferred rules list must bypass nested defaulting");

        assert!(
            !resources[0].attributes.contains_key("rules"),
            "the delete-marker default must not materialize deferred rules"
        );
    }

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
        let resource: Resource = Resource::with_provider("aws", "ec2.Subnet", "test-subnet", None);
        let id = resource.id;
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
