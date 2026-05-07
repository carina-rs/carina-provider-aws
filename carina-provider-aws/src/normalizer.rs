//! AWS Provider normalizer and enum resolution

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, ProviderNormalizer};
use carina_core::resource::{Resource, Value};
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

/// Resolve enum identifiers in resources to their fully-qualified DSL format.
///
/// For example, resolves bare `Enabled` or `VersioningStatus.Enabled` into
/// `aws.s3.Bucket.VersioningStatus.Enabled` based on schema definitions.
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

        // Resolve enum attributes
        let mut resolved_attrs = HashMap::new();
        for (key, value) in &resource.attributes {
            if let Some(attr_schema) = config.schema.attributes.get(key.as_str())
                && let Some(parts) = attr_schema.attr_type.namespaced_enum_parts()
                && let Some(resolved) = carina_core::utils::resolve_enum_value(value, &parts)
            {
                resolved_attrs.insert(key.clone(), resolved);
            }
        }

        for (key, value) in resolved_attrs {
            resource.set_attr(key, value);
        }
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
                && let Value::Map(map_fields) = value
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
                    resolved.insert(key.clone(), Value::Map(normalized_map));
                }
            }
        }
    }

    for (key, value) in resolved {
        attributes.insert(key, value);
    }
}

/// Normalize absent optional list/map attributes in read state to canonical empty
/// collections.
///
/// AWS SDK responses model "no value" as `Option::None` (or an empty collection
/// that read code skips inserting). The differ, however, treats an absent
/// attribute and an explicit `Value::List(vec![])` as different shapes, which
/// produces a permanent `(none) → []` plan diff for any optional `list(...)`
/// attribute the user declared as `[]`.
///
/// This normalizer establishes a canonical form at the provider boundary: for
/// every optional `list(...)` attribute in the resource schema that is missing
/// from the read state, insert `Value::List(vec![])`. Same for optional
/// `map(...)` → `Value::Map(IndexMap::new())`.
///
/// Skipped:
/// - `required` attributes — the differ already requires them; absence indicates
///   a real read-side bug, not the "AWS returned None" surface this fixes.
/// - `write_only` attributes — by definition, AWS doesn't return them, so an
///   absent value is correct, not a bug to canonicalize away.
/// - `read_only` attributes — they are populated server-side; if AWS didn't
///   return one (e.g. an output ARN), the resource really has no value and
///   inserting `[]` would be wrong.
///
/// Tracked at carina-rs/carina-provider-aws#236, parent carina-rs/carina#2544.
pub(crate) fn normalize_absent_collections(
    resource_type: &str,
    attributes: &mut HashMap<String, Value>,
) {
    let configs = crate::schemas::generated::configs();
    let config = match configs
        .iter()
        .find(|c| c.schema.resource_type == resource_type)
    {
        Some(c) => c,
        None => return,
    };

    for (name, attr) in &config.schema.attributes {
        // Only canonicalize attributes that the user can actually express in
        // the DSL and that AWS may legitimately omit from a read response.
        if attr.required || attr.write_only || attr.read_only {
            continue;
        }
        if attributes.contains_key(name) {
            continue;
        }
        match &attr.attr_type {
            AttributeType::List { .. } => {
                attributes.insert(name.clone(), Value::List(Vec::new()));
            }
            AttributeType::Map { .. } => {
                attributes.insert(name.clone(), Value::Map(IndexMap::new()));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_enum_identifiers_namespaced_value() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr(
            "status".to_string(),
            Value::String("aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_bare_ident() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr("status".to_string(), Value::String("Enabled".to_string()));
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_typename_value() {
        let mut resource = Resource::with_provider("aws", "s3.BucketOwnershipControls", "test");
        resource.set_attr(
            "object_ownership".to_string(),
            Value::String("ObjectOwnership.BucketOwnerEnforced".to_string()),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("object_ownership"),
            Some(&Value::String(
                "aws.s3.BucketOwnershipControls.ObjectOwnership.BucketOwnerEnforced".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_plain_string() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test");
        resource.set_attr("status".to_string(), Value::String("Enabled".to_string()));
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_skips_non_aws() {
        let mut resource = Resource::with_provider("awscc", "s3.BucketVersioning", "test");
        resource.set_attr("status".to_string(), Value::String("Enabled".to_string()));
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        // Should not be modified since provider is "awscc"
        assert_eq!(
            resources[0].get_attr("status"),
            Some(&Value::String("Enabled".to_string()))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_with_to_dsl() {
        // ip_protocol has to_dsl that maps "-1" → "all"
        let mut resource = Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule");
        resource.set_attr("ip_protocol".to_string(), Value::String("-1".to_string()));
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("ip_protocol"),
            Some(&Value::String(
                "aws.ec2.SecurityGroupIngress.IpProtocol.all".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_with_to_dsl() {
        // Read returns "-1" for ip_protocol, should be normalized to "all" via to_dsl
        let mut attributes =
            HashMap::from([("ip_protocol".to_string(), Value::String("-1".to_string()))]);
        normalize_state_enums("ec2.SecurityGroupIngress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::String(
                "aws.ec2.SecurityGroupIngress.IpProtocol.all".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums() {
        let mut attributes = HashMap::from([
            ("bucket".to_string(), Value::String("my-bucket".to_string())),
            (
                "object_ownership".to_string(),
                Value::String("BucketOwnerEnforced".to_string()),
            ),
        ]);
        normalize_state_enums("s3.BucketOwnershipControls", &mut attributes);
        assert_eq!(
            attributes.get("object_ownership"),
            Some(&Value::String(
                "aws.s3.BucketOwnershipControls.ObjectOwnership.BucketOwnerEnforced".to_string()
            ))
        );
        // Non-enum attributes should not be modified
        assert_eq!(
            attributes.get("bucket"),
            Some(&Value::String("my-bucket".to_string()))
        );
    }

    #[test]
    fn test_normalize_state_enums_bucket_versioning() {
        let mut attributes = HashMap::from([
            ("bucket".to_string(), Value::String("my-bucket".to_string())),
            ("status".to_string(), Value::String("Enabled".to_string())),
        ]);
        normalize_state_enums("s3.BucketVersioning", &mut attributes);
        assert_eq!(
            attributes.get("status"),
            Some(&Value::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_already_namespaced() {
        let mut attributes = HashMap::from([(
            "status".to_string(),
            Value::String("aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()),
        )]);
        normalize_state_enums("s3.BucketVersioning", &mut attributes);
        // Already namespaced values (contain dots) should not be modified
        assert_eq!(
            attributes.get("status"),
            Some(&Value::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_ec2_vpc_instance_tenancy() {
        let mut resource = Resource::with_provider("aws", "ec2.Vpc", "test-vpc");
        resource.set_attr(
            "instance_tenancy".to_string(),
            Value::String("InstanceTenancy.dedicated".to_string()),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("instance_tenancy"),
            Some(&Value::String(
                "aws.ec2.Vpc.InstanceTenancy.dedicated".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_ec2_security_group_ingress_protocol() {
        let mut resource = Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule");
        resource.set_attr(
            "ip_protocol".to_string(),
            Value::String("IpProtocol.tcp".to_string()),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("ip_protocol"),
            Some(&Value::String(
                "aws.ec2.SecurityGroupIngress.IpProtocol.tcp".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_ec2_vpc_tenancy() {
        let mut attributes = HashMap::from([(
            "instance_tenancy".to_string(),
            Value::String("default".to_string()),
        )]);
        normalize_state_enums("ec2.Vpc", &mut attributes);
        assert_eq!(
            attributes.get("instance_tenancy"),
            Some(&Value::String(
                "aws.ec2.Vpc.InstanceTenancy.default".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_ec2_security_group_egress() {
        let mut attributes =
            HashMap::from([("ip_protocol".to_string(), Value::String("-1".to_string()))]);
        normalize_state_enums("ec2.SecurityGroupEgress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::String(
                "aws.ec2.SecurityGroupEgress.IpProtocol.all".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_struct_field_enum() {
        let mut inner = IndexMap::new();
        inner.insert(
            "hostname_type".to_string(),
            Value::String("ip-name".to_string()),
        );
        inner.insert(
            "enable_resource_name_dns_a_record".to_string(),
            Value::Bool(true),
        );
        let mut attributes = HashMap::from([(
            "private_dns_name_options_on_launch".to_string(),
            Value::Map(inner),
        )]);
        normalize_state_enums("ec2.Subnet", &mut attributes);
        if let Some(Value::Map(fields)) = attributes.get("private_dns_name_options_on_launch") {
            assert_eq!(
                fields.get("hostname_type"),
                Some(&Value::String(
                    "aws.ec2.Subnet.HostnameType.ip_name".to_string()
                ))
            );
            // Non-enum fields should not be modified
            assert_eq!(
                fields.get("enable_resource_name_dns_a_record"),
                Some(&Value::Bool(true))
            );
        } else {
            panic!("Expected Value::Map for private_dns_name_options_on_launch");
        }
    }

    #[test]
    fn test_normalize_state_enums_ec2_security_group_egress_tcp() {
        let mut attributes =
            HashMap::from([("ip_protocol".to_string(), Value::String("tcp".to_string()))]);
        normalize_state_enums("ec2.SecurityGroupEgress", &mut attributes);
        assert_eq!(
            attributes.get("ip_protocol"),
            Some(&Value::String(
                "aws.ec2.SecurityGroupEgress.IpProtocol.tcp".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_vpn_gateway_type_with_dot() {
        // "ipsec.1" contains a dot but is a raw enum value, not a namespaced identifier.
        // The normalizer should recognize it as a valid enum value and namespace it.
        let mut attributes =
            HashMap::from([("type".to_string(), Value::String("ipsec.1".to_string()))]);
        normalize_state_enums("ec2.VpnGateway", &mut attributes);
        assert_eq!(
            attributes.get("type"),
            Some(&Value::String(
                "aws.ec2.VpnGateway.Type.ipsec.1".to_string()
            ))
        );
    }

    #[test]
    fn test_normalize_state_enums_vpn_gateway_type_already_namespaced() {
        // Already in DSL format — should NOT be double-normalized.
        let mut attributes = HashMap::from([(
            "type".to_string(),
            Value::String("aws.ec2.VpnGateway.Type.ipsec.1".to_string()),
        )]);
        normalize_state_enums("ec2.VpnGateway", &mut attributes);
        assert_eq!(
            attributes.get("type"),
            Some(&Value::String(
                "aws.ec2.VpnGateway.Type.ipsec.1".to_string()
            ))
        );
    }

    // ---- normalize_absent_collections (#236) ----

    #[test]
    fn test_normalize_absent_collections_inserts_empty_list() {
        // route53.RecordSet has an optional list(String) attribute
        // `resource_records`. When the SDK response carries no records the
        // service code skips the insert; the normalizer must canonicalize the
        // gap to `Value::List(vec![])`.
        let mut attributes: HashMap<String, Value> = HashMap::new();
        attributes.insert("name".to_string(), Value::String("example.com".to_string()));
        attributes.insert("type".to_string(), Value::String("A".to_string()));

        normalize_absent_collections("route53.RecordSet", &mut attributes);

        assert_eq!(
            attributes.get("resource_records"),
            Some(&Value::List(Vec::new())),
            "absent optional list(...) attribute must canonicalize to Value::List(vec![])"
        );
    }

    #[test]
    fn test_normalize_absent_collections_preserves_present_list() {
        // If the read code already populated the list, it must be left alone.
        let mut attributes: HashMap<String, Value> = HashMap::new();
        attributes.insert(
            "resource_records".to_string(),
            Value::List(vec![Value::String("1.2.3.4".to_string())]),
        );

        normalize_absent_collections("route53.RecordSet", &mut attributes);

        assert_eq!(
            attributes.get("resource_records"),
            Some(&Value::List(vec![Value::String("1.2.3.4".to_string())])),
        );
    }

    #[test]
    fn test_normalize_absent_collections_skips_required() {
        // `name` is required on route53.RecordSet — its absence is a bug, not
        // something we should paper over by inserting an empty value (and the
        // schema type is String, not List, so this is doubly defensive).
        let mut attributes: HashMap<String, Value> = HashMap::new();
        normalize_absent_collections("route53.RecordSet", &mut attributes);
        assert!(!attributes.contains_key("name"));
    }

    #[test]
    fn test_normalize_absent_collections_skips_unknown_resource_type() {
        // No-op for unknown resource types — never panics, never inserts.
        let mut attributes: HashMap<String, Value> = HashMap::new();
        normalize_absent_collections("not.a.real.resource", &mut attributes);
        assert!(attributes.is_empty());
    }

    #[test]
    fn test_normalize_absent_collections_does_not_touch_scalars() {
        // String/Int/Bool optional attributes must remain absent, not be
        // synthesized to "" / 0 / false. The canonical-form fix is for
        // collection shapes only.
        let mut attributes: HashMap<String, Value> = HashMap::new();
        normalize_absent_collections("iam.Role", &mut attributes);
        // `description` is optional String — must not be inserted.
        assert!(!attributes.contains_key("description"));
        // `path` is optional String — must not be inserted.
        assert!(!attributes.contains_key("path"));
    }

    #[test]
    fn test_normalize_absent_collections_skips_read_only() {
        // route53.RecordSet exposes a read-only `id` attribute (synthesized
        // during read). The normalizer must not invent collection values for
        // read-only fields even if their type happened to be a list/map.
        let configs = crate::schemas::generated::configs();
        let config = configs
            .iter()
            .find(|c| c.schema.resource_type == "route53.RecordSet")
            .expect("route53.RecordSet schema present");
        let read_only_lists: Vec<&str> = config
            .schema
            .attributes
            .iter()
            .filter(|(_, a)| a.read_only && matches!(a.attr_type, AttributeType::List { .. }))
            .map(|(name, _)| name.as_str())
            .collect();
        // Whether or not such a read-only list happens to exist today, this
        // test is a regression guard: any read-only list/map must stay absent
        // unless the read code actually returned it.
        let mut attributes: HashMap<String, Value> = HashMap::new();
        normalize_absent_collections("route53.RecordSet", &mut attributes);
        for name in read_only_lists {
            assert!(
                !attributes.contains_key(name),
                "read-only list attribute {} must not be normalized to []",
                name
            );
        }
    }
}
