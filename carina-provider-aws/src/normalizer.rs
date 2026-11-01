//! AWS Provider normalizer

use carina_core::provider::{
    BoxFuture, ProviderNormalizer, ProviderResult, SavedAttrs, StateTransformMap, ready_noop,
};
use carina_core::resource::{ProviderResource, ResourceId};
use carina_plugin_sdk::{DesiredView, NativeDesiredNormalization};

type DesiredNormalizer = fn(&mut DesiredView<'_>) -> ProviderResult<()>;

// This dispatch is also the admission check before projecting native resources.
fn desired_normalizer(id: &ResourceId) -> Option<DesiredNormalizer> {
    if id.provider != "aws" {
        return None;
    }
    match id.resource_type.as_str() {
        "route53.RecordSet" => Some(|view| {
            crate::services::route53::record_set::normalize_record_set_dns_names(view);
            Ok(())
        }),
        "acm.Certificate" => Some(|view| {
            crate::services::acm::certificate::normalize_acm_certificate_inputs(view);
            Ok(())
        }),
        "s3.BucketReplicationConfiguration" => Some(|view| {
            crate::services::s3::bucket_replication::normalize_delete_marker_replication(view);
            Ok(())
        }),
        "ec2.Subnet" => Some(crate::services::ec2::subnet::normalize_subnet_ipv6_cidr_blocks),
        "ec2.NatGateway" => Some(|view| {
            crate::services::ec2::nat_gateway::validate_nat_gateway_inputs(&view.inputs())
        }),
        "sqs.Queue" => {
            Some(|view| crate::services::sqs::queue::validate_sqs_queue_inputs(&view.inputs()))
        }
        _ => None,
    }
}

/// Provider-local desired-state normalization for native and WASM resources.
pub struct AwsNormalizer;

impl AwsNormalizer {
    /// Apply the same leaf-wise normalizers and validators on either SDK route.
    #[doc(hidden)]
    pub fn normalize_desired_view(&self, view: &mut DesiredView<'_>) -> ProviderResult<()> {
        if let Some(normalize) = desired_normalizer(view.id()) {
            normalize(view)?;
        }
        Ok(())
    }
}

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(
        &'a self,
        resources: &'a mut [ProviderResource],
    ) -> BoxFuture<'a, ProviderResult<()>> {
        // Bodies are pure; the trait is async for the core normalizer interface.
        Box::pin(async move {
            for resource in resources {
                let Some(normalize) = desired_normalizer(&resource.id) else {
                    continue;
                };
                let mut desired = NativeDesiredNormalization::from_core(resource)?;
                normalize(&mut desired.view())?;
                desired.finish()?;
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
    use carina_core::resource::{ConcreteValue, DeferredValue, Resource, Value};
    use indexmap::IndexMap;

    fn string(value: &str) -> Value {
        Value::Concrete(ConcreteValue::String(value.into()))
    }

    fn secret(value: Value) -> Value {
        Value::Deferred(DeferredValue::Secret(Box::new(value)))
    }

    fn reference(attribute: &str) -> Value {
        Value::resource_ref("peer", attribute, vec![]).resolve_root_for_test()
    }

    fn map(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Concrete(ConcreteValue::Map(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        ))
    }

    fn normalize_mixed(mut resource: Resource) -> ProviderResource {
        let mut desired = NativeDesiredNormalization::from_core(&mut resource)
            .expect("mixed desired input should project into a structural view");
        AwsNormalizer
            .normalize_desired_view(&mut desired.view())
            .expect("mixed desired normalization should succeed");
        desired
            .finish()
            .expect("deferred leaves must survive exactly once");
        resource.into()
    }

    #[tokio::test]
    async fn unhandled_resources_bypass_desired_projection() {
        use carina_core::resource::RawEnumIdentifier;

        for (provider, resource_type) in [("aws", "s3.Bucket"), ("awscc", "acm.Certificate")] {
            let value = Value::Concrete(ConcreteValue::EnumIdentifier(RawEnumIdentifier::parse(
                "raw",
            )));
            let mut resources = vec![
                Resource::with_provider(provider, resource_type, "untouched", None)
                    .with_attribute("unrelated", value.clone())
                    .into(),
            ];

            AwsNormalizer
                .normalize_desired(&mut resources)
                .await
                .expect("an unhandled resource must not be projected or normalized");

            assert_eq!(resources[0].get_attr("unrelated"), Some(&value));
        }
    }

    #[tokio::test]
    #[ignore = "pending carina DesiredInputs scoping decision"]
    async fn crud_inputs_do_not_reject_unrelated_values_admitted_by_core_preparation() {
        use carina_core::executor::{
            ModuleConstraintGate, ProviderPreparationContext, prepare_create_request,
        };
        use carina_core::parser::ProviderConfig;
        use carina_core::provider::ProviderFactory;
        use carina_plugin_sdk::DesiredInputs;

        let configs = [ProviderConfig {
            name: "aws".into(),
            attributes: IndexMap::new(),
            default_tags: IndexMap::from([(
                "unrelated".into(),
                Value::Concrete(ConcreteValue::Duration(std::time::Duration::from_secs(
                    u64::MAX,
                ))),
            )]),
            source: None,
            version: None,
            revision: None,
            unresolved_attributes: IndexMap::new(),
            binding: None,
        }];
        let bindings = carina_core::binding_index::ResolvedBindings::default();
        let module_gate = ModuleConstraintGate::from_live_compositions(&[]);
        let schemas = crate::schemas::schema_registry();
        let factories: Vec<Box<dyn ProviderFactory>> = vec![Box::new(crate::AwsProviderFactory)];
        let preparation = ProviderPreparationContext::new(
            &bindings,
            &module_gate,
            &configs,
            &AwsNormalizer,
            &factories,
            &schemas,
        );
        let resource = Resource::with_provider("aws", "sqs.Queue", "crud-projection", None)
            .with_attribute("queue_name", string("crud-projection"));
        let request = prepare_create_request(resource, &preparation)
            .await
            .expect("the actual core preparation gate admits the resource");

        let inputs = DesiredInputs::from_core(request.resource().as_resource());
        assert!(
            inputs.is_ok(),
            "CRUD validator projection rejected an unrelated default tag: {:?}",
            inputs.err()
        );
    }

    #[test]
    fn mixed_acm_sans_normalize_known_names_and_retain_secret_and_ref() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "mixed-sans", None);
        let secret = secret(string("PRIVATE.EXAMPLE.COM."));
        let reference = reference("domain_name");
        resource.set_attr("domain_name", string("Example.COM."));
        resource.set_attr(
            "subject_alternative_names",
            Value::Concrete(ConcreteValue::List(vec![
                string("EXAMPLE.COM."),
                secret.clone(),
                string("WWW.EXAMPLE.COM.."),
                reference.clone(),
                string("API.EXAMPLE.COM."),
            ])),
        );

        let normalized = normalize_mixed(resource);

        assert_eq!(
            normalized.get_attr("subject_alternative_names"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                secret,
                string("www.example.com"),
                reference,
                string("api.example.com"),
            ])))
        );
    }

    #[test]
    fn mixed_replication_rules_get_defaults_beside_secret_and_ref() {
        let mut resource = Resource::with_provider(
            "aws",
            "s3.BucketReplicationConfiguration",
            "mixed-rules",
            None,
        );
        let rules = vec![
            map([("id", secret(string("private-rule")))]),
            map([("destination", map([("bucket", reference("bucket_arn"))]))]),
            map([("id", string("known-rule"))]),
            map([(
                "delete_marker_replication",
                map([("status", string("Enabled"))]),
            )]),
            map([(
                "delete_marker_replication",
                reference("delete_marker_replication"),
            )]),
            map([(
                "delete_marker_replication",
                secret(map([("status", string("Enabled"))])),
            )]),
            secret(map([("id", string("opaque-rule"))])),
            reference("rule"),
        ];
        resource.set_attr("rules", Value::Concrete(ConcreteValue::List(rules.clone())));

        let normalized = normalize_mixed(resource);

        let mut expected = rules;
        for rule in &mut expected[..3] {
            let Value::Concrete(ConcreteValue::Map(fields)) = rule else {
                panic!("fixture must contain structural rule maps");
            };
            fields.insert(
                "delete_marker_replication".into(),
                map([("status", string("Disabled"))]),
            );
        }
        assert_eq!(
            normalized.get_attr("rules"),
            Some(&Value::Concrete(ConcreteValue::List(expected)))
        );
    }

    #[test]
    fn mixed_route53_alias_normalizes_dns_name_beside_secret_and_ref() {
        let mut resource = Resource::with_provider("aws", "route53.RecordSet", "mixed-alias", None);
        let zone = reference("hosted_zone_id");
        let health = secret(Value::Concrete(ConcreteValue::Bool(false)));
        resource.set_attr("name", string("www.example.com."));
        resource.set_attr(
            "alias_target",
            map([
                ("dns_name", string("lb.example.com.")),
                ("hosted_zone_id", zone.clone()),
                ("evaluate_target_health", health.clone()),
            ]),
        );

        let normalized = normalize_mixed(resource);

        assert_eq!(
            normalized.get_attr("name"),
            Some(&string("www.example.com"))
        );
        assert_eq!(
            normalized.get_attr("alias_target"),
            Some(&map([
                ("dns_name", string("lb.example.com")),
                ("hosted_zone_id", zone),
                ("evaluate_target_health", health),
            ]))
        );
    }

    #[test]
    fn deferred_acm_apex_does_not_hide_known_sans_or_guess_apex_removal() {
        for domain in [secret(string("EXAMPLE.COM.")), reference("domain_name")] {
            let mut resource =
                Resource::with_provider("aws", "acm.Certificate", "opaque-apex", None);
            resource.set_attr("domain_name", domain.clone());
            resource.set_attr(
                "subject_alternative_names",
                Value::Concrete(ConcreteValue::List(vec![
                    string("EXAMPLE.COM."),
                    string("WWW.EXAMPLE.COM."),
                ])),
            );

            let normalized = normalize_mixed(resource);

            assert_eq!(normalized.get_attr("domain_name"), Some(&domain));
            assert_eq!(
                normalized.get_attr("subject_alternative_names"),
                Some(&Value::Concrete(ConcreteValue::List(vec![
                    string("example.com"),
                    string("www.example.com"),
                ])))
            );
        }
    }

    #[test]
    fn unknown_san_survives_known_apex_removal() {
        use carina_core::resource::{ApplyTimeUnknown, absolute_binding_address_for_test};

        let mut resource = Resource::with_provider("aws", "acm.Certificate", "unknown-san", None);
        let binding = absolute_binding_address_for_test("peer");
        resource.set_attr("domain_name", string("example.com"));
        resource.set_attr(
            "subject_alternative_names",
            Value::Concrete(ConcreteValue::List(vec![
                string("EXAMPLE.COM."),
                Value::Deferred(DeferredValue::Unknown(
                    ApplyTimeUnknown::UpstreamAllExports {
                        binding: binding.clone(),
                    },
                )),
                string("WWW.EXAMPLE.COM."),
            ])),
        );

        let normalized = normalize_mixed(resource);

        let Some(Value::Concrete(ConcreteValue::List(names))) =
            normalized.get_attr("subject_alternative_names")
        else {
            panic!("expected normalized SAN list");
        };
        let [
            Value::Deferred(DeferredValue::Unknown(ApplyTimeUnknown::UpstreamAllExports {
                binding: actual,
            })),
            known,
        ] = names.as_slice()
        else {
            panic!("the unknown SAN must precede the canonical known SAN");
        };
        assert_eq!(actual, &binding);
        assert_eq!(known, &string("www.example.com"));
    }

    #[test]
    fn cross_field_checks_keep_secrets_opaque_during_normalization_and_crud_validation() {
        use carina_plugin_sdk::DesiredInputs;

        for (resource_type, attributes) in [
            (
                "ec2.NatGateway",
                vec![("availability_mode", secret(string("regional")))],
            ),
            (
                "ec2.NatGateway",
                vec![
                    ("availability_mode", string("regional")),
                    ("subnet_id", secret(string("subnet-12345678"))),
                ],
            ),
            (
                "sqs.Queue",
                vec![
                    (
                        "fifo_queue",
                        secret(Value::Concrete(ConcreteValue::Bool(true))),
                    ),
                    ("deduplication_scope", string("messageGroup")),
                ],
            ),
            (
                "sqs.Queue",
                vec![
                    (
                        "kms_data_key_reuse_period_seconds",
                        Value::Concrete(ConcreteValue::Int(300)),
                    ),
                    ("kms_master_key_id", secret(string("alias/private"))),
                ],
            ),
            (
                "sqs.Queue",
                vec![(
                    "kms_data_key_reuse_period_seconds",
                    secret(Value::Concrete(ConcreteValue::Int(300))),
                )],
            ),
            (
                "sqs.Queue",
                vec![
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(false))),
                    (
                        "content_based_deduplication",
                        secret(Value::Concrete(ConcreteValue::Bool(true))),
                    ),
                ],
            ),
        ] {
            let mut resource = Resource::with_provider("aws", resource_type, "opaque-check", None);
            for (name, value) in attributes {
                resource.set_attr(name, value);
            }
            let original = resource.attributes.clone();
            let inputs = DesiredInputs::from_core(&resource).expect("CRUD validation projection");
            crate::services::ec2::nat_gateway::validate_nat_gateway_inputs(&inputs).unwrap();
            crate::services::sqs::queue::validate_sqs_queue_inputs(&inputs).unwrap();
            let normalized = normalize_mixed(resource);
            assert_eq!(normalized.attributes, original);
        }
    }

    #[test]
    fn opaque_top_level_inputs_are_preserved_without_normalization() {
        for (resource_type, attribute, value) in [
            (
                "acm.Certificate",
                "subject_alternative_names",
                secret(Value::Concrete(ConcreteValue::List(vec![string(
                    "EXAMPLE.COM.",
                )]))),
            ),
            (
                "s3.BucketReplicationConfiguration",
                "rules",
                secret(Value::Concrete(ConcreteValue::List(vec![map([(
                    "id",
                    string("private"),
                )])]))),
            ),
            (
                "route53.RecordSet",
                "alias_target",
                secret(map([("dns_name", string("PRIVATE."))])),
            ),
            (
                "ec2.Subnet",
                "ipv6_cidr_block",
                secret(string("invalid private CIDR")),
            ),
            (
                "ec2.Subnet",
                "ipv6_cidr_block",
                reference("ipv6_cidr_block"),
            ),
        ] {
            let mut resource = Resource::with_provider("aws", resource_type, "opaque", None);
            resource.set_attr(attribute, value.clone());
            let normalized = normalize_mixed(resource);
            assert_eq!(normalized.get_attr(attribute), Some(&value));
        }
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
