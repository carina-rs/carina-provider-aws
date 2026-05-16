//! AWS Provider normalizer and enum resolution

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::{self, ProviderNormalizer};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::{AttributeType, SchemaRegistry};

/// Schema extension for the AWS provider.
///
/// Handles plan-time normalization of enum identifiers.
pub struct AwsNormalizer;

impl ProviderNormalizer for AwsNormalizer {
    fn normalize_desired(&self, resources: &mut [Resource]) {
        resolve_enum_identifiers(resources);
        crate::services::route53::record_set::normalize_record_set_dns_names(resources);
    }

    fn normalize_state(&self, current_states: &mut HashMap<ResourceId, State>) {
        canonicalize_state_enums(current_states);
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
    for resource in resources.iter_mut() {
        // Only handle aws resources
        if resource.id.provider != "aws" {
            continue;
        }

        let resolved_attrs = resolve_attrs_against_schema(
            &resource.id.resource_type,
            resource.attributes.iter(),
            |value, attr_type| {
                // Pass 1: bare/short DSL identifiers → fully-qualified DSL form.
                let after_dsl = carina_core::utils::resolve_enum_value_recursive(value, attr_type);
                // Pass 2: DSL spelling → API canonical at every nested enum position.
                let base = after_dsl.as_ref().unwrap_or(value);
                let after_api = api_canonicalize_recursive(base, attr_type);
                // Pass 2's rewrite wins; otherwise fall back to Pass 1's.
                after_api.or(after_dsl)
            },
        );

        for (key, value) in resolved_attrs {
            resource.set_attr(key, value);
        }
    }
}

/// Look up the AWS schema config for `resource_type`, then run
/// `rewrite` against every attribute whose schema is known, collecting
/// the rewrites (`Some`) into a map. Shared scaffold for the
/// schema-driven enum passes (`resolve_enum_identifiers`,
/// `canonicalize_state_enums`); callers own the provider guard,
/// container iteration, and write-back, since `Resource` and `State`
/// store attributes differently and the two passes differ.
fn resolve_attrs_against_schema<'a>(
    resource_type: &str,
    attributes: impl Iterator<Item = (&'a String, &'a Value)>,
    rewrite: impl Fn(&Value, &AttributeType) -> Option<Value>,
) -> HashMap<String, Value> {
    let configs = crate::schemas::generated::configs();
    let Some(config) = configs
        .iter()
        .find(|c| c.schema.resource_type == resource_type)
    else {
        return HashMap::new();
    };

    let mut resolved_attrs = HashMap::new();
    for (key, value) in attributes {
        let Some(attr_schema) = config.schema.attributes.get(key.as_str()) else {
            continue;
        };
        if let Some(v) = rewrite(value, &attr_schema.attr_type) {
            resolved_attrs.insert(key.clone(), v);
        }
    }
    resolved_attrs
}

/// Canonicalize enum spellings in read-returned state to the AWS
/// API-canonical form, symmetric to `normalize_desired`'s Pass 2.
///
/// `normalize_desired` lowers every (nested) `StringEnum` leaf on the
/// desired side to the AWS wire form via [`api_canonicalize_recursive`].
/// Provider read paths, however, deliberately emit the DSL-alias
/// spelling for nested StringEnum leaves (e.g. the IAM policy doc's
/// `version` / `effect`, which `json_to_policy_doc` returns as
/// `EnumIdentifier("2012_10_17")` / `EnumIdentifier("allow")`). With no
/// symmetric state-side pass, the differ compared a DSL-alias
/// `EnumIdentifier` state leaf against an API-canonical `String`
/// desired leaf and reported a never-converging
/// `~ effect: "allow" → "Allow"` diff every plan (aws#323).
///
/// Running the *same* [`api_canonicalize_recursive`] pass over
/// `current_states` makes both differ sides API-canonical `String`, so
/// the cosmetic diff disappears. The pass is schema-driven and recurses
/// Struct/List/Map, so this is not IAM-policy-specific — it stabilizes
/// every AWS resource whose read shape carries a DSL-alias enum leaf.
///
/// Only Pass 2 runs here, not Pass 1's `resolve_enum_value_recursive`:
/// state values are post-read (API `String` or DSL-alias
/// `EnumIdentifier`), never the bare/short identifiers Pass 1 expands —
/// see `api_canonicalize_recursive`'s doc for the accepted leaf shapes.
pub(crate) fn canonicalize_state_enums(current_states: &mut HashMap<ResourceId, State>) {
    for state in current_states.values_mut() {
        // Only handle aws resources that actually exist. `not_found`
        // states carry no attributes, so skip the schema lookup.
        if state.id.provider != "aws" || !state.exists {
            continue;
        }

        let resolved_attrs = resolve_attrs_against_schema(
            &state.id.resource_type,
            state.attributes.iter(),
            api_canonicalize_recursive,
        );

        for (key, value) in resolved_attrs {
            state.attributes.insert(key, value);
        }
    }
}

/// Walk `value` against `attr_type` and rewrite every enum spelling to
/// the AWS API-canonical form via `DslMap::api_for`.
///
/// Input shapes: enum leaves arrive either as fully-qualified DSL
/// `String` (the output of `resolve_enum_value_recursive`) or as
/// `EnumIdentifier` (DSL-source values that Pass-1 leaves untouched,
/// e.g. nested IAM policy `version` / `effect`, which the provider read
/// paths emit directly as DSL-alias `EnumIdentifier` via
/// `json_effect_to_enum_identifier` / `json_version_to_enum_identifier`).
/// Both carry the same text payload; extraction is
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

    /// Regression for aws#323: after `carina apply` of an
    /// `aws.s3.BucketPolicy`, the read-back state row carries the IAM
    /// policy doc's nested StringEnum leaves as DSL-alias
    /// `EnumIdentifier` (`effect: "allow"`, `version: "2012_10_17"`) and
    /// principal sub-fields as singleton lists. `normalize_desired`
    /// canonicalizes the desired side to API-canonical `String`
    /// (`"Allow"`, `"2012-10-17"`) via `api_canonicalize_recursive`, but
    /// nothing applied the symmetric pass to `current_states`, so every
    /// subsequent `carina plan` reported a never-converging
    /// `~ effect: "allow" → "Allow"` / `~ version: "2012_10_17" →
    /// "2012-10-17"` diff. `AwsNormalizer::normalize_state` must run the
    /// same recursive canonicalization over state so both differ sides
    /// reach the comparator as API-canonical `String`.
    #[test]
    fn test_normalize_state_canonicalizes_iam_policy_doc_enum_leaves() {
        use indexmap::IndexMap;

        let mut stmt = IndexMap::new();
        stmt.insert(
            "effect".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
        );
        stmt.insert(
            "action".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("s3:GetObject".to_string()),
            )])),
        );
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
        );
        policy.insert(
            "statement".to_string(),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Map(stmt),
            )])),
        );

        let id = ResourceId::with_provider("aws", "s3.BucketPolicy", "test-bp", None);
        let mut attributes = HashMap::new();
        attributes.insert(
            "bucket".to_string(),
            Value::Concrete(ConcreteValue::String("my-bucket".to_string())),
        );
        attributes.insert(
            "policy".to_string(),
            Value::Concrete(ConcreteValue::Map(policy)),
        );
        let state = State::existing(id.clone(), attributes);

        let mut current_states = HashMap::new();
        current_states.insert(id.clone(), state);

        AwsNormalizer.normalize_state(&mut current_states);

        let Some(Value::Concrete(ConcreteValue::Map(pd))) =
            current_states.get(&id).unwrap().attributes.get("policy")
        else {
            panic!("expected policy Map");
        };
        assert_eq!(
            pd.get("version"),
            Some(&Value::Concrete(ConcreteValue::String(
                "2012-10-17".to_string()
            ))),
            "version must be canonicalized to AWS-canonical String"
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
            "effect must be canonicalized to AWS-canonical String"
        );

        // Non-enum attributes and non-enum struct fields must survive
        // untouched — the schema-driven walk must not over-rewrite.
        assert_eq!(
            current_states.get(&id).unwrap().attributes.get("bucket"),
            Some(&Value::Concrete(ConcreteValue::String(
                "my-bucket".to_string()
            ))),
            "non-enum attribute must not be rewritten"
        );
        assert_eq!(
            s0.get("action"),
            Some(&Value::Concrete(ConcreteValue::List(vec![
                Value::Concrete(ConcreteValue::String("s3:GetObject".to_string()))
            ]))),
            "non-enum struct field must not be rewritten"
        );

        // Idempotence: re-running over already-canonical state is a
        // no-op (every leaf is now a plain API-spelling `String`, which
        // `api_canonicalize_recursive` returns `None` for). A regression
        // here would re-wrap and reintroduce the never-converging diff.
        let snapshot = current_states.get(&id).unwrap().attributes.clone();
        AwsNormalizer.normalize_state(&mut current_states);
        assert_eq!(
            current_states.get(&id).unwrap().attributes,
            snapshot,
            "second normalize_state pass over canonical state must be a no-op"
        );
    }

    /// Companion to the IAM-policy regression above: proves
    /// `normalize_state` is generic (not IAM-policy-specific) by
    /// canonicalizing a *top-level* StringEnum state leaf on an
    /// unrelated resource. `s3.BucketVersioning.status` read-back as the
    /// DSL-alias `EnumIdentifier` must reach the differ as the
    /// API-canonical `String` `"Enabled"`, matching the desired side
    /// (`resolve_enum_identifiers`).
    #[test]
    fn test_normalize_state_canonicalizes_top_level_string_enum() {
        let id = ResourceId::with_provider("aws", "s3.BucketVersioning", "test-bv", None);
        let mut attributes = HashMap::new();
        attributes.insert(
            "status".to_string(),
            Value::Concrete(ConcreteValue::EnumIdentifier("enabled".to_string())),
        );
        let state = State::existing(id.clone(), attributes);

        let mut current_states = HashMap::new();
        current_states.insert(id.clone(), state);

        AwsNormalizer.normalize_state(&mut current_states);

        assert_eq!(
            current_states.get(&id).unwrap().attributes.get("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "Enabled".to_string()
            ))),
            "top-level StringEnum state leaf must be canonicalized"
        );
    }

    /// `not_found` states (`exists == false`) carry no attributes and
    /// must be skipped without panic.
    #[test]
    fn test_normalize_state_skips_not_found_state() {
        let id = ResourceId::with_provider("aws", "s3.BucketVersioning", "absent", None);
        let mut current_states = HashMap::new();
        current_states.insert(id.clone(), State::not_found(id.clone()));

        AwsNormalizer.normalize_state(&mut current_states);

        assert!(!current_states.get(&id).unwrap().exists);
        assert!(current_states.get(&id).unwrap().attributes.is_empty());
    }

    // After aws#258, `resolve_enum_identifiers` runs a second pass that
    // canonicalizes DSL spellings into the AWS API form (bare value). So
    // every shape of input — namespaced, TypeName.value, bare DSL alias,
    // bare API canonical — collapses to the AWS API spelling
    // (e.g. `"Enabled"`) before the provider sees it. Pre-#258, the
    // output was the fully-qualified DSL form
    // (e.g. `"aws.s3.BucketVersioning.VersioningStatus.enabled"`).
    #[test]
    fn test_resolve_enum_identifiers_namespaced_value() {
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
            Resource::with_provider("aws", "s3.BucketOwnershipControls", "test", None);
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
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource = Resource::with_provider("awscc", "s3.BucketVersioning", "test", None);
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
            Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule", None);
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
        let mut resource = Resource::with_provider("aws", "s3.BucketVersioning", "test", None);
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
        let mut resource = Resource::with_provider("aws", "s3.BucketLogging", "test", None);
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

        let mut resource = Resource::with_provider("aws", "iam.Role", "test-role", None);
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
            Resource::with_provider("aws", "route53.RecordSet", "test-cf-alias", None);
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
            let mut resource = Resource::with_provider("aws", "route53.RecordSet", "test", None);
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
            Resource::with_provider("aws", "route53.RecordSet", "test-toplevel", None);
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
        let mut resource = Resource::with_provider("aws", "ec2.Vpc", "test-vpc", None);
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
            Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test-rule", None);
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

    #[test]
    fn test_aws_normalizer_strips_record_set_trailing_dot() {
        // Regression for aws#300: when `route53.RecordSet.name` is fed by
        // another resource's read (e.g. `cert.domain_validation_options[0]
        // .resource_record.name`), the AWS API returns the FQDN with a
        // trailing dot. AwsNormalizer::normalize_desired must strip it so
        // the diff against the dot-stripped state row is stable.
        let mut resource = Resource::with_provider("aws", "route53.RecordSet", "test-rec", None);
        resource.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String("_abc.example.com.".to_string())),
        );
        let mut resources = vec![resource];
        AwsNormalizer.normalize_desired(&mut resources);
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
}
