//! AWS Provider normalizer and enum resolution

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, BoxFuture, ProviderNormalizer, SavedAttrs, ready_noop};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use carina_core::schema::{AttributeType, SchemaRegistry};

/// Schema extension for the AWS provider.
///
/// Handles plan-time normalization of enum identifiers.
pub struct AwsNormalizer;

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired<'a>(&'a self, resources: &'a mut [ManagedResource]) -> BoxFuture<'a, ()> {
        // Bodies are pure (enum resolution, dns-name strip — no I/O);
        // the trait is async only so the WASM host impl can `.await`
        // the guest directly (carina#3112). Wrapping the existing sync
        // logic in a ready future is sufficient here.
        Box::pin(async move {
            resolve_enum_identifiers(resources);
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
        resources: &'a mut [ManagedResource],
        default_tags: &'a IndexMap<String, Value>,
        registry: &'a SchemaRegistry,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            provider::merge_default_tags_for_provider("aws", resources, default_tags, registry);
        })
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
pub(crate) fn resolve_enum_identifiers(resources: &mut [ManagedResource]) {
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
/// Input shapes: enum leaves arrive either as fully-qualified DSL
/// `String` (the output of `resolve_enum_value_recursive`) or as
/// `EnumIdentifier` (DSL-source values that Pass-1 leaves untouched,
/// e.g. nested IAM policy `version` / `effect` after the carina#3055
/// state-lift). Both carry the same text payload; extraction is
/// `extract_enum_value_with_values(s, values)` — strip the namespace
/// prefix, recovering dotted enum values like the legacy `ipsec.1` —
/// followed by `dsl_map.api_for(trailing)` to translate DSL → API.
///
/// Returns `None` when nothing was rewritten, mirroring the
/// `resolve_enum_value_recursive` contract so callers can compose the
/// two passes without redundant clones.
fn api_canonicalize_recursive(value: &Value, attr_type: &AttributeType) -> Option<Value> {
    // Leaf: StringEnum (with or without namespace). We canonicalize via
    // the alias table regardless of whether the input was namespaced —
    // `extract_enum_value_with_values` handles both shapes (including
    // dotted enum values like the legacy `ipsec.1`).
    //
    // Both `String` and `EnumIdentifier` carry the same text payload; the
    // shape distinction (carina#2986) is irrelevant once we are
    // rewriting to the AWS wire form. Accepting `EnumIdentifier` here is
    // the fix for aws#315: after the carina#3055 state-lift, nested
    // StringEnum struct fields (IAM policy `version` / `effect`) arrive
    // as `EnumIdentifier`, and a `String`-only guard silently skipped
    // them, so the DSL spelling reached AWS and was rejected with
    // `MalformedPolicy`.
    if let Some((_, values, _, dsl_map)) = attr_type.string_enum_parts() {
        let (Value::Concrete(ConcreteValue::String(s))
        | Value::Concrete(ConcreteValue::EnumIdentifier(s))) = value
        else {
            return None;
        };
        let valid: Vec<&str> = values.iter().map(String::as_str).collect();
        let dsl_trailing = carina_core::utils::extract_enum_value_with_values(s, &valid);
        let api = dsl_map.api_for(dsl_trailing);
        // No-op only when the value is already the bare API spelling AND
        // arrived as a plain `String` (no namespace, no shape rewrite).
        // An `EnumIdentifier` must still be lowered to `String` so the
        // schema-blind downstream serializers see a plain string.
        if matches!(value, Value::Concrete(ConcreteValue::String(_))) && s == &api {
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
        AttributeType::Map { key, value: inner } => {
            let Value::Concrete(ConcreteValue::Map(map)) = value else {
                return None;
            };
            // aws#325: when the Map's *key* type is a `StringEnum`
            // (e.g. the IAM policy `condition`'s `ConditionOperator`),
            // canonicalize each key to the StringEnum spelling —
            // symmetric to the value recursion below. Without this a
            // non-canonical operator key (namespaced / aliased) on the
            // desired side was never reconciled, while the state side
            // already lands on the snake spelling via the IAM read
            // path (`json_to_condition` → `condition_operator_to_snake`,
            // role.rs) — leaving the two sides out of sync. This key
            // canon brings the desired side to the same spelling.
            let canon_of = |k: &str| -> String {
                match &key.string_enum_parts() {
                    Some((_, values, _, dsl_map)) => {
                        let valid: Vec<&str> = values.iter().map(String::as_str).collect();
                        dsl_map.api_for(carina_core::utils::extract_enum_value_with_values(
                            k, &valid,
                        ))
                    }
                    None => k.to_string(),
                }
            };
            // Detect whether canonicalizing keys would collapse two
            // distinct source keys onto one canonical spelling (e.g.
            // the same condition operator written twice — bare AND
            // namespaced). That is malformed input; pre-PR the keys
            // were never canonicalized so both survived as distinct
            // keys and the downstream serializer rejected it loudly
            // with `MalformedPolicy`. To preserve that (never silently
            // drop a clause) we disable key canonicalization wholesale
            // for this map when a collision exists — order-independent
            // because it inspects the whole key set up front, not the
            // per-iteration insertion order. Value recursion still
            // applies in both cases.
            let mut seen_canon = std::collections::HashSet::new();
            let key_collision = map.keys().any(|k| !seen_canon.insert(canon_of(k)));
            // Rebuild preserving insertion order (IndexMap order is
            // load-bearing for plan display, #2222).
            let mut rewritten = indexmap::IndexMap::with_capacity(map.len());
            let mut changed = false;
            for (k, v) in map {
                let new_key = if key_collision {
                    k.clone()
                } else {
                    let ck = canon_of(k);
                    if ck != *k {
                        changed = true;
                    }
                    ck
                };
                let new_v = match api_canonicalize_recursive(v, inner) {
                    Some(nv) => {
                        changed = true;
                        nv
                    }
                    None => v.clone(),
                };
                rewritten.insert(new_key, new_v);
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
        let mut resource =
            ManagedResource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "s3.BucketOwnershipControls", "test", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource =
            ManagedResource::with_provider("awscc", "s3.BucketVersioning", "test", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource = ManagedResource::with_provider("aws", "s3.BucketLogging", "test", None);
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

    /// Root-cause regression for aws#315: the IAM policy document is a
    /// fully schema-typed `Struct` (`iam_policy_document()`) whose
    /// `version` and nested `statement[].effect` are `StringEnum` fields.
    /// After the carina#3055 state-lift these arrive as `EnumIdentifier`,
    /// not `String`. The Pass-2 leaf guard previously matched only
    /// `String`, so it silently skipped them and the DSL spelling
    /// (`aws.iam.PolicyDocument.Version.2012_10_17`, `allow`) flowed
    /// through to the AWS API, which rejected the PUT with
    /// `MalformedPolicy`. The fix accepts `EnumIdentifier` at the
    /// StringEnum leaf and lowers it to the AWS-canonical `String`
    /// (`"2012-10-17"`, `"Allow"`) — generically, for every nested
    /// StringEnum struct field, not just IAM policy.
    #[test]
    fn test_resolve_enum_identifiers_iam_policy_doc_enum_identifier_nested() {
        use indexmap::IndexMap;
        let mut stmt = IndexMap::new();
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::String("sts:AssumeRole".to_string())),
        );
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier(
                "aws.iam.PolicyDocument.Version.2012_10_17".to_string(),
            )),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let mut resource = ManagedResource::with_provider("aws", "iam.Role", "test-role", None);
        resource.set_attr(
            "assume_role_policy_document".to_string(),
            Value::Concrete(ConcreteValue::Map(policy)),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);

        let Some(Value::Concrete(ConcreteValue::Map(pd))) =
            resources[0].get_attr("assume_role_policy_document")
        else {
            panic!("expected assume_role_policy_document Map");
        };
        assert_eq!(
            pd.get("version"),
            Some(&Value::Concrete(ConcreteValue::String(
                "2012-10-17".to_string()
            ))),
            "version must be lowered to AWS-canonical String"
        );
        let Some(Value::Concrete(ConcreteValue::List(stmts))) = pd.get("statement") else {
            panic!("expected statement List");
        };
        let Some(Value::Concrete(ConcreteValue::Map(s0))) = stmts.first() else {
            panic!("expected statement[0] Map");
        };
        assert_eq!(
            s0.get("effect"),
            Some(&Value::Concrete(ConcreteValue::String("Allow".to_string()))),
            "effect must be lowered to AWS-canonical String"
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_cloudfront_hosted_zone_id_in_alias_target() {
        // Regression for aws#302: `aws.cloudfront.HostedZoneId.global` written
        // at value position of `alias_target.hosted_zone_id` (a Struct field)
        // must resolve to the AWS-published constant `Z2FDTNDATAQYW2` before
        // the value reaches the SDK builder. The receiving Struct field is
        // schema-typed as a CloudFront-namespaced StringEnum, so the normalizer
        // descends into the struct and rewrites via `DslMap::api_for`.
        use indexmap::IndexMap;
        let mut resource =
            ManagedResource::with_provider("aws", "route53.RecordSet", "test-cf-alias", None);
        let mut alias_target = IndexMap::new();
        alias_target.insert(
            "dns_name".to_string(),
            Value::Concrete(ConcreteValue::String("d1234.cloudfront.net".to_string())),
        );
        alias_target.insert(
            "evaluate_target_health".to_string(),
            Value::Concrete(ConcreteValue::Bool(false)),
        );
        alias_target.insert(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String(
                "aws.cloudfront.HostedZoneId.global".to_string(),
            )),
        );
        resource.set_attr(
            "alias_target".to_string(),
            Value::Concrete(ConcreteValue::Map(alias_target)),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(at))) = resources[0].get_attr("alias_target")
        else {
            panic!("expected alias_target Map");
        };
        assert_eq!(
            at.get("hosted_zone_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Z2FDTNDATAQYW2".to_string()
            )))
        );
    }

    /// Variants of the aws#302 input shape — literal AWS spelling,
    /// TypeName shorthand, and the negative case where the *top-level*
    /// `RecordSet.hosted_zone_id` (user's own Route 53 zone) must NOT
    /// be coerced through the CloudFront alias table.
    #[test]
    fn test_resolve_enum_identifiers_cloudfront_hosted_zone_id_input_shapes() {
        use indexmap::IndexMap;
        let make_resource = |hzid: &str| {
            let mut resource =
                ManagedResource::with_provider("aws", "route53.RecordSet", "test", None);
            let mut at = IndexMap::new();
            at.insert(
                "dns_name".to_string(),
                Value::Concrete(ConcreteValue::String("d1.cloudfront.net".to_string())),
            );
            at.insert(
                "evaluate_target_health".to_string(),
                Value::Concrete(ConcreteValue::Bool(false)),
            );
            at.insert(
                "hosted_zone_id".to_string(),
                Value::Concrete(ConcreteValue::String(hzid.to_string())),
            );
            resource.set_attr(
                "alias_target".to_string(),
                Value::Concrete(ConcreteValue::Map(at)),
            );
            resource
        };

        // Literal `Z2FDTNDATAQYW2` (the AWS-published spelling) — the
        // StringEnum's `values` list accepts it directly without alias
        // rewriting.
        let mut resources = vec![make_resource("Z2FDTNDATAQYW2")];
        resolve_enum_identifiers(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(at))) = resources[0].get_attr("alias_target")
        else {
            panic!("expected alias_target Map");
        };
        assert_eq!(
            at.get("hosted_zone_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Z2FDTNDATAQYW2".to_string()
            )))
        );

        // TypeName shorthand `HostedZoneId.global` — resolves through
        // Pass 1's `TypeName.value` arm.
        let mut resources = vec![make_resource("HostedZoneId.global")];
        resolve_enum_identifiers(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(at))) = resources[0].get_attr("alias_target")
        else {
            panic!("expected alias_target Map");
        };
        assert_eq!(
            at.get("hosted_zone_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Z2FDTNDATAQYW2".to_string()
            )))
        );

        // Bare DSL alias `global` — `resolve_enum_value_recursive`
        // expands a bare identifier to `aws.cloudfront.HostedZoneId.global`
        // (Pass 1), and Pass 2 then rewrites the trailing alias through
        // `DslMap::api_for` to land on `Z2FDTNDATAQYW2`.
        let mut resources = vec![make_resource("global")];
        resolve_enum_identifiers(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(at))) = resources[0].get_attr("alias_target")
        else {
            panic!("expected alias_target Map");
        };
        assert_eq!(
            at.get("hosted_zone_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Z2FDTNDATAQYW2".to_string()
            )))
        );

        // Negative: top-level `RecordSet.hosted_zone_id` is the user's
        // own Route 53 zone (plain String in the schema) — a literal
        // hosted-zone ID like `Z1XYZABC123` must NOT be rewritten.
        let mut resource =
            ManagedResource::with_provider("aws", "route53.RecordSet", "test-toplevel", None);
        resource.set_attr(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String("Z1XYZABC123".to_string())),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);
        assert_eq!(
            resources[0].get_attr("hosted_zone_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Z1XYZABC123".to_string()
            )))
        );
    }

    #[test]
    fn test_resolve_enum_identifiers_ec2_vpc_instance_tenancy() {
        let mut resource = ManagedResource::with_provider("aws", "ec2.Vpc", "test-vpc", None);
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
        let mut resource =
            ManagedResource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule", None);
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
        // "ipsec.1" arrives from the AWS API as the canonical spelling.
        // Under carina#2980 / aws#268 the dotted form is rewritten to
        // snake_case (`ipsec.1` → `ipsec_1`) so it stays a bare DSL
        // identifier, and the namespaced DSL form uses the underscore
        // variant.
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

    #[tokio::test]
    async fn test_aws_normalizer_strips_record_set_trailing_dot() {
        // Regression for aws#300: when `route53.RecordSet.name` is fed by
        // another resource's read (e.g. `cert.domain_validation_options[0]
        // .resource_record.name`), the AWS API returns the FQDN with a
        // trailing dot. AwsNormalizer::normalize_desired must strip it so
        // the diff against the dot-stripped state row is stable.
        let mut resource =
            ManagedResource::with_provider("aws", "route53.RecordSet", "test-rec", None);
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

    /// aws#325: the IAM policy `condition` field is
    /// `Map<StringEnum "ConditionOperator", Map<..>>`.
    /// `api_canonicalize_recursive`'s `Map` arm recursed only the
    /// value, never the **key**, so a non-canonical
    /// `ConditionOperator` map key (here a fully-namespaced form) was
    /// left untouched on both the desired and state passes. Symmetric
    /// to the value recursion, the key must be canonicalized to the
    /// StringEnum's spelling.
    #[test]
    fn test_resolve_enum_identifiers_condition_operator_map_key() {
        use indexmap::IndexMap;

        // Inner condition body: { "aws:SecureTransport": "true" }
        let mut cond_body = IndexMap::new();
        cond_body.insert(
            "aws:SecureTransport".to_string(),
            Value::Concrete(ConcreteValue::String("true".to_string())),
        );
        // Operator key arrives fully-namespaced (the shape Pass-1 can
        // produce / a read path could emit); only the trailing
        // `string_equals` is the StringEnum value.
        let mut condition = IndexMap::new();
        condition.insert(
            "aws.iam.IamPolicyStatement.ConditionOperator.string_equals".to_string(),
            Value::Concrete(ConcreteValue::Map(cond_body)),
        );
        let mut stmt = IndexMap::new();
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
        );
        stmt.insert(
            "condition".to_string(),
            Value::Concrete(ConcreteValue::Map(condition)),
        );
        let mut policy = IndexMap::new();
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let mut resource = ManagedResource::with_provider("aws", "iam.Role", "test-role", None);
        resource.set_attr(
            "assume_role_policy_document".to_string(),
            Value::Concrete(ConcreteValue::Map(policy)),
        );
        let mut resources = vec![resource];
        resolve_enum_identifiers(&mut resources);

        let Some(Value::Concrete(ConcreteValue::Map(pd))) =
            resources[0].get_attr("assume_role_policy_document")
        else {
            panic!("expected assume_role_policy_document Map");
        };
        let Some(Value::Concrete(ConcreteValue::List(stmts))) = pd.get("statement") else {
            panic!("expected statement List");
        };
        let Some(Value::Concrete(ConcreteValue::Map(s0))) = stmts.first() else {
            panic!("expected statement[0] Map");
        };
        let Some(Value::Concrete(ConcreteValue::Map(cond))) = s0.get("condition") else {
            panic!("expected condition Map");
        };
        assert!(
            cond.contains_key("string_equals"),
            "ConditionOperator map key must be canonicalized to the \
             StringEnum spelling, got keys: {:?}",
            cond.keys().collect::<Vec<_>>()
        );
        assert!(
            !cond.contains_key("aws.iam.IamPolicyStatement.ConditionOperator.string_equals"),
            "the namespaced key must not survive"
        );
    }

    /// aws#325 collision guard: a malformed `condition` writing the
    /// same operator twice in different spellings (bare + namespaced)
    /// would have both keys fold to `string_equals`. Blind
    /// canonicalization lets `IndexMap::insert` overwrite one clause —
    /// silent policy weakening. The guard inspects the whole key set
    /// up front and, on collision, disables key canonicalization for
    /// the map wholesale so BOTH clauses survive with their original
    /// spellings (pre-PR behavior) and the downstream serializer still
    /// rejects the malformed input loudly. Asserted for **both**
    /// insertion orders — the bug the first cut had was order-dependent
    /// (reversed order silently dropped a clause).
    #[test]
    fn test_condition_operator_key_collision_keeps_both_clauses() {
        use indexmap::IndexMap;

        let body = |v: &str| {
            let mut m = IndexMap::new();
            m.insert(
                "aws:SecureTransport".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
            Value::Concrete(ConcreteValue::Map(m))
        };
        let bare = "string_equals";
        let nsd = "aws.iam.IamPolicyStatement.ConditionOperator.string_equals";

        // Run the same malformed condition in both key orderings.
        for (first, second) in [(bare, nsd), (nsd, bare)] {
            let mut condition = IndexMap::new();
            condition.insert(first.to_string(), body("true"));
            condition.insert(second.to_string(), body("false"));
            let mut stmt = IndexMap::new();
            stmt.insert(
                "effect".to_string(),
                Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
            );
            stmt.insert(
                "condition".to_string(),
                Value::Concrete(ConcreteValue::Map(condition)),
            );
            let mut policy = IndexMap::new();
            policy.insert(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(stmt),
                )])),
            );

            let mut resource = ManagedResource::with_provider("aws", "iam.Role", "test-role", None);
            resource.set_attr(
                "assume_role_policy_document".to_string(),
                Value::Concrete(ConcreteValue::Map(policy)),
            );
            let mut resources = vec![resource];
            resolve_enum_identifiers(&mut resources);

            let Some(Value::Concrete(ConcreteValue::Map(pd))) =
                resources[0].get_attr("assume_role_policy_document")
            else {
                panic!("expected Map");
            };
            let Some(Value::Concrete(ConcreteValue::List(stmts))) = pd.get("statement") else {
                panic!("expected statement List");
            };
            let Some(Value::Concrete(ConcreteValue::Map(s0))) = stmts.first() else {
                panic!("expected statement[0] Map");
            };
            let Some(Value::Concrete(ConcreteValue::Map(cond))) = s0.get("condition") else {
                panic!("expected condition Map");
            };
            // Both clauses survive with their ORIGINAL spellings —
            // collision disables canon wholesale, order-independent.
            assert_eq!(
                cond.len(),
                2,
                "order ({first}, {second}): both colliding clauses must \
                 survive, got: {:?}",
                cond.keys().collect::<Vec<_>>()
            );
            assert!(
                cond.contains_key(bare) && cond.contains_key(nsd),
                "order ({first}, {second}): both original keys kept \
                 (no silent drop), got: {:?}",
                cond.keys().collect::<Vec<_>>()
            );
        }
    }
}
