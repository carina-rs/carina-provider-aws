//! AWS Provider normalizer

use carina_core::provider::{
    BoxFuture, ProviderError, ProviderNormalizer, ProviderResult, SavedAttrs, StateTransformMap,
    ready_noop,
};
use carina_core::resource::{ProviderResource, ResourceId};
use carina_plugin_sdk::{
    DesiredMap, DesiredScalar, DesiredValue, DesiredView, NativeDesiredNormalization,
};

type DesiredNormalizer = fn(&mut DesiredView<'_>) -> ProviderResult<()>;

// This dispatch is also the admission check before projecting native resources.
fn desired_normalizer(id: &ResourceId) -> Option<DesiredNormalizer> {
    if id.provider != "aws" {
        return None;
    }
    match id.resource_type.as_str() {
        "route53.RecordSet" => Some(validate_route53_record_set_view),
        "acm.Certificate" => {
            Some(crate::services::acm::certificate::normalize_acm_certificate_inputs)
        }
        "s3.BucketReplicationConfiguration" => Some(validate_s3_replication_view),
        "s3.BucketLifecycleConfiguration" => Some(validate_s3_lifecycle_view),
        "s3.BucketLogging" => Some(validate_s3_logging_view),
        "s3.BucketWebsiteConfiguration" => Some(validate_s3_website_view),
        "s3.BucketNotificationConfiguration" => Some(validate_s3_notification_view),
        "ec2.Subnet" => Some(normalize_subnet_view),
        "ec2.FlowLog" => Some(validate_flow_log_view),
        "ec2.Vpc" => Some(validate_vpc_view),
        "ec2.NatGateway" => Some(validate_nat_gateway_view),
        "sqs.Queue" => Some(validate_sqs_queue_view),
        _ => None,
    }
}

fn validate_route53_record_set_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    crate::services::route53::record_set::validate_record_set_inputs(&view.inputs())?;
    crate::services::route53::record_set::normalize_record_set_dns_names(view)
}

fn known_string(value: Option<&DesiredValue>) -> Option<&str> {
    match value {
        Some(DesiredValue::Known(value)) => value.as_str(),
        _ => None,
    }
}

fn known_map(value: Option<&DesiredValue>) -> Option<&DesiredMap> {
    match value {
        Some(DesiredValue::Map(value)) => Some(value),
        _ => None,
    }
}

fn known_int(value: Option<&DesiredValue>) -> Option<i64> {
    match value {
        Some(DesiredValue::Known(DesiredScalar::Int(value))) => Some(*value),
        _ => None,
    }
}

fn known_present(value: Option<&DesiredValue>) -> bool {
    matches!(
        value,
        Some(DesiredValue::Known(_) | DesiredValue::Map(_) | DesiredValue::List(_))
    )
}

fn s3_filter_has_tags(filter: &DesiredMap) -> bool {
    known_map(filter.get("tag")).is_some()
        || known_map(filter.get("and")).is_some_and(|and| {
            matches!(and.get("tags"), Some(DesiredValue::List(tags)) if tags.iter().any(|tag| known_map(Some(tag)).is_some()))
        })
}

fn validate_s3_replication_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_ReplicationRule.html
    let inputs = view.inputs();
    let invalid = |message| ProviderError::invalid_input(message).for_resource(inputs.id().clone());
    if let Some(DesiredValue::List(rules)) = inputs.attribute("rules") {
        for rule in rules.iter().filter_map(|value| known_map(Some(value))) {
            if let Some(filter) = known_map(rule.get("filter")) {
                if ["prefix", "tag", "and"]
                    .iter()
                    .filter(|field| known_present(filter.get(field)))
                    .count()
                    > 1
                {
                    return Err(invalid(
                        "s3.BucketReplicationConfiguration rules[].filter must select at most one of prefix, tag or and; an empty filter matches all objects",
                    ));
                }
                if s3_filter_has_tags(filter)
                    && known_map(rule.get("delete_marker_replication"))
                        .and_then(|marker| known_string(marker.get("status")))
                        == Some("Enabled")
                {
                    return Err(invalid(
                        "s3.BucketReplicationConfiguration delete_marker_replication must be Disabled with tag filters",
                    ));
                }
            }
        }
    }
    crate::services::s3::bucket_replication::normalize_delete_marker_replication(view)
}

fn validate_s3_lifecycle_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    let inputs = view.inputs();
    let invalid = |message| ProviderError::invalid_input(message).for_resource(inputs.id().clone());
    let Some(DesiredValue::List(rules)) = inputs.attribute("rules") else {
        return Ok(());
    };
    for rule in rules.iter().filter_map(|rule| known_map(Some(rule))) {
        // Days and Date are alternative clocks for current-version actions.
        // https://docs.aws.amazon.com/AmazonS3/latest/userguide/lifecycle-configuration-examples.html
        // ExpiredObjectDeleteMarker cannot accompany Days or Date, even false.
        // https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleExpiration.html
        let expiration = known_map(rule.get("expiration"));
        let mut has_days = expiration.is_some_and(|action| known_present(action.get("days")));
        let mut has_date = expiration.is_some_and(|action| known_present(action.get("date")));
        if let Some(expiration) = expiration {
            if known_present(expiration.get("days")) && known_present(expiration.get("date")) {
                return Err(invalid(
                    "s3.BucketLifecycleConfiguration rules[].expiration cannot combine days and date",
                ));
            }
            if known_present(expiration.get("expired_object_delete_marker"))
                && (known_present(expiration.get("days")) || known_present(expiration.get("date")))
            {
                return Err(invalid(
                    "s3.BucketLifecycleConfiguration rules[].expiration.expired_object_delete_marker cannot combine with days or date",
                ));
            }
        }
        if let Some(DesiredValue::List(transitions)) = rule.get("transitions") {
            for transition in transitions
                .iter()
                .filter_map(|value| known_map(Some(value)))
            {
                has_days |= known_present(transition.get("days"));
                has_date |= known_present(transition.get("date"));
                if known_present(transition.get("days")) && known_present(transition.get("date")) {
                    return Err(invalid(
                        "s3.BucketLifecycleConfiguration rules[].transitions[] cannot combine days and date",
                    ));
                }
            }
        }
        if has_days && has_date {
            return Err(invalid(
                "s3.BucketLifecycleConfiguration rules[] cannot mix days and date across expiration and transitions",
            ));
        }
        // No independent size cap: '50 TB' is not defined exactly for filters.
        // The ordering is explicit and independent of that unresolved cap.
        // https://docs.aws.amazon.com/AmazonS3/latest/userguide/intro-lifecycle-filters.html
        if let Some(filter) = known_map(rule.get("filter")) {
            // https://docs.aws.amazon.com/AmazonS3/latest/userguide/intro-lifecycle-rules.html
            if s3_filter_has_tags(filter)
                && (known_present(rule.get("abort_incomplete_multipart_upload"))
                    || expiration.is_some_and(|action| {
                        known_present(action.get("expired_object_delete_marker"))
                    }))
            {
                return Err(invalid(
                    "s3.BucketLifecycleConfiguration tag filters cannot combine with abort_incomplete_multipart_upload or expired_object_delete_marker",
                ));
            }
            for bounds in [Some(filter), known_map(filter.get("and"))]
                .into_iter()
                .flatten()
            {
                if let (Some(lower), Some(upper)) = (
                    known_int(bounds.get("object_size_greater_than")),
                    known_int(bounds.get("object_size_less_than")),
                ) && lower >= upper
                {
                    return Err(invalid(
                        "s3.BucketLifecycleConfiguration filter object_size_greater_than must be less than object_size_less_than",
                    ));
                }
            }
        }
        // NewerNoncurrentVersions requires Filter on the wire; build_rule
        // already supplies Filter={} when authored input omits it.
    }
    Ok(())
}

fn validate_s3_logging_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_TargetObjectKeyFormat.html
    let inputs = view.inputs();
    if let Some(format) = known_map(inputs.attribute("target_object_key_format")) {
        let simple = format.get("simple_prefix");
        let partitioned = format.get("partitioned_prefix");
        if (simple.is_none() && partitioned.is_none())
            || (known_present(simple) && known_present(partitioned))
        {
            return Err(ProviderError::invalid_input("s3.BucketLogging target_object_key_format must select exactly one of simple_prefix or partitioned_prefix").for_resource(inputs.id().clone()));
        }
    }
    Ok(())
}

fn validate_s3_website_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_WebsiteConfiguration.html
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutBucketWebsite.html
    let inputs = view.inputs();
    let redirect = inputs.attribute("redirect_all_requests_to");
    if known_present(redirect)
        && ["index_document", "error_document"]
            .iter()
            .any(|field| known_present(inputs.attribute(field)))
    {
        return Err(ProviderError::invalid_input("s3.BucketWebsiteConfiguration redirect_all_requests_to cannot combine with index_document or error_document").for_resource(inputs.id().clone()));
    }
    if redirect.is_none() && inputs.attribute("index_document").is_none() {
        return Err(ProviderError::invalid_input("s3.BucketWebsiteConfiguration index_document is required unless redirect_all_requests_to is supplied").for_resource(inputs.id().clone()));
    }
    Ok(())
}

struct S3NotificationScope {
    events: Vec<String>,
    prefix: String,
    suffix: String,
}

fn s3_notification_scope(configuration: &DesiredMap) -> Option<S3NotificationScope> {
    let Some(DesiredValue::List(events)) = configuration.get("events") else {
        return None;
    };
    let events = events
        .iter()
        .filter_map(|value| known_string(Some(value)))
        .map(str::to_string)
        .collect();
    let mut prefix = String::new();
    let mut suffix = String::new();
    match configuration.get("filter") {
        None => {}
        Some(DesiredValue::Map(filter)) => match filter.get("filter_rules") {
            None => {}
            Some(DesiredValue::List(rules)) => {
                for rule in rules.iter() {
                    let rule = known_map(Some(rule))?;
                    let name = known_string(rule.get("name"))?;
                    let value = known_string(rule.get("value"))?;
                    // Compare logical key prefixes/suffixes, including encoded
                    // spaces. Leave admitted read/write encoding forms to AWS.
                    let value = value.replace('+', " ");
                    let decoded = urlencoding::decode(&value).ok()?.into_owned();
                    match name {
                        "prefix" => prefix = decoded,
                        "suffix" => suffix = decoded,
                        _ => return None,
                    }
                }
            }
            _ => return None,
        },
        _ => return None,
    }
    Some(S3NotificationScope {
        events,
        prefix,
        suffix,
    })
}

fn validate_s3_notification_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    // Overlap is forbidden when the same event can reach both configurations.
    // https://docs.aws.amazon.com/AmazonS3/latest/userguide/notification-how-to-filtering.html
    let inputs = view.inputs();
    let mut scopes: Vec<S3NotificationScope> = Vec::new();
    for attribute in [
        "topic_configurations",
        "queue_configurations",
        "lambda_function_configurations",
    ] {
        let Some(DesiredValue::List(configurations)) = inputs.attribute(attribute) else {
            continue;
        };
        for configuration in configurations
            .iter()
            .filter_map(|value| known_map(Some(value)))
        {
            let Some(scope) = s3_notification_scope(configuration) else {
                continue;
            };
            for previous in &scopes {
                let events_overlap = scope.events.iter().any(|left| {
                    previous.events.iter().any(|right| {
                        left == right
                            || left
                                .strip_suffix('*')
                                .is_some_and(|prefix| right.starts_with(prefix))
                            || right
                                .strip_suffix('*')
                                .is_some_and(|prefix| left.starts_with(prefix))
                    })
                });
                if events_overlap
                    && (scope.prefix.starts_with(&previous.prefix)
                        || previous.prefix.starts_with(&scope.prefix))
                    && (scope.suffix.ends_with(&previous.suffix)
                        || previous.suffix.ends_with(&scope.suffix))
                {
                    return Err(ProviderError::invalid_input("s3.BucketNotificationConfiguration has overlapping notification filters for overlapping events").for_resource(inputs.id().clone()));
                }
            }
            scopes.push(scope);
        }
    }
    Ok(())
}

fn validate_flow_log_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    let inputs = view.inputs();
    let invalid = |message| ProviderError::invalid_input(message).for_resource(inputs.id().clone());
    // ec2.json CreateFlowLogsRequest: all predicates below use only the
    // written config. Opaque secrets, references, and parents remain deferred.
    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateFlowLogs.html
    let resource_type = known_string(inputs.attribute("resource_type"));
    let transit = match resource_type {
        Some("TransitGateway" | "TransitGatewayAttachment") => Some(true),
        Some("VPC" | "Subnet" | "NetworkInterface" | "RegionalNatGateway") => Some(false),
        _ => None,
    };
    if let Some(transit) = transit {
        if transit {
            if matches!(inputs.attribute("max_aggregation_interval"), Some(DesiredValue::Known(DesiredScalar::Int(value))) if *value != 60)
            {
                return Err(invalid(
                    "ec2.FlowLog max_aggregation_interval must be 60 for transit gateway resource types",
                ));
            }
            if known_string(inputs.attribute("traffic_type")).is_some() {
                return Err(invalid(
                    "ec2.FlowLog traffic_type is not supported for transit gateway resource types",
                ));
            }
        } else if inputs.attribute("traffic_type").is_none() {
            return Err(invalid(
                "ec2.FlowLog traffic_type is required for non-transit-gateway resource types",
            ));
        }
        if let Some(DesiredValue::List(values)) = inputs.attribute("resource_ids") {
            let maximum = if transit { 25 } else { 300 };
            if values.iter().count() > maximum {
                return Err(invalid(
                    "ec2.FlowLog resource_ids exceeds the resource type's limit (25 for transit gateways, 300 otherwise)",
                ));
            }
        }
    }
    let destination = match inputs.attribute("log_destination_type") {
        None => Some("cloud-watch-logs"),
        value => known_string(value),
    };
    if destination == Some("cloud-watch-logs") {
        if inputs.attribute("deliver_logs_permission_arn").is_none() {
            return Err(invalid(
                "ec2.FlowLog deliver_logs_permission_arn is required for cloud-watch-logs",
            ));
        }
    } else if matches!(destination, Some("s3" | "kinesis-data-firehose"))
        && known_string(inputs.attribute("log_group_name")).is_some()
    {
        return Err(invalid(
            "ec2.FlowLog log_group_name is supported only for cloud-watch-logs",
        ));
    }
    Ok(())
}

fn normalize_subnet_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    let inputs = view.inputs();
    // CreateSubnetRequest$CidrBlock excludes IPv6-only subnets. Apply the
    // same exclusion to explicit IPv4 IPAM allocation inputs.
    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateSubnet.html
    if matches!(
        inputs.attribute("ipv6_native"),
        Some(DesiredValue::Known(DesiredScalar::Bool(true)))
    ) {
        for field in ["cidr_block", "ipv4_ipam_pool_id", "ipv4_netmask_length"] {
            if matches!(inputs.attribute(field), Some(DesiredValue::Known(_))) {
                return Err(ProviderError::invalid_input(format!(
                    "ec2.Subnet {field} is not supported when ipv6_native = true"
                ))
                .for_resource(inputs.id().clone()));
            }
        }
        // The request API permits resource-name for IPv4-only as well as
        // dual-stack subnets, unlike PrivateDnsNameOptionsOnLaunch's prose.
        // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_ModifySubnetAttribute.html
        if let Some(DesiredValue::Map(options)) =
            inputs.attribute("private_dns_name_options_on_launch")
            && known_string(options.get("hostname_type")) == Some("ip-name")
        {
            return Err(ProviderError::invalid_input("ec2.Subnet private_dns_name_options_on_launch.hostname_type must be resource-name when ipv6_native = true")
                .for_resource(inputs.id().clone()));
        }
    }
    crate::services::ec2::subnet::normalize_subnet_ipv6_cidr_blocks(view)
}

fn validate_vpc_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    // Tenancy is a shared Smithy enum. Keep its read domain while enforcing
    // CreateVpcRequest$InstanceTenancy's narrower documented request domain.
    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateVpc.html
    if known_string(view.inputs().attribute("instance_tenancy")) == Some("host") {
        return Err(ProviderError::invalid_input("ec2.Vpc instance_tenancy must be default or dedicated; host is not supported by CreateVpc")
            .for_resource(view.id().clone()));
    }
    Ok(())
}

fn validate_nat_gateway_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    crate::services::ec2::nat_gateway::validate_nat_gateway_inputs(&view.inputs())
}

fn validate_sqs_queue_view(view: &mut DesiredView<'_>) -> ProviderResult<()> {
    crate::services::sqs::queue::validate_sqs_queue_inputs(&view.inputs())
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
        Value::Deferred(DeferredValue::Secret(Box::new(value).into()))
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

        for (provider, resource_type) in [
            ("aws", "s3.Bucket"),
            ("awscc", "acm.Certificate"),
            ("awscc", "route53.RecordSet"),
        ] {
            let value = Value::Concrete(ConcreteValue::EnumIdentifier(RawEnumIdentifier::parse(
                "raw",
            )));
            let mut resources = vec![
                Resource::with_provider(provider, resource_type, "untouched", None)
                    .with_attribute("unrelated", value.clone())
                    .with_attribute("name", string("unchanged.example.com."))
                    .into(),
            ];

            AwsNormalizer
                .normalize_desired(&mut resources)
                .await
                .expect("an unhandled resource must not be projected or normalized");

            assert_eq!(resources[0].get_attr("unrelated"), Some(&value));
            assert_eq!(
                resources[0].get_attr("name"),
                Some(&string("unchanged.example.com."))
            );
        }
    }

    #[tokio::test]
    async fn native_preparation_rejects_non_wire_default_tags() {
        carina_core::validation::deferred_types::with_session_for_test_async(
            &carina_core::parser::LoadedProviderContext::empty_for_test(),
            async |type_checks| {
                use carina_core::executor::{
                    ModuleConstraintGate, ProviderPreparationContext, ProviderPreparationError,
                    prepare_create_request,
                };
                use carina_core::parser::ProviderConfig;
                use carina_core::protocol_convert::{
                    ProtocolConvertError, WirePathSegment, try_core_to_protocol_value,
                };
                use carina_core::provider::ProviderFactory;
                use carina_core::provider_wire::ProviderWireTarget;

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
                let factories: Vec<Box<dyn ProviderFactory>> =
                    vec![Box::new(crate::AwsProviderFactory)];
                let fixture_normalizer = carina_core::test_utils::provider_text::normalizer(
                    "aws",
                    Box::new(AwsNormalizer),
                );
                let preparation = ProviderPreparationContext::new(
                    &bindings,
                    &module_gate,
                    &configs,
                    &fixture_normalizer,
                    &factories,
                    &schemas,
                    type_checks,
                );
                let resource = Resource::with_provider("aws", "sqs.Queue", "crud-projection", None)
                    .with_attribute("queue_name", string("crud-projection"));
                let id = resource.id.clone();
                let error = prepare_create_request(resource, &preparation)
                    .await
                    .expect_err(
                        "native preparation must reject the non-wire default tag before dispatch",
                    );
                let ProviderPreparationError::Wire(error) = error else {
                    panic!("expected a typed provider wire failure: {error:?}");
                };
                assert_eq!(error.target(), &ProviderWireTarget::Resource(id));
                assert_eq!(error.attribute(), "tags");
                assert_eq!(error.path(), &[WirePathSegment::Key("unrelated".into())]);
                assert!(matches!(
                    error.conversion_error(),
                    ProtocolConvertError::DurationOutOfRange
                ));

                // WASM CRUD encoding uses the same checked projection and conversion.
                let wasm_error = try_core_to_protocol_value(&Value::Concrete(ConcreteValue::Map(
                    configs[0].default_tags.clone(),
                )))
                .expect_err("the same default tags cannot be encoded for WASM either");
                assert_eq!(error.path(), wasm_error.value_path());
                let ProtocolConvertError::ValuePath { error: reason, .. } = wasm_error else {
                    panic!("expected the WASM conversion error at the default-tag key");
                };
                assert!(matches!(*reason, ProtocolConvertError::DurationOutOfRange));
            },
        )
        .await
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
    fn cross_field_checks_keep_deferred_inputs_opaque_during_normalization() {
        for (resource_type, attributes) in [
            (
                "ec2.NatGateway",
                vec![
                    ("availability_mode", string("zonal")),
                    ("subnet_id", secret(string("subnet-12345678"))),
                ],
            ),
            (
                "ec2.NatGateway",
                vec![
                    ("availability_mode", string("zonal")),
                    ("subnet_id", reference("subnet_id")),
                ],
            ),
            (
                "sqs.Queue",
                vec![
                    ("fifo_queue", reference("fifo_queue")),
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
                    ("kms_master_key_id", reference("key_id")),
                ],
            ),
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
            let normalized = normalize_mixed(resource);
            assert_eq!(normalized.attributes, original);
        }
    }

    #[tokio::test]
    async fn normalize_desired_rejects_decidable_cross_field_conflicts() {
        for (resource_type, attributes, message) in [
            (
                "ec2.NatGateway",
                vec![
                    ("availability_mode", string("regional")),
                    ("subnet_id", string("subnet-12345678")),
                ],
                "must be omitted when availability_mode = regional",
            ),
            (
                "sqs.Queue",
                vec![
                    ("fifo_queue", Value::Concrete(ConcreteValue::Bool(false))),
                    ("deduplication_scope", string("messageGroup")),
                ],
                "requires fifo_queue = true",
            ),
        ] {
            let mut resource = Resource::with_provider("aws", resource_type, "conflict", None);
            for (name, value) in attributes {
                resource.set_attr(name, value);
            }
            let mut resources = vec![resource.into()];
            let error = AwsNormalizer
                .normalize_desired(&mut resources)
                .await
                .expect_err("decidable conflicts must fail native normalization");
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            assert!(error.to_string().contains(message), "{error:?}");
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
            crate::read_model::test_existing_state(id, attributes).to_snapshot(),
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
