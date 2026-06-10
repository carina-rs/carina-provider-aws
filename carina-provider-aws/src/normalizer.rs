//! AWS Provider normalizer

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, BoxFuture, ProviderNormalizer, SavedAttrs, ready_noop};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
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
            if let Some(parts @ (_, enum_vals, _, _, _)) = attr_schema.attr_type.enum_parts() {
                let check = |s: &str| {
                    enum_vals.is_some_and(|vals| vals.iter().any(|v| v.eq_ignore_ascii_case(s)))
                };
                if let Some(normalized) =
                    carina_core::utils::normalize_state_enum_value(value, &parts, Some(&check))
                {
                    resolved.insert(key.clone(), normalized);
                }
            }
            // Normalize enum fields within struct (Map) values.
            if let carina_core::schema::Shape::Struct { .. } =
                config.schema.shape_of(&attr_schema.attr_type)
                && let Value::Concrete(ConcreteValue::Map(map_fields)) = value
            {
                let mut budget = carina_core::schema::ShapeWalkBudget::new(64);
                let Some(fields) = config
                    .schema
                    .struct_fields_with_budget(&attr_schema.attr_type, &mut budget)
                else {
                    continue;
                };
                let mut normalized_map = map_fields.clone();
                for field in fields {
                    if let Some(parts) = field.field_type.enum_parts()
                        && let Some(field_value) = map_fields.get(&field.name)
                    {
                        // Struct field state normalization: bare values only.
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

    #[test]
    fn test_normalize_state_enums_with_to_dsl() {
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
        assert_eq!(
            attributes.get("object_ownership"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketOwnershipControls.ObjectOwnership.bucket_owner_enforced".to_string()
            )))
        );
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
        assert_eq!(
            attributes.get("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.s3.BucketVersioning.VersioningStatus.Enabled".to_string()
            )))
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
                    "aws.ec2.Subnet.PrivateDnsNameOptionsOnLaunch.HostnameType.ip_name".to_string()
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
        let mut attributes = HashMap::from([(
            "type".to_string(),
            Value::Concrete(ConcreteValue::String("ipsec.1".to_string())),
        )]);
        normalize_state_enums("ec2.VpnGateway", &mut attributes);
        assert_eq!(
            attributes.get("type"),
            Some(&Value::Concrete(ConcreteValue::String(
                "aws.ec2.VpnGateway.Type.ipsec_1".to_string()
            )))
        );
    }

    #[test]
    fn test_normalize_state_enums_vpn_gateway_type_already_namespaced() {
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
