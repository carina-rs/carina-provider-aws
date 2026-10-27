//! Conversions between carina-core types and carina-provider-protocol types.
//!
//! This is a local copy of the convert module from carina-plugin-host,
//! needed because carina-plugin-host depends on wasmtime which cannot
//! compile to wasm32-wasip2.

use std::collections::HashMap;

use carina_core::provider::{
    AppliedState as CoreAppliedState, ReadOutcome as CoreReadOutcome,
    UpdateOutcome as CoreUpdateOutcome,
};
use carina_core::resource::{
    ConcreteValue, DataSource as CoreDataSource, DeferredValue, Directives as CoreDirectives,
    RedactedSecretHash, Resource as CoreResource, ResourceId as CoreResourceId,
    ResourceIdentity as CoreResourceIdentity, ResourceIdentityError as CoreResourceIdentityError,
    State as CoreState, Value as CoreValue,
};
use carina_core::schema::{
    AttributeSchema as CoreAttributeSchema, AttributeSchemaParts as CoreAttributeSchemaParts,
    AttributeType as CoreAttributeType, InputMode as CoreInputMode, RawShape as CoreRawShape,
    ResourceSchema as CoreResourceSchema, ResourceSchemaParts as CoreResourceSchemaParts,
    ResourceValidation as CoreResourceValidation, SchemaKind as CoreSchemaKind,
    StructField as CoreStructField, StructFieldParts as CoreStructFieldParts,
    UniqueNameSpec as CoreUniqueNameSpec,
};
use carina_provider_protocol::types::{
    AppliedPersistedState as ProtoAppliedPersistedState, AppliedState as ProtoAppliedState,
    AttributeSchema as ProtoAttributeSchema, AttributeType as ProtoAttributeType,
    Directives as ProtoDirectives, PartialReadDiagnostic as ProtoPartialReadDiagnostic,
    PersistedState as ProtoPersistedState, PersistedValue as ProtoPersistedValue,
    ReadOutcome as ProtoReadOutcome, Resource as ProtoResource, ResourceId as ProtoResourceId,
    ResourceSchema as ProtoResourceSchema, SchemaKind as ProtoSchemaKind, State as ProtoState,
    StructField as ProtoStructField, UniqueNameSpec as ProtoUniqueNameSpec,
    UpdateOutcome as ProtoUpdateOutcome, Value as ProtoValue,
};

#[derive(Debug, thiserror::Error)]
#[error("persisted state contains an invalid redacted-secret digest")]
pub struct PersistedValueDecodeError;

// -- ResourceId --

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
        CoreValue::Concrete(ConcreteValue::String(s)) => {
            ProtoValue::String(s.expose_plaintext().to_string())
        }
        // Protocol v6 makes unresolved enum identifiers a host-boundary
        // violation. The converter is intentionally infallible, so retain an
        // explicit invariant guard instead of smuggling DSL spelling over the
        // wire as a string.
        CoreValue::Concrete(ConcreteValue::EnumIdentifier(_)) => {
            unreachable!("unresolved enum identifier must not reach protocol conversion")
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
        CoreValue::Concrete(ConcreteValue::StringList(items)) => ProtoValue::StringList(
            items
                .iter()
                .map(|item| item.expose_plaintext().to_string())
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
        // includes the inner plaintext — a leak. Raw secrets retain the
        // existing redacted-sentinel behavior. A saved-state-only
        // `RedactedSecret` must never cross the desired provider boundary;
        // this legacy infallible converter rejects that invariant violation
        // by panicking rather than passing the persisted hash through.
        CoreValue::Deferred(DeferredValue::Secret(_)) => {
            ProtoValue::String("<redacted-secret>".to_string())
        }
        CoreValue::Deferred(DeferredValue::RedactedSecret { .. }) => {
            panic!("redacted saved-state secret must not reach a provider")
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
        CoreValue::Deferred(
            DeferredValue::FunctionCall { name, .. } | DeferredValue::Unavailable { name, .. },
        ) => ProtoValue::String(format!("<unresolved-fn:{name}>")),
        CoreValue::Deferred(DeferredValue::Coalesce { .. }) => {
            ProtoValue::String("<unresolved-coalesce>".to_string())
        }
        CoreValue::Deferred(DeferredValue::ResolvedFunctionCall { function, .. }) => {
            ProtoValue::String(format!("<unresolved-fn:{}>", function.name))
        }
        CoreValue::Deferred(DeferredValue::Access(_)) => {
            ProtoValue::String("<unresolved-access>".to_string())
        }
        CoreValue::Deferred(DeferredValue::Unknown(_)) => {
            ProtoValue::String("<unknown>".to_string())
        }
    }
}

pub fn proto_to_core_value(v: &ProtoValue) -> CoreValue {
    match v {
        ProtoValue::String(s) => CoreValue::Concrete(ConcreteValue::String(s.clone().into())),
        ProtoValue::Int(i) => CoreValue::Concrete(ConcreteValue::Int(*i)),
        ProtoValue::Float(f) => CoreValue::Concrete(ConcreteValue::Float(*f)),
        ProtoValue::Bool(b) => CoreValue::Concrete(ConcreteValue::Bool(*b)),
        ProtoValue::List(l) => CoreValue::Concrete(ConcreteValue::List(
            l.iter().map(proto_to_core_value).collect(),
        )),
        ProtoValue::StringList(items) => CoreValue::Concrete(ConcreteValue::StringList(
            items.iter().cloned().map(Into::into).collect(),
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

/// Convert a core value at a persisted-state position.
///
/// This is deliberately separate from [`core_to_proto_value`]: only this
/// typed position may carry a redacted-secret digest.
pub fn core_to_proto_persisted_value(v: &CoreValue) -> ProtoPersistedValue {
    match v {
        CoreValue::Concrete(ConcreteValue::String(s)) => {
            ProtoPersistedValue::String(s.expose_plaintext().to_string())
        }
        CoreValue::Concrete(ConcreteValue::EnumIdentifier(_)) => {
            unreachable!("unresolved enum identifier must not reach protocol conversion")
        }
        CoreValue::Concrete(ConcreteValue::CanonicalEnum(c)) => {
            ProtoPersistedValue::String(c.api_value().to_string())
        }
        CoreValue::Concrete(ConcreteValue::Int(i)) => ProtoPersistedValue::Int(*i),
        CoreValue::Concrete(ConcreteValue::Float(f)) => ProtoPersistedValue::Float(*f),
        CoreValue::Concrete(ConcreteValue::Bool(b)) => ProtoPersistedValue::Bool(*b),
        CoreValue::Concrete(ConcreteValue::Duration(d)) => {
            ProtoPersistedValue::Int(d.as_secs() as i64)
        }
        CoreValue::Concrete(ConcreteValue::List(l)) => {
            ProtoPersistedValue::List(l.iter().map(core_to_proto_persisted_value).collect())
        }
        CoreValue::Concrete(ConcreteValue::StringList(items)) => ProtoPersistedValue::StringList(
            items
                .iter()
                .map(|item| item.expose_plaintext().to_string())
                .collect(),
        ),
        CoreValue::Concrete(ConcreteValue::Map(m)) => ProtoPersistedValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), core_to_proto_persisted_value(v)))
                .collect(),
        ),
        CoreValue::Deferred(DeferredValue::Secret(_)) => {
            panic!("live secret must be redacted before persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::RedactedSecret { hash }) => {
            ProtoPersistedValue::RedactedSecret(hash.encoded_digest())
        }
        CoreValue::Deferred(DeferredValue::ResourceRef { .. }) => {
            panic!("unresolved resource reference must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::BindingRef { .. }) => {
            panic!("unresolved binding reference must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::Interpolation(_)) => {
            panic!("unresolved interpolation must not reach persisted-state conversion")
        }
        CoreValue::Deferred(
            DeferredValue::FunctionCall { .. } | DeferredValue::Unavailable { .. },
        ) => {
            panic!("unresolved built-in function call must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::Coalesce { .. }) => {
            panic!("unresolved coalesce expression must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::ResolvedFunctionCall { .. }) => {
            panic!("unresolved user function call must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::Access(_)) => {
            panic!("unresolved access expression must not reach persisted-state conversion")
        }
        CoreValue::Deferred(DeferredValue::Unknown(_)) => {
            panic!("unknown value must not reach persisted-state conversion")
        }
    }
}

fn core_to_proto_persisted_value_map(
    m: &HashMap<String, CoreValue>,
) -> HashMap<String, ProtoPersistedValue> {
    m.iter()
        .map(|(k, v)| (k.clone(), core_to_proto_persisted_value(v)))
        .collect()
}

/// Decode a value from a persisted-state position.
///
/// Ordinary strings are always strings. Only the protocol's structural
/// `RedactedSecret` variant gains secret semantics.
pub fn proto_to_core_persisted_value(
    v: &ProtoPersistedValue,
) -> Result<CoreValue, PersistedValueDecodeError> {
    match v {
        ProtoPersistedValue::String(s) => {
            Ok(CoreValue::Concrete(ConcreteValue::String(s.clone().into())))
        }
        ProtoPersistedValue::RedactedSecret(digest) => {
            let hash =
                RedactedSecretHash::from_encoded_digest(digest).ok_or(PersistedValueDecodeError)?;
            Ok(CoreValue::Deferred(DeferredValue::RedactedSecret { hash }))
        }
        ProtoPersistedValue::Int(i) => Ok(CoreValue::Concrete(ConcreteValue::Int(*i))),
        ProtoPersistedValue::Float(f) => Ok(CoreValue::Concrete(ConcreteValue::Float(*f))),
        ProtoPersistedValue::Bool(b) => Ok(CoreValue::Concrete(ConcreteValue::Bool(*b))),
        ProtoPersistedValue::List(l) => l
            .iter()
            .map(proto_to_core_persisted_value)
            .collect::<Result<Vec<_>, _>>()
            .map(ConcreteValue::List)
            .map(CoreValue::Concrete),
        ProtoPersistedValue::StringList(items) => Ok(CoreValue::Concrete(
            ConcreteValue::StringList(items.iter().cloned().map(Into::into).collect()),
        )),
        ProtoPersistedValue::Map(m) => m
            .iter()
            .map(|(k, v)| proto_to_core_persisted_value(v).map(|value| (k.clone(), value)))
            .collect::<Result<indexmap::IndexMap<_, _>, _>>()
            .map(ConcreteValue::Map)
            .map(CoreValue::Concrete),
    }
}

fn proto_to_core_persisted_value_map(
    m: &HashMap<String, ProtoPersistedValue>,
) -> Result<HashMap<String, CoreValue>, PersistedValueDecodeError> {
    m.iter()
        .map(|(k, v)| proto_to_core_persisted_value(v).map(|value| (k.clone(), value)))
        .collect()
}

// -- State --

pub fn core_to_proto_state(s: &CoreState) -> ProtoState {
    ProtoState {
        id: s.id.to_wire(),
        identifier: s.identifier.clone(),
        attributes: core_to_proto_value_map(&s.attributes),
        exists: s.exists,
    }
}

pub fn core_to_proto_applied_state(s: &CoreAppliedState) -> ProtoAppliedState {
    ProtoAppliedState {
        id: s.id().to_wire(),
        identifier: s.identifier().clone(),
        attributes: core_to_proto_value_map(s.attributes()),
    }
}

pub fn core_to_proto_read_outcome(outcome: CoreReadOutcome) -> ProtoReadOutcome {
    match outcome {
        CoreReadOutcome::NotFound { id } => ProtoReadOutcome::NotFound { id: id.to_wire() },
        CoreReadOutcome::Found { state } => ProtoReadOutcome::Found {
            state: core_to_proto_applied_state(&state),
        },
    }
}

pub fn proto_to_core_persisted_state(
    s: &ProtoPersistedState,
) -> Result<CoreState, PersistedValueDecodeError> {
    let id = proto_to_core_resource_id(&s.id);
    if s.exists {
        let mut state = CoreState::existing(id, proto_to_core_persisted_value_map(&s.attributes)?);
        if let Some(ref ident) = s.identifier {
            state = state.with_identifier(ident);
        }
        Ok(state)
    } else {
        Ok(CoreState::not_found(id))
    }
}

// -- UpdateOutcome --

fn core_to_proto_update_state(
    state: &CoreAppliedState,
    mut state_after_patch: ProtoPersistedState,
) -> ProtoAppliedPersistedState {
    let mut attributes = core_to_proto_persisted_value_map(state.attributes());
    for (key, value) in state_after_patch.attributes.drain() {
        attributes.entry(key).or_insert(value);
    }
    ProtoAppliedPersistedState {
        id: state.id().to_wire(),
        identifier: state.identifier().clone(),
        attributes,
    }
}

pub fn core_to_proto_update_outcome(
    outcome: CoreUpdateOutcome,
    state_after_patch: ProtoPersistedState,
) -> ProtoUpdateOutcome {
    match outcome {
        CoreUpdateOutcome::Success { state } => ProtoUpdateOutcome::Success {
            state: core_to_proto_update_state(&state, state_after_patch),
        },
        CoreUpdateOutcome::PartialSuccess { state, diagnostic } => {
            ProtoUpdateOutcome::PartialSuccess {
                state: core_to_proto_update_state(&state, state_after_patch),
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
        id: r.id.to_wire(),
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
    let CoreStructFieldParts {
        name,
        field_type,
        input_mode,
        description,
        provider_name,
        block_name,
        deferred_populate,
        write_only,
    } = f.parts();
    let (required, read_only) = core_input_mode_to_proto(input_mode);
    ProtoStructField {
        name: name.to_string(),
        field_type: core_to_proto_attribute_type(field_type),
        required,
        description: description.map(str::to_owned),
        block_name: block_name.map(str::to_owned),
        provider_name: provider_name.map(str::to_owned),
        read_only,
        write_only,
        deferred_populate,
    }
}

fn core_to_proto_attribute_schema(a: &CoreAttributeSchema) -> ProtoAttributeSchema {
    let CoreAttributeSchemaParts {
        name,
        attr_type,
        input_mode,
        default,
        description,
        // The protocol has no field for completions.
        completions: _completions,
        provider_name,
        create_only,
        removable,
        block_name,
        write_only,
        identity,
        deferred_populate,
    } = a.parts();
    let (required, read_only) = core_input_mode_to_proto(input_mode);
    ProtoAttributeSchema {
        name: name.to_string(),
        attr_type: core_to_proto_attribute_type(attr_type),
        required,
        default: default.map(core_to_proto_value),
        description: description.map(str::to_owned),
        create_only,
        read_only,
        write_only,
        block_name: block_name.map(str::to_owned),
        provider_name: provider_name.map(str::to_owned),
        removable,
        identity,
        deferred_populate,
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
    let CoreResourceSchemaParts {
        resource_type,
        attributes,
        description,
        validation,
        kind,
        unique_name,
        operation_config,
        exclusive_required,
        // The protocol has no fields for wait defaults.
        default_wait_timeout: _default_wait_timeout,
        default_wait_interval: _default_wait_interval,
        defs,
    } = s.parts();

    ProtoResourceSchema {
        resource_type: resource_type.to_string(),
        attributes: attributes
            .iter()
            .map(|(k, v)| (k.clone(), core_to_proto_attribute_schema(v)))
            .collect(),
        description: description.map(str::to_owned),
        kind: core_to_proto_schema_kind(kind),
        unique_name: core_to_proto_unique_name(unique_name),
        operation_config: operation_config.cloned(),
        validators: match validation {
            CoreResourceValidation::Declared(validators) => validators.clone(),
            CoreResourceValidation::None => vec![],
            // Native callbacks are not portable across the protocol.
            CoreResourceValidation::InProcess(_native_callback) => vec![],
        },
        exclusive_required: exclusive_required.to_vec(),
        // Cyclic CFN struct definitions reachable via Ref (carina#3340).
        defs: defs
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
    fn schema_transport_clones_the_shared_operation_config() {
        let schema =
            carina_provider_aws::schemas::generated::ec2::nat_gateway::ec2_nat_gateway_config()
                .schema;
        let operation_config = carina_core::schema::OperationConfig {
            create_timeout_secs: Some(11),
            update_timeout_secs: Some(22),
            delete_timeout_secs: Some(33),
            create_max_retries: Some(4),
            delete_max_retries: Some(5),
        };
        let schema = schema.with_operation_config(operation_config.clone());

        assert_eq!(
            core_to_proto_schema(&schema).operation_config,
            Some(operation_config)
        );
    }

    #[test]
    fn nested_struct_field_write_only_round_trips_to_protocol() {
        let fields = [
            (
                CoreStructField::new("client_secret", CoreAttributeType::string()).write_only(),
                true,
            ),
            (
                CoreStructField::new("client_id", CoreAttributeType::string()),
                false,
            ),
        ];

        for (field, expected) in fields {
            let protocol = core_to_proto_struct_field(&field);
            assert_eq!(protocol.write_only, expected, "field {}", field.name());
        }
    }

    #[test]
    #[should_panic(expected = "unresolved enum identifier must not reach protocol conversion")]
    fn unresolved_enum_identifier_is_rejected_at_provider_boundary() {
        let value = CoreValue::Concrete(ConcreteValue::enum_identifier("aws.example.Mode.dsl"));

        let _ = core_to_proto_value(&value);
    }

    #[test]
    #[should_panic(expected = "redacted saved-state secret must not reach a provider")]
    fn redacted_saved_state_secret_is_rejected_at_provider_boundary() {
        let secret = CoreValue::Deferred(DeferredValue::Secret(Box::new(CoreValue::Concrete(
            ConcreteValue::String("boundary-test-secret".into()),
        ))));
        let value = carina_core::value::redact_secrets_in_value(&secret)
            .expect("redact saved-state placeholder");

        let _ = core_to_proto_value(&value);
    }

    #[test]
    fn update_outcome_carries_unpatched_redacted_secret_as_persisted_value() {
        let secret = CoreValue::Deferred(DeferredValue::Secret(Box::new(CoreValue::Concrete(
            ConcreteValue::String("stored-write-only-secret".into()),
        ))));
        let redacted = carina_core::value::redact_secrets_in_value(&secret)
            .expect("redact saved-state placeholder");
        let CoreValue::Deferred(DeferredValue::RedactedSecret { hash }) = &redacted else {
            panic!("expected a redacted secret")
        };
        let digest = hash.encoded_digest();
        let id = ProtoResourceId {
            provider: "aws".into(),
            resource_type: "test.Resource".into(),
            identity: "example".into(),
        };
        let request = carina_provider_protocol::types::UpdateRequest {
            from: ProtoPersistedState {
                id: id.clone(),
                identifier: Some("resource-id".into()),
                attributes: HashMap::from([
                    (
                        "token".into(),
                        ProtoPersistedValue::RedactedSecret(digest.clone()),
                    ),
                    ("color".into(), ProtoPersistedValue::String("red".into())),
                ]),
                exists: true,
            },
            patch: carina_provider_protocol::types::UpdatePatch {
                ops: vec![carina_provider_protocol::types::PatchOp::Replace {
                    key: "color".into(),
                    value: ProtoValue::String("blue".into()),
                }],
            },
        };

        let core_from = proto_to_core_persisted_state(&request.from)
            .expect("decode structurally persisted current state");
        assert!(matches!(
            core_from.attributes.get("token"),
            Some(CoreValue::Deferred(DeferredValue::RedactedSecret { .. }))
        ));

        // Simulate an AWS read-back that omits the write-only token. The
        // protocol-level state-after-patch baseline must keep that untouched
        // persisted value in the outcome without presenting it to the cloud.
        let provider_state = CoreAppliedState::new(
            proto_to_core_resource_id(&id),
            "resource-id",
            HashMap::from([(
                "color".into(),
                CoreValue::Concrete(ConcreteValue::String("blue".into())),
            )]),
        )
        .expect("test provider identifier must be non-empty");
        let outcome = core_to_proto_update_outcome(
            CoreUpdateOutcome::Success {
                state: provider_state,
            },
            request.state_after_patch(),
        );

        let ProtoUpdateOutcome::Success { state } = outcome else {
            panic!("expected a successful update outcome")
        };
        assert_eq!(
            state.attributes.get("token"),
            Some(&ProtoPersistedValue::RedactedSecret(digest))
        );
        assert_eq!(
            state.attributes.get("color"),
            Some(&ProtoPersistedValue::String("blue".into()))
        );
    }

    #[test]
    fn test_string_enum_name_transport() {
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
    }

    /// Regression for aws#395 / carina#3364: a refined string attribute's schema
    /// `pattern` and `length` constraints MUST cross the WASM boundary. If they
    /// are dropped, `carina validate` cannot enforce them and a violating value
    /// only fails at `apply`.
    #[test]
    fn refined_string_pattern_and_length_cross_proto_boundary() {
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
    }

    #[test]
    fn custom_enum_dsl_transform_crosses_proto_boundary() {
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

        match core_to_proto_attribute_type(&core_type) {
            ProtoAttributeType::String {
                identity,
                pattern: proto_pattern,
                length: proto_length,
                ..
            } => {
                assert!(identity.is_none(), "anonymous custom stays anonymous");
                assert_eq!(proto_pattern.as_deref(), Some(pattern));
                assert_eq!(proto_length, None);
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
                        "PENDING_VALIDATION".to_string().into(),
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
    }
}
