//! Conversions between carina-core types and carina-provider-protocol types.
//!
//! This is a local copy of the convert module from carina-plugin-host,
//! needed because carina-plugin-host depends on wasmtime which cannot
//! compile to wasm32-wasip2.

use std::collections::HashMap;

use carina_core::provider::{
    CreateOutcome as CoreCreateOutcome, UpdateOutcome as CoreUpdateOutcome,
};
use carina_core::resource::{
    ConcreteValue, DataSource as CoreDataSource, DeferredValue, Directives as CoreDirectives,
    Resource as CoreResource, ResourceId as CoreResourceId,
    ResourceIdentity as CoreResourceIdentity, ResourceIdentityError as CoreResourceIdentityError,
    ResourceIdentityState as CoreResourceIdentityState, State as CoreState, Value as CoreValue,
};
use carina_core::schema::{
    AttributeSchema as CoreAttributeSchema, AttributeType as CoreAttributeType,
    InputMode as CoreInputMode, OperationConfig as CoreOperationConfig, RawShape as CoreRawShape,
    ResourceSchema as CoreResourceSchema, SchemaKind as CoreSchemaKind,
    StructField as CoreStructField, UniqueNameSpec as CoreUniqueNameSpec, legacy_validator,
};
use carina_provider_protocol::types::{
    AttributeSchema as ProtoAttributeSchema, AttributeType as ProtoAttributeType,
    CreateOutcome as ProtoCreateOutcome, Directives as ProtoDirectives,
    OperationConfig as ProtoOperationConfig, PartialReadDiagnostic as ProtoPartialReadDiagnostic,
    Resource as ProtoResource, ResourceId as ProtoResourceId,
    ResourceSchema as ProtoResourceSchema, SchemaKind as ProtoSchemaKind, State as ProtoState,
    StructField as ProtoStructField, UniqueNameSpec as ProtoUniqueNameSpec,
    UpdateOutcome as ProtoUpdateOutcome, Value as ProtoValue,
};

// -- ResourceId --

pub fn core_to_proto_resource_id(id: &CoreResourceId) -> ProtoResourceId {
    let identity = match id.identity_state() {
        CoreResourceIdentityState::Pending(_) => String::new(),
        CoreResourceIdentityState::Resolved(identity) => identity.as_str().to_string(),
    };
    ProtoResourceId {
        provider: id.provider.clone(),
        resource_type: id.resource_type.clone(),
        identity,
    }
}

pub fn proto_to_core_resource_id(id: &ProtoResourceId) -> CoreResourceId {
    // `ProtoResourceId` predates carina#3038's `provider_instance` field;
    // the WIT wire format does not carry it. Pass `None` so the
    // reconstructed `CoreResourceId` routes through the default provider.
    match CoreResourceIdentity::try_from(id.identity.clone()) {
        Ok(identity) => CoreResourceId::with_provider_identity(
            id.provider.clone(),
            id.resource_type.clone(),
            identity,
            None,
        ),
        Err(CoreResourceIdentityError::Empty) => CoreResourceId::pending_with_provider(
            id.provider.clone(),
            id.resource_type.clone(),
            None,
        ),
    }
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
        CoreValue::Concrete(ConcreteValue::String(s)) => ProtoValue::String(s.clone()),
        CoreValue::Concrete(ConcreteValue::EnumIdentifier(s)) => {
            ProtoValue::String(s.as_str().to_string())
        }
        CoreValue::Concrete(ConcreteValue::CanonicalEnum(c)) => {
            ProtoValue::String(c.api_value().to_string())
        }
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
        CoreValue::Concrete(ConcreteValue::StringList(items)) => {
            ProtoValue::StringList(items.clone())
        }
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
        ProtoValue::StringList(items) => {
            CoreValue::Concrete(ConcreteValue::StringList(items.clone()))
        }
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

// -- CreateOutcome --

pub fn core_to_proto_create_outcome(outcome: CoreCreateOutcome) -> ProtoCreateOutcome {
    match outcome {
        CoreCreateOutcome::Success { state } => ProtoCreateOutcome::Success {
            state: core_to_proto_state(&state),
        },
        CoreCreateOutcome::PartialSuccess { state, diagnostic } => {
            ProtoCreateOutcome::PartialSuccess {
                state: core_to_proto_state(&state),
                diagnostic: ProtoPartialReadDiagnostic {
                    reason: diagnostic.reason().to_string(),
                    missing_attributes: diagnostic.missing_attributes().to_vec(),
                },
            }
        }
    }
}

// -- UpdateOutcome --

pub fn core_to_proto_update_outcome(outcome: CoreUpdateOutcome) -> ProtoUpdateOutcome {
    match outcome {
        CoreUpdateOutcome::Success { state } => ProtoUpdateOutcome::Success {
            state: core_to_proto_state(&state),
        },
        CoreUpdateOutcome::PartialSuccess { state, diagnostic } => {
            ProtoUpdateOutcome::PartialSuccess {
                state: core_to_proto_state(&state),
                diagnostic: ProtoPartialReadDiagnostic {
                    reason: diagnostic.reason().to_string(),
                    missing_attributes: diagnostic.missing_attributes().to_vec(),
                },
            }
        }
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
    let mut resource = CoreResource::from_id(proto_to_core_resource_id(&r.id));
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
    let mut data_source = match CoreResourceIdentity::try_from(r.id.identity.clone()) {
        Ok(identity) => CoreDataSource::with_provider(
            r.id.provider.clone(),
            r.id.resource_type.clone(),
            identity,
            None,
        ),
        Err(CoreResourceIdentityError::Empty) => CoreDataSource::pending_with_provider(
            r.id.provider.clone(),
            r.id.resource_type.clone(),
            None,
        ),
    };
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
        ProtoAttributeType::String {
            pattern,
            length,
            to_dsl,
            identity,
            ..
        } => CoreAttributeType::refined_string(
            identity
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(carina_core::schema::TypeIdentity::from_dotted),
            pattern.clone(),
            *length,
            to_dsl.clone(),
        ),
        ProtoAttributeType::Int { range, identity } => CoreAttributeType::refined_int(
            identity
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(carina_core::schema::TypeIdentity::from_dotted),
            *range,
        ),
        ProtoAttributeType::Float { range, identity } => CoreAttributeType::refined_float(
            identity
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(carina_core::schema::TypeIdentity::from_dotted),
            *range,
        ),
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
        ProtoAttributeType::List {
            element_type,
            ordered,
            length,
            ..
        } => {
            let inner_t = proto_to_core_attribute_type(element_type);
            if *ordered {
                CoreAttributeType::refined_list(
                    inner_t,
                    true,
                    *length,
                    legacy_validator(|_| Ok(())),
                )
            } else {
                CoreAttributeType::refined_list(
                    inner_t,
                    false,
                    *length,
                    legacy_validator(|_| Ok(())),
                )
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
            to_dsl,
            ..
        } => {
            let identity = if name.is_empty() {
                None
            } else {
                Some(carina_core::schema::TypeIdentity::from_dotted(name))
            };
            let base = proto_to_core_attribute_type(base);
            match base.raw_shape() {
                CoreRawShape::String { .. } => CoreAttributeType::refined_string(
                    identity,
                    pattern.clone(),
                    *length,
                    to_dsl.clone(),
                ),
                CoreRawShape::Int { .. } => CoreAttributeType::refined_int(
                    identity,
                    length.map(|(min, max)| (min.map(|v| v as i64), max.map(|v| v as i64))),
                ),
                CoreRawShape::Float { .. } => CoreAttributeType::refined_float(identity, None),
                CoreRawShape::List {
                    element_type,
                    ordered,
                    ..
                } => {
                    let _ = (identity, pattern);
                    CoreAttributeType::refined_list(
                        element_type.clone(),
                        ordered,
                        *length,
                        legacy_validator(|_| Ok(())),
                    )
                }
                other => panic!("unsupported Custom base in provider protocol: {other:?}"),
            }
        }
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
            dsl_transform.clone(),
        ),
        // Cyclic CFN struct reference (carina#3340). The host's
        // structural counterpart is `AttributeType::ref_`; the matching
        // `ResourceSchema.defs` map is converted alongside in
        // `proto_to_core_schema` so resolution at walk-sites succeeds.
        ProtoAttributeType::Ref { name } => CoreAttributeType::ref_(name.clone()),
    }
}

fn proto_to_core_struct_field(f: &ProtoStructField) -> CoreStructField {
    let ProtoStructField {
        name,
        field_type,
        required,
        description,
        block_name,
        provider_name,
        read_only,
        deferred_populate,
    } = f;
    CoreStructField {
        name: name.clone(),
        field_type: proto_to_core_attribute_type(field_type),
        input_mode: proto_input_mode_to_core(*required, *read_only),
        description: description.clone(),
        provider_name: provider_name.clone(),
        block_name: block_name.clone(),
        deferred_populate: *deferred_populate,
    }
}

fn proto_input_mode_to_core(required: bool, read_only: bool) -> CoreInputMode {
    match (required, read_only) {
        (false, false) => CoreInputMode::Optional,
        (true, false) => CoreInputMode::Required,
        (false, true) => CoreInputMode::ProviderPopulated,
        (true, true) => panic!("schema input mode cannot be both required and read-only"),
    }
}

fn proto_to_core_attribute_schema(a: &ProtoAttributeSchema) -> CoreAttributeSchema {
    let ProtoAttributeSchema {
        name,
        attr_type,
        required,
        default,
        description,
        create_only,
        read_only,
        write_only,
        block_name,
        provider_name,
        removable,
        identity,
        deferred_populate,
    } = a;
    CoreAttributeSchema {
        name: name.clone(),
        attr_type: proto_to_core_attribute_type(attr_type),
        input_mode: proto_input_mode_to_core(*required, *read_only),
        default: default.as_ref().map(proto_to_core_value),
        description: description.clone(),
        completions: None,
        provider_name: provider_name.clone(),
        create_only: *create_only,
        removable: *removable,
        block_name: block_name.clone(),
        write_only: *write_only,
        identity: *identity,
        deferred_populate: *deferred_populate,
    }
}

pub fn proto_to_core_schema(s: &ProtoResourceSchema) -> CoreResourceSchema {
    CoreResourceSchema {
        resource_type: s.resource_type.clone(),
        attributes: s
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), proto_to_core_attribute_schema(v)))
            .collect(),
        description: s.description.clone(),
        validator: None,
        kind: proto_to_core_schema_kind(s.kind),
        unique_name: proto_to_core_unique_name(&s.unique_name),
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

fn proto_to_core_unique_name(spec: &ProtoUniqueNameSpec) -> CoreUniqueNameSpec {
    match spec {
        ProtoUniqueNameSpec::Attribute(attribute) => {
            CoreUniqueNameSpec::Attribute(attribute.clone())
        }
        ProtoUniqueNameSpec::Coexisting => CoreUniqueNameSpec::Coexisting,
        ProtoUniqueNameSpec::Conflicting => CoreUniqueNameSpec::Conflicting,
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
        CoreRawShape::String {
            identity,
            pattern,
            length,
            to_dsl,
            ..
        } => ProtoAttributeType::String {
            pattern: pattern.map(|s| s.to_string()),
            length,
            validate: None,
            to_dsl: to_dsl.cloned(),
            identity: identity.map(|id| id.to_string()),
        },
        CoreRawShape::Int {
            identity, range, ..
        } => ProtoAttributeType::Int {
            range,
            identity: identity.map(|id| id.to_string()),
        },
        CoreRawShape::Float {
            identity, range, ..
        } => ProtoAttributeType::Float {
            range,
            identity: identity.map(|id| id.to_string()),
        },
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
        CoreRawShape::List {
            element_type,
            ordered,
            length,
            ..
        } => ProtoAttributeType::List {
            element_type: Box::new(core_to_proto_attribute_type(element_type)),
            ordered,
            length,
            validate: None,
        },
        CoreRawShape::Map { key, value: inner } => ProtoAttributeType::Map {
            inner: Box::new(core_to_proto_attribute_type(inner)),
            key: Box::new(core_to_proto_attribute_type(key)),
        },
        CoreRawShape::Struct { name, fields } => ProtoAttributeType::Struct {
            name: name.to_string(),
            fields: fields.iter().map(core_to_proto_struct_field).collect(),
        },
        // Enum without a closed value list carries the enum-shorthand marker as a type-level
        // fact (carina#3222); on the wire form it still travels as a
        // separate `CustomEnum` variant with the dotted prefix as a
        // flat string.
        CoreRawShape::Enum {
            identity,
            base,
            values: None,
            to_dsl,
            ..
        } => ProtoAttributeType::CustomEnum {
            name: identity.kind.clone(),
            base: Box::new(core_to_proto_attribute_type(base)),
            namespace: identity.dotted_prefix().unwrap_or_default(),
            dsl_transform: to_dsl.cloned(),
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
    let CoreStructField {
        name,
        field_type,
        input_mode,
        description,
        provider_name,
        block_name,
        deferred_populate,
    } = f;
    let (required, read_only) = core_input_mode_to_proto(*input_mode);
    ProtoStructField {
        name: name.clone(),
        field_type: core_to_proto_attribute_type(field_type),
        required,
        description: description.clone(),
        block_name: block_name.clone(),
        provider_name: provider_name.clone(),
        read_only,
        deferred_populate: *deferred_populate,
    }
}

fn core_to_proto_attribute_schema(a: &CoreAttributeSchema) -> ProtoAttributeSchema {
    let CoreAttributeSchema {
        name,
        attr_type,
        input_mode,
        default,
        description,
        completions: _completions,
        provider_name,
        create_only,
        removable,
        block_name,
        write_only,
        identity,
        deferred_populate,
    } = a;
    let (required, read_only) = core_input_mode_to_proto(*input_mode);
    ProtoAttributeSchema {
        name: name.clone(),
        attr_type: core_to_proto_attribute_type(attr_type),
        required,
        default: default.as_ref().map(core_to_proto_value),
        description: description.clone(),
        create_only: *create_only,
        read_only,
        write_only: *write_only,
        block_name: block_name.clone(),
        provider_name: provider_name.clone(),
        removable: *removable,
        identity: *identity,
        deferred_populate: *deferred_populate,
    }
}

fn core_input_mode_to_proto(input_mode: CoreInputMode) -> (bool, bool) {
    match input_mode {
        CoreInputMode::Optional => (false, false),
        CoreInputMode::Required => (true, false),
        CoreInputMode::ProviderPopulated => (false, true),
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
        unique_name: core_to_proto_unique_name(&s.unique_name),
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

fn core_to_proto_unique_name(spec: &CoreUniqueNameSpec) -> ProtoUniqueNameSpec {
    match spec {
        CoreUniqueNameSpec::Attribute(attribute) => {
            ProtoUniqueNameSpec::Attribute(attribute.clone())
        }
        CoreUniqueNameSpec::Coexisting => ProtoUniqueNameSpec::Coexisting,
        CoreUniqueNameSpec::Conflicting => ProtoUniqueNameSpec::Conflicting,
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

    /// Regression for aws#395 / carina#3364: a refined string attribute's schema
    /// `pattern` and `length` constraints MUST cross the WASM boundary in
    /// BOTH directions. If they are dropped, `carina validate` cannot
    /// enforce them and a violating value only fails at `apply`. Asserts
    /// the constraints reach the proto wire form and survive the
    /// proto -> core round-trip.
    #[test]
    fn refined_string_pattern_and_length_cross_proto_boundary_both_ways() {
        let pattern = "^[a-z]+$";
        let length = (Some(1u64), Some(256u64));
        let core_type = CoreAttributeType::refined_string(
            Some(carina_core::schema::TypeIdentity::from_dotted(
                "aws.example.Resource.SomeConstrained",
            )),
            Some(pattern.to_string()),
            Some(length),
            None,
        );

        // core -> proto: the constraint must reach the wire form.
        let proto_type = core_to_proto_attribute_type(&core_type);
        match &proto_type {
            ProtoAttributeType::String {
                identity,
                pattern: proto_pattern,
                length: proto_length,
                ..
            } => {
                assert_eq!(
                    identity.as_deref(),
                    Some("aws.example.Resource.SomeConstrained")
                );
                assert_eq!(proto_pattern.as_deref(), Some(pattern));
                assert_eq!(*proto_length, Some(length));
            }
            other => panic!("Expected String, got {other:?}"),
        }

        // proto -> core round-trip: the constraint must survive.
        let roundtripped = proto_to_core_attribute_type(&proto_type);
        match roundtripped.raw_shape() {
            CoreRawShape::String {
                identity,
                pattern: rt_pattern,
                length: rt_length,
                ..
            } => {
                assert_eq!(
                    identity.map(|id| id.to_string()).as_deref(),
                    Some("aws.example.Resource.SomeConstrained")
                );
                assert_eq!(rt_pattern, Some(pattern));
                assert_eq!(rt_length, Some(length));
            }
            other => panic!("Expected String, got {other:?}"),
        }
    }

    #[test]
    fn custom_enum_dsl_transform_crosses_proto_boundary_both_ways() {
        let core_type = CoreAttributeType::enum_with_base(
            carina_core::schema::enum_identity("Region", Some("aws")),
            CoreAttributeType::string(),
            None,
            vec![],
            None,
            Some(carina_core::schema::DslTransform::HyphenToUnderscore),
        );

        let proto_type = core_to_proto_attribute_type(&core_type);
        match &proto_type {
            ProtoAttributeType::CustomEnum { dsl_transform, .. } => {
                assert_eq!(
                    dsl_transform.as_ref(),
                    Some(&carina_core::schema::DslTransform::HyphenToUnderscore)
                );
            }
            other => panic!("Expected CustomEnum, got {other:?}"),
        }

        let roundtripped = proto_to_core_attribute_type(&proto_type);
        match roundtripped.raw_shape() {
            CoreRawShape::Enum { to_dsl, .. } => {
                assert_eq!(
                    to_dsl,
                    Some(&carina_core::schema::DslTransform::HyphenToUnderscore)
                );
            }
            other => panic!("Expected Enum, got {other:?}"),
        }
    }

    /// An anonymous pattern-only refined string (`identity: None`, no `length`)
    /// is the common generated shape for a string attribute carrying just
    /// a CloudFormation `pattern`. The `identity: None` path crosses the
    /// boundary via the `name.is_empty()` branch, so it gets its own
    /// coverage.
    #[test]
    fn anonymous_refined_string_pattern_only_crosses_proto_boundary() {
        let pattern = "^[\\w\\-]+$";
        let core_type =
            CoreAttributeType::refined_string(None, Some(pattern.to_string()), None, None);

        let proto_type = core_to_proto_attribute_type(&core_type);
        let roundtripped = proto_to_core_attribute_type(&proto_type);
        match roundtripped.raw_shape() {
            CoreRawShape::String {
                identity,
                pattern: rt_pattern,
                length: rt_length,
                ..
            } => {
                assert!(identity.is_none(), "anonymous custom stays anonymous");
                assert_eq!(rt_pattern, Some(pattern));
                assert_eq!(rt_length, None);
            }
            other => panic!("Expected String, got {other:?}"),
        }
    }

    #[test]
    fn old_custom_string_wire_payload_converts_to_refined_string() {
        let proto_type = ProtoAttributeType::Custom {
            name: "aws.example.Resource.Legacy".to_string(),
            base: Box::new(ProtoAttributeType::String {
                pattern: None,
                length: None,
                validate: None,
                to_dsl: None,
                identity: None,
            }),
            pattern: Some("^[a-z]+$".to_string()),
            length: Some((Some(1), Some(9))),
            validate: None,
            to_dsl: None,
        };

        let core_type = proto_to_core_attribute_type(&proto_type);
        match core_type.raw_shape() {
            CoreRawShape::String {
                identity,
                pattern,
                length,
                ..
            } => {
                assert_eq!(
                    identity.map(|id| id.to_string()).as_deref(),
                    Some("aws.example.Resource.Legacy")
                );
                assert_eq!(pattern, Some("^[a-z]+$"));
                assert_eq!(length, Some((Some(1), Some(9))));
            }
            other => panic!("Expected String, got {other:?}"),
        }
    }

    #[test]
    fn schema_transport_preserves_nested_input_mode_deferred_populate_and_default() {
        let schema = CoreResourceSchema::new("acm.Certificate")
            .attribute(CoreAttributeSchema::new(
                "domain_validation_options",
                CoreAttributeType::struct_(
                    "DomainValidation",
                    vec![
                        CoreStructField::new("resource_record", CoreAttributeType::string())
                            .read_only()
                            .deferred_populate(),
                    ],
                ),
            ))
            .attribute(
                CoreAttributeSchema::new("status", CoreAttributeType::string())
                    .with_default(CoreValue::Concrete(ConcreteValue::String(
                        "PENDING_VALIDATION".to_string(),
                    )))
                    .read_only()
                    .deferred_populate(),
            );

        let proto = core_to_proto_schema(&schema);
        let proto_domain_validation = &proto.attributes["domain_validation_options"];
        let ProtoAttributeType::Struct {
            name: proto_struct_name,
            fields: proto_fields,
        } = &proto_domain_validation.attr_type
        else {
            panic!("domain_validation_options must remain a Struct");
        };
        assert_eq!(proto_struct_name, "DomainValidation");
        let proto_resource_record = &proto_fields[0];
        assert!(!proto_resource_record.required);
        assert!(proto_resource_record.read_only);
        assert!(proto_resource_record.deferred_populate);

        let proto_status = &proto.attributes["status"];
        assert!(!proto_status.required);
        assert!(proto_status.read_only);
        assert!(proto_status.deferred_populate);
        assert_eq!(
            proto_status.default,
            Some(ProtoValue::String("PENDING_VALIDATION".to_string()))
        );

        let round_tripped = proto_to_core_schema(&proto);
        let domain_validation = &round_tripped.attributes["domain_validation_options"];
        let CoreRawShape::Struct {
            name: round_tripped_struct_name,
            fields: round_tripped_fields,
        } = domain_validation.attr_type.raw_shape()
        else {
            panic!("round-tripped domain_validation_options must remain a Struct");
        };
        assert_eq!(round_tripped_struct_name, "DomainValidation");
        let CoreStructField {
            name: field_name,
            field_type: _field_type,
            input_mode: field_input_mode,
            description: _field_description,
            provider_name: _field_provider_name,
            block_name: _field_block_name,
            deferred_populate: field_deferred_populate,
        } = &round_tripped_fields[0];
        assert_eq!(field_name, "resource_record");
        assert_eq!(*field_input_mode, CoreInputMode::ProviderPopulated);
        assert!(*field_deferred_populate);

        let CoreAttributeSchema {
            name: status_name,
            attr_type: _status_type,
            input_mode: status_input_mode,
            default: status_default,
            description: _status_description,
            completions: _status_completions,
            provider_name: _status_provider_name,
            create_only: _status_create_only,
            removable: _status_removable,
            block_name: _status_block_name,
            write_only: _status_write_only,
            identity: _status_identity,
            deferred_populate: status_deferred_populate,
        } = &round_tripped.attributes["status"];
        assert_eq!(status_name, "status");
        assert_eq!(*status_input_mode, CoreInputMode::ProviderPopulated);
        assert!(matches!(
            status_default,
            Some(CoreValue::Concrete(ConcreteValue::String(value)))
                if value == "PENDING_VALIDATION"
        ));
        assert!(*status_deferred_populate);
    }
}
