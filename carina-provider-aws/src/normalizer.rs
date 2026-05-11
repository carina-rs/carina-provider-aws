//! AWS Provider normalizer and enum resolution

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, ProviderNormalizer};
use carina_core::resource::{ConcreteValue, Resource, Value};
use carina_core::schema::{AttributeType, SchemaRegistry};

/// Schema extension for the AWS provider.
///
/// Handles plan-time normalization of enum identifiers.
pub struct AwsNormalizer;

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired(&self, resources: &mut [Resource]) {
        resolve_enum_identifiers(resources);
    }

    fn merge_default_tags(
        &self,
        resources: &mut [Resource],
        default_tags: &IndexMap<String, Value>,
        registry: &SchemaRegistry,
    ) {
        provider::merge_default_tags_for_provider("aws", resources, default_tags, registry);
    }
}

/// Normalize enum identifiers in resources to the AWS API-canonical
/// spelling — including nested positions (Struct fields, List items,
/// Map values).
///
/// Runs as two passes per attribute:
///
/// 1. [`carina_core::utils::resolve_enum_value_recursive`] converts every
///    DSL identifier in the value tree to the fully-qualified DSL form
///    (`enabled` → `aws.s3.Bucket.VersioningStatus.enabled`). This step
///    is provider-neutral.
/// 2. [`api_canonicalize_recursive`] then walks the same shape and
///    rewrites each enum value to its API-canonical spelling
///    (`aws.s3.Bucket.VersioningStatus.enabled` → `Enabled`) via
///    [`DslMap::api_for`]. Provider hand-written code reads
///    `Value::Concrete(ConcreteValue::String(api_canonical))` and
///    passes it straight to the AWS SDK builder — `extract_enum_value`
///    is no longer needed anywhere in `services/`.
///
/// The recursion is the contract: every enum value reaching the
/// provider is API-canonical no matter how deeply it is nested.
pub(crate) fn resolve_enum_identifiers(resources: &mut [Resource]) {
    let configs = crate::schemas::generated::configs();

    for resource in resources.iter_mut() {
        // Only handle aws resources
        if resource.id.provider != "aws" {
            continue;
        }

        // Find the matching schema config
        let config = configs
            .iter()
            .find(|c| c.schema.resource_type == resource.id.resource_type);
        let config = match config {
            Some(c) => c,
            None => continue,
        };

        let mut resolved_attrs = HashMap::new();
        for (key, value) in &resource.attributes {
            let Some(attr_schema) = config.schema.attributes.get(key.as_str()) else {
                continue;
            };
            // Pass 1: bare/short DSL identifiers → fully-qualified DSL form.
            let after_dsl =
                carina_core::utils::resolve_enum_value_recursive(value, &attr_schema.attr_type);
            // Pass 2: DSL spelling → API canonical at every nested enum position.
            let base = after_dsl.as_ref().unwrap_or(value);
            let after_api = api_canonicalize_recursive(base, &attr_schema.attr_type);

            match (after_dsl, after_api) {
                (_, Some(v)) => {
                    resolved_attrs.insert(key.clone(), v);
                }
                (Some(v), None) => {
                    resolved_attrs.insert(key.clone(), v);
                }
                (None, None) => {}
            }
        }

        for (key, value) in resolved_attrs {
            resource.set_attr(key, value);
        }
    }
}

/// Walk `value` against `attr_type` and rewrite every enum spelling to
/// the AWS API-canonical form via `DslMap::api_for`.
///
/// Input expectation: enum identifiers are already in fully-qualified
/// DSL form (the output of `resolve_enum_value_recursive`), so the
/// extraction is `extract_enum_value(s)` — strip the namespace prefix —
/// followed by `dsl_map.api_for(trailing)` to translate DSL → API.
///
/// Returns `None` when nothing was rewritten, mirroring the
/// `resolve_enum_value_recursive` contract so callers can compose the
/// two passes without redundant clones.
fn api_canonicalize_recursive(value: &Value, attr_type: &AttributeType) -> Option<Value> {
    // Leaf: StringEnum (with or without namespace). We canonicalize via
    // the alias table regardless of whether the input was namespaced —
    // `extract_enum_value` handles both shapes.
    if let Some((_, _, _, dsl_map)) = attr_type.string_enum_parts() {
        let Value::Concrete(ConcreteValue::String(s)) = value else {
            return None;
        };
        let dsl_trailing = carina_core::utils::extract_enum_value(s);
        let api = dsl_map.api_for(dsl_trailing);
        // No-op if the string is already API-canonical AND has no
        // namespace prefix (i.e. `s == api`). Otherwise rewrite to the
        // bare API spelling so SDK::from(...) accepts it.
        if s == &api {
            return None;
        }
        return Some(Value::Concrete(ConcreteValue::String(api)));
    }

    match attr_type {
        AttributeType::Struct { fields, .. } => {
            let Value::Concrete(ConcreteValue::Map(map)) = value else {
                return None;
            };
            let mut rewritten = map.clone();
            let mut changed = false;
            for field in fields {
                if let Some(field_value) = map.get(&field.name)
                    && let Some(new_field) =
                        api_canonicalize_recursive(field_value, &field.field_type)
                {
                    rewritten.insert(field.name.clone(), new_field);
                    changed = true;
                }
            }
            changed.then_some(Value::Concrete(ConcreteValue::Map(rewritten)))
        }
        AttributeType::List { inner, .. } => {
            let Value::Concrete(ConcreteValue::List(items)) = value else {
                return None;
            };
            let mut rewritten = items.clone();
            let mut changed = false;
            for (i, item) in items.iter().enumerate() {
                if let Some(new_item) = api_canonicalize_recursive(item, inner) {
                    rewritten[i] = new_item;
                    changed = true;
                }
            }
            changed.then_some(Value::Concrete(ConcreteValue::List(rewritten)))
        }
        AttributeType::Map { value: inner, .. } => {
            let Value::Concrete(ConcreteValue::Map(map)) = value else {
                return None;
            };
            let mut rewritten = map.clone();
            let mut changed = false;
            for (k, v) in map {
                if let Some(new_v) = api_canonicalize_recursive(v, inner) {
                    rewritten.insert(k.clone(), new_v);
                    changed = true;
                }
            }
            changed.then_some(Value::Concrete(ConcreteValue::Map(rewritten)))
        }
        // Scalars and Union: nothing to descend into. Union arms with
        // enum values are not supported (mirroring the upstream
        // `resolve_enum_value_recursive` contract).
        _ => None,
    }
}

/// Normalize enum values in read-returned state attributes to namespaced DSL format.
///
/// Read methods return plain values like `"Enabled"` from AWS APIs.
/// This converts them to namespaced format like `aws.s3.Bucket.VersioningStatus.Enabled`
/// to match the resolved DSL values.
pub(crate) fn normalize_state_enums(resource_type: &str, attributes: &mut HashMap<String, Value>) {
    let configs = crate::schemas::generated::configs();
    let config = configs
        .iter()
        .find(|c| c.schema.resource_type == resource_type);
    let config = match config {
        Some(c) => c,
        None => return,
    };

    let mut resolved = HashMap::new();
    for (key, value) in attributes.iter() {
        if let Some(attr_schema) = config.schema.attributes.get(key.as_str()) {
            if let Some(parts) = attr_schema.attr_type.namespaced_enum_parts() {
                let enum_vals = attr_schema
                    .attr_type
                    .string_enum_parts()
                    .map(|(_, v, _, _)| v);
                let check = |s: &str| {
                    enum_vals.is_some_and(|vals| vals.iter().any(|v| v.eq_ignore_ascii_case(s)))
                };
                if let Some(normalized) =
                    carina_core::utils::normalize_state_enum_value(value, &parts, Some(&check))
                {
                    resolved.insert(key.clone(), normalized);
                }
            }
            // Normalize enum fields within struct (Map) values
            if let carina_core::schema::AttributeType::Struct { fields, .. } =
                &attr_schema.attr_type
                && let Value::Concrete(ConcreteValue::Map(map_fields)) = value
            {
                let mut normalized_map = map_fields.clone();
                for field in fields {
                    if let Some(parts) = field.field_type.namespaced_enum_parts()
                        && let Some(field_value) = map_fields.get(&field.name)
                    {
                        // Struct field state normalization: bare values only (no dot-check needed)
                        if let Some(normalized) =
                            carina_core::utils::resolve_enum_value(field_value, &parts)
                        {
                            normalized_map.insert(field.name.clone(), normalized);
                        }
                    }
                }
                if normalized_map != *map_fields {
                    resolved.insert(
                        key.clone(),
                        Value::Concrete(ConcreteValue::Map(normalized_map)),
                    );
                }
            }
        }
    }

    for (key, value) in resolved {
        attributes.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // After aws#258, `resolve_enum_identifiers` runs a second pass that
    // canonicalizes DSL spellings into the AWS API form (bare value). So
    // every shape of input — namespaced, TypeName.value, bare DSL alias,
    // bare API canonical — collapses to the AWS API spelling
    // (e.g. `"Enabled"`) before the provider sees it. Pre-#258, the
    // output was the fully-qualified DSL form
    // (e.g. `"aws.s3.BucketVersioning.VersioningStatus.enabled"`).
    #[test]
    fn test_resolve_enum_identifiers_namespaced_value() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string(),
            )),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_bare_ident() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String("Enabled".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_typename_value() {
        let mut resource = Resource::with_provider("aws", "s3.BucketOwnershipControls", "test");
        resource.set_attr(
            "object_ownership".to_string(),
            Value::Concrete(ConcreteValue::String(
                "ObjectOwnership.BucketOwnerEnforced".to_string(),
            )),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("object_ownership"),
            Some(&Value::Concrete(ConcreteValue::String(
                "BucketOwnerEnforced".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_plain_string() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String("Enabled".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_skips_non_aws() {
        let mut resource = Resource::with_provider("awscc", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String("Enabled".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        // Should not be modified since provider is "awscc"
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_with_to_dsl() {
        // ip_protocol has `dsl_aliases` mapping API `"-1"` ↔ DSL `"all"`.
        // The pass-2 canonicalize rewrites the DSL spelling back to
        // the API form, so `"-1"` round-trips to itself (it's already
        // API-canonical; pass-1 only namespaces it, pass-2 strips the
        // namespace and canonicalizes via api_for).
        let mut resource = Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule");
        resource.set_attr(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("-1".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("ip_protocol"),
            Some(&Value::Concrete(ConcreteValue::String("-1".to_string())))
        );
    }

    #[test]
    fn test_normalize_state_enums_with_to_dsl() {
        // Read returns "-1" for ip_protocol, should be normalized to "all" via to_dsl
        let mut attributes = HashMap::from([(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("-1".to_string())),
        )]);
        normalize_state_enums("ec2.SecurityGroupIngress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.SecurityGroupIngress.IpProtocol.all".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums() {
        let mut attributes = HashMap::from([
            (
                "bucket".to_string(),
                Value::Concrete(ConcreteValue::String("my-bucket".to_string())),
            ),
            (
                "object_ownership".to_string(),
                Value::Concrete(ConcreteValue::String("BucketOwnerEnforced".to_string())),
            ),
        ]);
        normalize_state_enums("s3.BucketOwnershipControls", &mut attributes);
        // After carina#2832, normalize_state_enum_value applies dsl_aliases
        // and writes back the DSL spelling for diff stability.
        assert_eq!(
            attributes.get("object_ownership"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketOwnershipControls.ObjectOwnership.bucket_owner_enforced".to_string()
            )))
        );
        // Non-enum attributes should not be modified
        assert_eq!(
            attributes.get("bucket"),
            Some(&Value::Concrete(ConcreteValue::String(
                "my-bucket".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_bucket_versioning() {
        let mut attributes = HashMap::from([
            (
                "bucket".to_string(),
                Value::Concrete(ConcreteValue::String("my-bucket".to_string())),
            ),
            (
                "status".to_string(),
                Value::Concrete(ConcreteValue::String("Enabled".to_string())),
            ),
        ]);
        normalize_state_enums("s3.BucketVersioning", &mut attributes);
        assert_eq!(
            attributes.get("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_already_namespaced() {
        let mut attributes = HashMap::from([(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string(),
            )),
        )]);
        normalize_state_enums("s3.BucketVersioning", &mut attributes);
        // Already namespaced values (contain dots) should not be modified
        assert_eq!(
            attributes.get("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_dsl_alias_to_api_canonical() {
        // Regression for issue #258: DSL alias `enabled` must be
        // canonicalized to AWS API spelling `Enabled` before reaching
        // the provider's SDK::from() call. Without this, the SDK wraps
        // the unknown value and S3 returns MalformedXML.
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.enabled".to_string(),
            )),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_nested_struct_field() {
        // Struct field enums (e.g. partitioned_prefix.partition_date_source
        // in BucketLogging) must be canonicalized through the recursion,
        // not just the top-level attribute.
        use indexmap::IndexMap;
        let mut resource = Resource::with_provider("aws", "s3.BucketLogging", "test");
        let mut pp = IndexMap::new();
        pp.insert(
            "partition_date_source".to_string(),
            Value::Concrete(ConcreteValue::String("event_time".to_string())),
        );
        let mut tokf = IndexMap::new();
        tokf.insert(
            "partitioned_prefix".to_string(),
            Value::Concrete(ConcreteValue::Map(pp)),
        );
        resource.set_attr(
            "target_object_key_format".to_string(),
            Value::Concrete(ConcreteValue::Map(tokf)),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(outer))) =
            resources[0].get_attr("target_object_key_format")
        else {
            panic!("expected map");
        };
        let Some(Value::Concrete(ConcreteValue::Map(inner))) = outer.get("partitioned_prefix")
        else {
            panic!("expected nested map");
        };
        assert_eq!(
            inner.get("partition_date_source"),
            Some(&Value::Concrete(ConcreteValue::String(
                "EventTime".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_ec2_vpc_instance_tenancy() {
        let mut resource = Resource::with_provider("aws", "ec2.Vpc", "test-vpc");
        resource.set_attr(
            "instance_tenancy".to_string(),
            Value::Concrete(ConcreteValue::String(
                "InstanceTenancy.dedicated".to_string(),
            )),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("instance_tenancy"),
            Some(&Value::Concrete(ConcreteValue::String(
                "dedicated".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_ec2_security_group_ingress_protocol() {
        let mut resource = Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule");
        resource.set_attr(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("IpProtocol.tcp".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("ip_protocol"),
            Some(&Value::Concrete(ConcreteValue::String("tcp".to_string())))
        );
    }

    #[test]
    fn test_normalize_state_enums_ec2_vpc_tenancy() {
        let mut attributes = HashMap::from([(
            "instance_tenancy".to_string(),
            Value::Concrete(ConcreteValue::String("default".to_string())),
        )]);
        normalize_state_enums("ec2.Vpc", &mut attributes);
        assert_eq!(
            attributes.get("instance_tenancy"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.Vpc.InstanceTenancy.default".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_ec2_security_group_egress() {
        let mut attributes = HashMap::from([(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("-1".to_string())),
        )]);
        normalize_state_enums("ec2.SecurityGroupEgress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.SecurityGroupEgress.IpProtocol.all".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_struct_field_enum() {
        let mut inner = IndexMap::new();
        inner.insert(
            "hostname_type".to_string(),
            Value::Concrete(ConcreteValue::String("ip-name".to_string())),
        );
        inner.insert(
            "enable_resource_name_dns_a_record".to_string(),
            Value::Concrete(ConcreteValue::Bool(true)),
        );
        let mut attributes = HashMap::from([(
            "private_dns_name_options_on_launch".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        )]);
        normalize_state_enums("ec2.Subnet", &mut attributes);
        if let Some(Value::Concrete(ConcreteValue::Map(fields))) =
            attributes.get("private_dns_name_options_on_launch")
        {
            assert_eq!(
                fields.get("hostname_type"),
                Some(&Value::Concrete(ConcreteValue::String(
                    "aws.ec2.Subnet.HostnameType.ip_name".to_string()
                )))
            );
            // Non-enum fields should not be modified
            assert_eq!(
                fields.get("enable_resource_name_dns_a_record"),
                Some(&Value::Concrete(ConcreteValue::Bool(true)))
            );
        } else {
            panic!("Expected Value::Map for private_dns_name_options_on_launch");
        }
    }

    #[test]
    fn test_normalize_state_enums_ec2_security_group_egress_tcp() {
        let mut attributes = HashMap::from([(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("tcp".to_string())),
        )]);
        normalize_state_enums("ec2.SecurityGroupEgress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.SecurityGroupEgress.IpProtocol.tcp".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_vpn_gateway_type_with_dot() {
        // "ipsec.1" contains a dot but is a raw enum value, not a namespaced identifier.
        // The normalizer should recognize it as a valid enum value and namespace it.
        let mut attributes = HashMap::from([(
            "type".to_string(),
            Value::Concrete(ConcreteValue::String("ipsec.1".to_string())),
        )]);
        normalize_state_enums("ec2.VpnGateway", &mut attributes);
        assert_eq!(
            attributes.get("type"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.VpnGateway.Type.ipsec.1".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_vpn_gateway_type_already_namespaced() {
        // Already in DSL format — should NOT be double-normalized.
        let mut attributes = HashMap::from([(
            "type".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.ec2.VpnGateway.Type.ipsec.1".to_string(),
            )),
        )]);
        normalize_state_enums("ec2.VpnGateway", &mut attributes);
        assert_eq!(
            attributes.get("type"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.VpnGateway.Type.ipsec.1".to_string()
            )))
        );
    }
}
