//! Conversions between carina-core types and carina-provider-protocol types.
//!
//! This is a local copy of the convert module from carina-plugin-host,
//! needed because carina-plugin-host depends on wasmtime which cannot
//! compile to wasm32-wasip2.

use std::collections::HashMap;

use carina_core::resource::{
    ConcreteValue, DataSource as CoreDataSource, DeferredValue, Directives as CoreDirectives,
    Resource as CoreResource, ResourceId as CoreResourceId, State as CoreState, Value as CoreValue,
};
use carina_core::schema::{
    AttributeSchema as CoreAttributeSchema, AttributeType as CoreAttributeType,
    OperationConfig as CoreOperationConfig, RawShape as CoreRawShape,
    ResourceSchema as CoreResourceSchema, SchemaKind as CoreSchemaKind,
    StructField as CoreStructField, legacy_validator,
};
use carina_provider_protocol::types::{
    AttributeSchema as ProtoAttributeSchema, AttributeType as ProtoAttributeType,
    Directives as ProtoDirectives, OperationConfig as ProtoOperationConfig,
    Resource as ProtoResource, ResourceId as ProtoResourceId,
    ResourceSchema as ProtoResourceSchema, SchemaKind as ProtoSchemaKind, State as ProtoState,
    StructField as ProtoStructField, Value as ProtoValue,
};

// -- ResourceId --

pub fn core_to_proto_resource_id(id: &CoreResourceId) -> ProtoResourceId {
    ProtoResourceId {
        provider: id.provider.clone(),
        resource_type: id.resource_type.clone(),
        name: id.name.to_string(),
    }
}

pub fn proto_to_core_resource_id(id: &ProtoResourceId) -> CoreResourceId {
    // `ProtoResourceId` predates carina#3038's `provider_instance` field;
    // the WIT wire format does not carry it. Pass `None` so the
    // reconstructed `CoreResourceId` routes through the default provider.
    CoreResourceId::with_provider(&id.provider, &id.resource_type, &id.name, None)
}

// -- Value --

pub fn core_to_proto_value(v: &CoreValue) -> ProtoValue {
    match v {
        // `EnumIdentifier` carries identifier-shape text (parser-level
        // distinction from quoted-string literals, carina#2986). The
        // provider wire protocol has no native identifier variant, so we
        // emit it as `ProtoValue::String` — identical to the `String`
        // arm. The shape distinction is consumed at the validator entry
        // before reaching this conversion.
        CoreValue::Concrete(ConcreteValue::String(s))
        | CoreValue::Concrete(ConcreteValue::EnumIdentifier(s)) => ProtoValue::String(s.clone()),
        CoreValue::Concrete(ConcreteValue::Int(i)) => ProtoValue::Int(*i),
        CoreValue::Concrete(ConcreteValue::Float(f)) => ProtoValue::Float(*f),
        CoreValue::Concrete(ConcreteValue::Bool(b)) => ProtoValue::Bool(*b),
        // Duration is serialised to providers as integer seconds: the
        // WIT *type* boundary now has a native Duration variant
        // (carina#3166), but the WIT *value* boundary still crosses
        // Duration as IntVal(seconds) — schema-aware inbound re-typing
        // is the deferred follow-up flagged at
        // `carina-plugin-host/src/wasm_convert.rs:60-76`.
        CoreValue::Concrete(ConcreteValue::Duration(d)) => ProtoValue::Int(d.as_secs() as i64),
        CoreValue::Concrete(ConcreteValue::List(l)) => {
            ProtoValue::List(l.iter().map(core_to_proto_value).collect())
        }
        CoreValue::Concrete(ConcreteValue::StringList(items)) => ProtoValue::List(
            items
                .iter()
                .map(|s| ProtoValue::String(s.clone()))
                .collect(),
        ),
        CoreValue::Concrete(ConcreteValue::Map(m)) => ProtoValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), core_to_proto_value(v)))
                .collect(),
        ),
        // Deferred-axis values must be resolved before reaching the provider.
        // Phase 5a of RFC #2972 makes the axis explicit so we can pattern-match
        // each deferred variant individually. We do NOT fall through to
        // `format!("{v:?}")` because `Debug` on `Value::Deferred(Secret(inner))`
        // includes the inner plaintext — a leak. Emit a redacted sentinel
        // instead so the provider sees a clearly-bogus value rather than
        // either the plaintext or an inner pointer / panic.
        CoreValue::Deferred(DeferredValue::Secret(_)) => {
            ProtoValue::String("<redacted-secret>".to_string())
        }
        CoreValue::Deferred(DeferredValue::ResourceRef { path }) => {
            ProtoValue::String(format!("<unresolved-ref:{}>", path.to_dot_string()))
        }
        CoreValue::Deferred(DeferredValue::BindingRef { binding }) => {
            ProtoValue::String(format!("<unresolved-binding:{binding}>"))
        }
        CoreValue::Deferred(DeferredValue::Interpolation(_)) => {
            ProtoValue::String("<unresolved-interpolation>".to_string())
        }
        CoreValue::Deferred(DeferredValue::FunctionCall { name, .. }) => {
            ProtoValue::String(format!("<unresolved-fn:{name}>"))
        }
        CoreValue::Deferred(DeferredValue::Unknown(_)) => {
            ProtoValue::String("<unknown>".to_string())
        }
    }
}

pub fn proto_to_core_value(v: &ProtoValue) -> CoreValue {
    match v {
        ProtoValue::String(s) => CoreValue::Concrete(ConcreteValue::String(s.clone())),
        ProtoValue::Int(i) => CoreValue::Concrete(ConcreteValue::Int(*i)),
        ProtoValue::Float(f) => CoreValue::Concrete(ConcreteValue::Float(*f)),
        ProtoValue::Bool(b) => CoreValue::Concrete(ConcreteValue::Bool(*b)),
        ProtoValue::List(l) => CoreValue::Concrete(ConcreteValue::List(
            l.iter().map(proto_to_core_value).collect(),
        )),
        ProtoValue::Map(m) => CoreValue::Concrete(ConcreteValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), proto_to_core_value(v)))
                .collect(),
        )),
    }
}

pub fn core_to_proto_value_map(m: &HashMap<String, CoreValue>) -> HashMap<String, ProtoValue> {
    m.iter()
        .map(|(k, v)| (k.clone(), core_to_proto_value(v)))
        .collect()
}

pub fn proto_to_core_value_map(m: &HashMap<String, ProtoValue>) -> HashMap<String, CoreValue> {
    m.iter()
        .map(|(k, v)| (k.clone(), proto_to_core_value(v)))
        .collect()
}

// -- State --

pub fn core_to_proto_state(s: &CoreState) -> ProtoState {
    ProtoState {
        id: core_to_proto_resource_id(&s.id),
        identifier: s.identifier.clone(),
        attributes: core_to_proto_value_map(&s.attributes),
        exists: s.exists,
    }
}

pub fn proto_to_core_state(s: &ProtoState) -> CoreState {
    let id = proto_to_core_resource_id(&s.id);
    if s.exists {
        let mut state = CoreState::existing(id, proto_to_core_value_map(&s.attributes));
        if let Some(ref ident) = s.identifier {
            state = state.with_identifier(ident);
        }
        state
    } else {
        CoreState::not_found(id)
    }
}

// -- Resource --

pub fn core_to_proto_resource(r: &CoreResource) -> ProtoResource {
    ProtoResource {
        id: core_to_proto_resource_id(&r.id),
        attributes: core_to_proto_value_map(&r.resolved_attributes()),
        directives: core_to_proto_directives(&r.directives),
    }
}

// -- Directives --

pub fn core_to_proto_directives(l: &CoreDirectives) -> ProtoDirectives {
    ProtoDirectives {
        force_delete: l.force_delete,
        create_before_destroy: l.create_before_destroy,
        prevent_destroy: l.prevent_destroy,
    }
}

// -- proto_to_core_resource (reverse of core_to_proto_resource) --

pub fn proto_to_core_resource(r: &ProtoResource) -> CoreResource {
    let mut resource =
        CoreResource::with_provider(&r.id.provider, &r.id.resource_type, &r.id.name, None);
    resource.attributes = r
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), proto_to_core_value(v)))
        .collect();
    resource.directives = CoreDirectives {
        force_delete: r.directives.force_delete,
        create_before_destroy: r.directives.create_before_destroy,
        prevent_destroy: r.directives.prevent_destroy,
        depends_on: Vec::new(),
        provider_instance: None,
    };
    resource
}

/// Rebuild a [`CoreDataSource`] from the WIT `ResourceDef` carried over
/// the plugin boundary. The WIT contract has a single `Resource` record
/// shape; `Provider::read_data_source` consumes a `DataSource`, so a
/// data-source read request maps to this typed projection (carina#3181).
pub fn proto_to_core_data_source(r: &ProtoResource) -> CoreDataSource {
    let mut data_source =
        CoreDataSource::with_provider(&r.id.provider, &r.id.resource_type, &r.id.name, None);
    data_source.attributes = r
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), proto_to_core_value(v)))
        .collect();
    data_source.directives = CoreDirectives {
        force_delete: r.directives.force_delete,
        create_before_destroy: r.directives.create_before_destroy,
        prevent_destroy: r.directives.prevent_destroy,
        depends_on: Vec::new(),
        provider_instance: None,
    };
    data_source
}

// -- AttributeType --

fn proto_to_core_attribute_type(t: &ProtoAttributeType) -> CoreAttributeType {
    match t {
        ProtoAttributeType::String => CoreAttributeType::string(),
        ProtoAttributeType::Int => CoreAttributeType::int(),
        ProtoAttributeType::Float => CoreAttributeType::float(),
        ProtoAttributeType::Bool => CoreAttributeType::bool(),
        ProtoAttributeType::Duration => CoreAttributeType::duration(),
        ProtoAttributeType::StringEnum {
            values,
            name,
            namespace,
            dsl_aliases,
        } => CoreAttributeType::enum_(
            // Lift the wire-form flat dotted prefix into the
            // structured `TypeIdentity` the core schema carries
            // post-#3222.
            namespace
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|ns| carina_core::schema::enum_identity(name, Some(ns)))
                .unwrap_or_else(|| carina_core::schema::enum_identity(name, None)),
            Some(values.clone()),
            // Thread the alias data through the WASM boundary so the host
            // validator's `matches_alias` arm can accept DSL spellings —
            // a `fn` pointer cannot survive proto serialization
            // (carina#2831 / aws#247).
            dsl_aliases.clone(),
            None,
            None,
        ),
        ProtoAttributeType::List { inner, ordered } => {
            let inner_t = proto_to_core_attribute_type(inner);
            if *ordered {
                CoreAttributeType::list(inner_t)
            } else {
                CoreAttributeType::unordered_list(inner_t)
            }
        }
        ProtoAttributeType::Map { inner, key } => CoreAttributeType::map_with_key(
            proto_to_core_attribute_type(key),
            proto_to_core_attribute_type(inner),
        ),
        ProtoAttributeType::Struct { name, fields } => CoreAttributeType::struct_(
            name.clone(),
            fields.iter().map(proto_to_core_struct_field).collect(),
        ),
        ProtoAttributeType::Union { members } => {
            CoreAttributeType::union(members.iter().map(proto_to_core_attribute_type).collect())
        }
        ProtoAttributeType::Custom {
            name,
            base,
            pattern,
            length,
        } => CoreAttributeType::custom(
            if name.is_empty() {
                None
            } else {
                Some(carina_core::schema::TypeIdentity::from_dotted(name))
            },
            proto_to_core_attribute_type(base),
            // carina#3364: carry the schema `pattern`/`length` so the
            // host's `validate_custom` can enforce them; dropping them
            // here is why a violating value only failed at `apply`.
            pattern.clone(),
            *length,
            legacy_validator(|_| Ok(())),
            None,
        ),
        // CustomEnum: carries a mandatory identity, lifted from the
        // wire-form flat `(name, namespace)` pair via the
        // `enum_identity` helper. Matches the post-#3222 core
        // schema split — enum-shaped Customs expand the namespaced
        // shorthand before the validator runs.
        ProtoAttributeType::CustomEnum {
            name,
            base,
            namespace,
            dsl_transform,
        } => CoreAttributeType::enum_with_base(
            carina_core::schema::enum_identity(name, Some(namespace.as_str())),
            proto_to_core_attribute_type(base),
            None,
            vec![],
            None,
            dsl_transform
                .as_deref()
                .and_then(carina_core::schema::dsl_transform_for),
        ),
        // Cyclic CFN struct reference (carina#3340). The host's
        // structural counterpart is `AttributeType::ref_`; the matching
        // `ResourceSchema.defs` map is converted alongside in
        // `proto_to_core_schema` so resolution at walk-sites succeeds.
        ProtoAttributeType::Ref { name } => CoreAttributeType::ref_(name.clone()),
    }
}

fn proto_to_core_struct_field(f: &ProtoStructField) -> CoreStructField {
    CoreStructField {
        name: f.name.clone(),
        field_type: proto_to_core_attribute_type(&f.field_type),
        required: f.required,
        description: f.description.clone(),
        provider_name: f.provider_name.clone(),
        block_name: f.block_name.clone(),
        // The WIT contract does not transmit `deferred_populate`
        // (carina#3034). The annotation lives entirely in the host-
        // side schema (set by codegen output in
        // `carina-provider-aws/src/schemas/generated/`), which is
        // loaded directly via `SchemaRegistry` rather than crossing
        // the WASM boundary.
        deferred_populate: false,
    }
}

fn _proto_to_core_attribute_schema(a: &ProtoAttributeSchema) -> CoreAttributeSchema {
    CoreAttributeSchema {
        name: a.name.clone(),
        attr_type: proto_to_core_attribute_type(&a.attr_type),
        required: a.required,
        default: a.default.as_ref().map(proto_to_core_value),
        description: a.description.clone(),
        completions: None,
        provider_name: a.provider_name.clone(),
        create_only: a.create_only,
        read_only: a.read_only,
        removable: a.removable,
        block_name: a.block_name.clone(),
        write_only: a.write_only,
        identity: a.identity,
        // See `proto_to_core_struct_field` for the rationale.
        deferred_populate: false,
    }
}

pub fn proto_to_core_schema(s: &ProtoResourceSchema) -> CoreResourceSchema {
    CoreResourceSchema {
        resource_type: s.resource_type.clone(),
        attributes: s
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), _proto_to_core_attribute_schema(v)))
            .collect(),
        description: s.description.clone(),
        validator: None,
        kind: proto_to_core_schema_kind(s.kind),
        name_attribute: s.name_attribute.clone(),
        force_replace: s.force_replace,
        operation_config: s.operation_config.as_ref().map(|c| CoreOperationConfig {
            delete_timeout_secs: c.delete_timeout_secs,
            delete_max_retries: c.delete_max_retries,
            create_timeout_secs: c.create_timeout_secs,
            create_max_retries: c.create_max_retries,
        }),
        exclusive_required: s.exclusive_required.clone(),
        default_wait_timeout: None,
        default_wait_interval: None,
        // Cyclic CFN struct definitions reachable via Ref (carina#3340).
        defs: s
            .defs
            .iter()
            .map(|(k, v)| (k.clone(), proto_to_core_attribute_type(v)))
            .collect(),
    }
}

fn proto_to_core_schema_kind(k: ProtoSchemaKind) -> CoreSchemaKind {
    match k {
        ProtoSchemaKind::Managed => CoreSchemaKind::Resource,
        ProtoSchemaKind::DataSource => CoreSchemaKind::DataSource,
    }
}

fn core_to_proto_schema_kind(k: CoreSchemaKind) -> ProtoSchemaKind {
    match k {
        CoreSchemaKind::Resource => ProtoSchemaKind::Managed,
        CoreSchemaKind::DataSource => ProtoSchemaKind::DataSource,
    }
}

fn core_to_proto_attribute_type(t: &CoreAttributeType) -> ProtoAttributeType {
    // `raw_shape()` is the Ref-preserving projection (carina#3349 / #3352).
    // `shape(defs)` would auto-resolve Ref and either flatten the
    // structure (acyclic) or infinite-loop (cyclic CFN schemas like
    // WAFv2 WebACL.Statement); the wire form must transmit Ref verbatim
    // so the receiver can rebuild from its own copy of `defs`. Aligns
    // this provider with awscc#284.
    match t.raw_shape() {
        CoreRawShape::String => ProtoAttributeType::String,
        CoreRawShape::Int => ProtoAttributeType::Int,
        CoreRawShape::Float => ProtoAttributeType::Float,
        CoreRawShape::Bool => ProtoAttributeType::Bool,
        // `Duration` is now a first-class proto variant (carina#3166) so
        // providers can declare Duration-typed schema attributes and the
        // host's type checker accepts DSL literals like `30min` / `1h` /
        // `15s` against them. The WIT *value* boundary is still
        // integer-seconds (see carina-plugin-host wasm_convert.rs:60-76),
        // but the *type* boundary now round-trips faithfully.
        CoreRawShape::Duration => ProtoAttributeType::Duration,
        CoreRawShape::Enum {
            identity,
            values: Some(values),
            dsl_aliases,
            ..
        } => ProtoAttributeType::StringEnum {
            values: values.to_vec(),
            name: identity.kind.clone(),
            // The wire form still carries the dotted prefix as a flat
            // string. `dotted_prefix()` is the inverse of
            // `enum_identity`: provider + segments without the
            // trailing `kind`.
            namespace: identity.dotted_prefix(),
            dsl_aliases: dsl_aliases.to_vec(),
        },
        CoreRawShape::List { inner, ordered } => ProtoAttributeType::List {
            inner: Box::new(core_to_proto_attribute_type(inner)),
            ordered,
        },
        CoreRawShape::Map { key, value: inner } => ProtoAttributeType::Map {
            inner: Box::new(core_to_proto_attribute_type(inner)),
            key: Box::new(core_to_proto_attribute_type(key)),
        },
        CoreRawShape::Struct { name, fields } => ProtoAttributeType::Struct {
            name: name.to_string(),
            fields: fields.iter().map(core_to_proto_struct_field).collect(),
        },
        CoreRawShape::Custom {
            identity,
            base,
            pattern,
            length,
            ..
        } => ProtoAttributeType::Custom {
            // Serialize the structured identity to its dotted display
            // form for the wire. The host's `TypeIdentity::from_dotted`
            // parses it back on the other side, so the provider axis
            // survives the JSON round-trip.
            name: identity.map(|id| id.to_string()).unwrap_or_default(),
            base: Box::new(core_to_proto_attribute_type(base)),
            // carina#3364: carry the schema `pattern`/`length` across the
            // wire so the host can enforce them at validate/plan time.
            pattern: pattern.map(|s| s.to_string()),
            length,
        },
        // Enum without a closed value list carries the enum-shorthand marker as a type-level
        // fact (carina#3222); on the wire form it still travels as a
        // separate `CustomEnum` variant with the dotted prefix as a
        // flat string.
        CoreRawShape::Enum {
            identity,
            base,
            values: None,
            ..
        } => ProtoAttributeType::CustomEnum {
            name: identity.kind.clone(),
            base: Box::new(core_to_proto_attribute_type(base)),
            namespace: identity.dotted_prefix().unwrap_or_default(),
            dsl_transform: None,
        },
        CoreRawShape::Union(members) => ProtoAttributeType::Union {
            members: members.iter().map(core_to_proto_attribute_type).collect(),
        },
        // Cyclic CFN struct reference (carina#3340). Passes through
        // unchanged so the host can reconstruct the structural Ref
        // against its own copy of `ResourceSchema.defs`.
        CoreRawShape::Ref(name) => ProtoAttributeType::Ref {
            name: name.to_string(),
        },
    }
}

fn core_to_proto_struct_field(f: &CoreStructField) -> ProtoStructField {
    ProtoStructField {
        name: f.name.clone(),
        field_type: core_to_proto_attribute_type(&f.field_type),
        required: f.required,
        description: f.description.clone(),
        block_name: f.block_name.clone(),
        provider_name: f.provider_name.clone(),
    }
}

fn core_to_proto_attribute_schema(a: &CoreAttributeSchema) -> ProtoAttributeSchema {
    ProtoAttributeSchema {
        name: a.name.clone(),
        attr_type: core_to_proto_attribute_type(&a.attr_type),
        required: a.required,
        default: a.default.as_ref().map(core_to_proto_value),
        description: a.description.clone(),
        create_only: a.create_only,
        read_only: a.read_only,
        write_only: a.write_only,
        block_name: a.block_name.clone(),
        provider_name: a.provider_name.clone(),
        removable: a.removable,
        identity: a.identity,
    }
}

pub fn core_to_proto_schema(s: &CoreResourceSchema) -> ProtoResourceSchema {
    ProtoResourceSchema {
        resource_type: s.resource_type.clone(),
        attributes: s
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), core_to_proto_attribute_schema(v)))
            .collect(),
        description: s.description.clone(),
        kind: core_to_proto_schema_kind(s.kind),
        name_attribute: s.name_attribute.clone(),
        force_replace: s.force_replace,
        operation_config: s.operation_config.as_ref().map(|c| ProtoOperationConfig {
            delete_timeout_secs: c.delete_timeout_secs,
            delete_max_retries: c.delete_max_retries,
            create_timeout_secs: c.create_timeout_secs,
            create_max_retries: c.create_max_retries,
        }),
        validators: vec![],
        exclusive_required: s.exclusive_required.clone(),
        // Cyclic CFN struct definitions reachable via Ref (carina#3340).
        defs: s
            .defs
            .iter()
            .map(|(k, v)| (k.clone(), core_to_proto_attribute_type(v)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_enum_name_roundtrip() {
        let core_type = CoreAttributeType::enum_(
            carina_core::schema::enum_identity("VersioningStatus", Some("aws.s3.Bucket")),
            Some(vec!["Enabled".to_string(), "Suspended".to_string()]),
            vec![
                ("Enabled".to_string(), "enabled".to_string()),
                ("Suspended".to_string(), "suspended".to_string()),
            ],
            None,
            None,
        );
        let proto_type = core_to_proto_attribute_type(&core_type);
        match &proto_type {
            ProtoAttributeType::StringEnum {
                values,
                name,
                namespace,
                dsl_aliases,
            } => {
                assert_eq!(name, "VersioningStatus");
                assert_eq!(
                    values,
                    &vec!["Enabled".to_string(), "Suspended".to_string()]
                );
                assert_eq!(namespace.as_deref(), Some("aws.s3.Bucket"));
                assert_eq!(
                    dsl_aliases,
                    &vec![
                        ("Enabled".to_string(), "enabled".to_string()),
                        ("Suspended".to_string(), "suspended".to_string()),
                    ]
                );
            }
            _ => panic!("Expected StringEnum"),
        }
        let roundtrip = proto_to_core_attribute_type(&proto_type);
        if let CoreRawShape::Enum {
            identity,
            values: Some(values),
            ..
        } = roundtrip.raw_shape()
        {
            assert_eq!(identity.kind, "VersioningStatus");
            assert_eq!(values, &["Enabled", "Suspended"]);
        } else {
            panic!("Expected enum");
        }
    }

    /// Regression for aws#395 / carina#3364: a `Custom` attribute's schema
    /// `pattern` and `length` constraints MUST cross the WASM boundary in
    /// BOTH directions. If they are dropped, `carina validate` cannot
    /// enforce them and a violating value only fails at `apply`. Asserts
    /// the constraints reach the proto wire form and survive the
    /// proto -> core round-trip.
    #[test]
    fn custom_pattern_and_length_cross_proto_boundary_both_ways() {
        let pattern = "^[a-z]+$";
        let length = (Some(1u64), Some(256u64));
        let core_type = CoreAttributeType::custom(
            Some(carina_core::schema::TypeIdentity::from_dotted(
                "aws.example.Resource.SomeConstrained",
            )),
            CoreAttributeType::string(),
            Some(pattern.to_string()),
            Some(length),
            legacy_validator(|_| Ok(())),
            None,
        );

        // core -> proto: the constraint must reach the wire form.
        let proto_type = core_to_proto_attribute_type(&core_type);
        match &proto_type {
            ProtoAttributeType::Custom {
                pattern: proto_pattern,
                length: proto_length,
                ..
            } => {
                assert_eq!(proto_pattern.as_deref(), Some(pattern));
                assert_eq!(*proto_length, Some(length));
            }
            other => panic!("Expected Custom, got {other:?}"),
        }

        // proto -> core round-trip: the constraint must survive.
        let roundtripped = proto_to_core_attribute_type(&proto_type);
        match roundtripped.raw_shape() {
            CoreRawShape::Custom {
                pattern: rt_pattern,
                length: rt_length,
                ..
            } => {
                assert_eq!(rt_pattern, Some(pattern));
                assert_eq!(rt_length, Some(length));
            }
            other => panic!("Expected Custom, got {other:?}"),
        }
    }

    /// An anonymous pattern-only `Custom` (`identity: None`, no `length`)
    /// is the common generated shape for a string attribute carrying just
    /// a CloudFormation `pattern`. The `identity: None` path crosses the
    /// boundary via the `name.is_empty()` branch, so it gets its own
    /// coverage.
    #[test]
    fn anonymous_custom_pattern_only_crosses_proto_boundary() {
        let pattern = "^[\\w\\-]+$";
        let core_type = CoreAttributeType::custom(
            None,
            CoreAttributeType::string(),
            Some(pattern.to_string()),
            None,
            legacy_validator(|_| Ok(())),
            None,
        );

        let proto_type = core_to_proto_attribute_type(&core_type);
        let roundtripped = proto_to_core_attribute_type(&proto_type);
        match roundtripped.raw_shape() {
            CoreRawShape::Custom {
                identity,
                pattern: rt_pattern,
                length: rt_length,
                ..
            } => {
                assert!(identity.is_none(), "anonymous custom stays anonymous");
                assert_eq!(rt_pattern, Some(pattern));
                assert_eq!(rt_length, None);
            }
            other => panic!("Expected Custom, got {other:?}"),
        }
    }
}
