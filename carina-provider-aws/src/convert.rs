//! Conversions between carina-core types and carina-provider-protocol types.
//!
//! This is a local copy of the convert module from carina-plugin-host,
//! needed because carina-plugin-host depends on wasmtime which cannot
//! compile to wasm32-wasip2.

use std::collections::HashMap;

use carina_core::resource::{
    LifecycleConfig as CoreLifecycle, Resource as CoreResource, ResourceId as CoreResourceId,
    State as CoreState, Value as CoreValue,
};
use carina_core::schema::{
    AttributeSchema as CoreAttributeSchema, AttributeType as CoreAttributeType,
    OperationConfig as CoreOperationConfig, ResourceSchema as CoreResourceSchema,
    StructField as CoreStructField,
};
use carina_provider_protocol::types::{
    AttributeSchema as ProtoAttributeSchema, AttributeType as ProtoAttributeType,
    LifecycleConfig as ProtoLifecycle, OperationConfig as ProtoOperationConfig,
    Resource as ProtoResource, ResourceId as ProtoResourceId,
    ResourceSchema as ProtoResourceSchema, State as ProtoState, StructField as ProtoStructField,
    Value as ProtoValue,
};

// -- ResourceId --

pub fn core_to_proto_resource_id(id: &CoreResourceId) -> ProtoResourceId {
    ProtoResourceId {
        provider: id.provider.clone(),
        resource_type: id.resource_type.clone(),
        name: id.name.clone(),
    }
}

pub fn proto_to_core_resource_id(id: &ProtoResourceId) -> CoreResourceId {
    CoreResourceId::with_provider(&id.provider, &id.resource_type, &id.name)
}

// -- Value --

pub fn core_to_proto_value(v: &CoreValue) -> ProtoValue {
    match v {
        CoreValue::String(s) => ProtoValue::String(s.clone()),
        CoreValue::Int(i) => ProtoValue::Int(*i),
        CoreValue::Float(f) => ProtoValue::Float(*f),
        CoreValue::Bool(b) => ProtoValue::Bool(*b),
        CoreValue::List(l) => ProtoValue::List(l.iter().map(core_to_proto_value).collect()),
        CoreValue::Map(m) => ProtoValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), core_to_proto_value(v)))
                .collect(),
        ),
        // ResourceRef, Interpolation, FunctionCall, Closure, Secret
        // should be resolved before reaching the provider.
        _ => ProtoValue::String(format!("{v:?}")),
    }
}

pub fn proto_to_core_value(v: &ProtoValue) -> CoreValue {
    match v {
        ProtoValue::String(s) => CoreValue::String(s.clone()),
        ProtoValue::Int(i) => CoreValue::Int(*i),
        ProtoValue::Float(f) => CoreValue::Float(*f),
        ProtoValue::Bool(b) => CoreValue::Bool(*b),
        ProtoValue::List(l) => CoreValue::List(l.iter().map(proto_to_core_value).collect()),
        ProtoValue::Map(m) => CoreValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), proto_to_core_value(v)))
                .collect(),
        ),
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
        lifecycle: core_to_proto_lifecycle(&r.lifecycle),
    }
}

// -- LifecycleConfig --

pub fn core_to_proto_lifecycle(l: &CoreLifecycle) -> ProtoLifecycle {
    ProtoLifecycle {
        force_delete: l.force_delete,
        create_before_destroy: l.create_before_destroy,
        prevent_destroy: l.prevent_destroy,
    }
}

// -- proto_to_core_resource (reverse of core_to_proto_resource) --

pub fn proto_to_core_resource(r: &ProtoResource) -> CoreResource {
    use carina_core::resource::Expr;
    let mut resource = CoreResource::with_provider(&r.id.provider, &r.id.resource_type, &r.id.name);
    resource.attributes = r
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), Expr(proto_to_core_value(v))))
        .collect();
    resource.lifecycle = CoreLifecycle {
        force_delete: r.lifecycle.force_delete,
        create_before_destroy: r.lifecycle.create_before_destroy,
        prevent_destroy: r.lifecycle.prevent_destroy,
    };
    resource
}

// -- AttributeType --

fn proto_to_core_attribute_type(t: &ProtoAttributeType) -> CoreAttributeType {
    match t {
        ProtoAttributeType::String => CoreAttributeType::String,
        ProtoAttributeType::Int => CoreAttributeType::Int,
        ProtoAttributeType::Float => CoreAttributeType::Float,
        ProtoAttributeType::Bool => CoreAttributeType::Bool,
        ProtoAttributeType::StringEnum {
            values,
            name,
            namespace,
        } => CoreAttributeType::StringEnum {
            name: name.clone(),
            values: values.clone(),
            namespace: namespace.clone(),
            to_dsl: None,
        },
        ProtoAttributeType::List { inner, ordered } => CoreAttributeType::List {
            inner: Box::new(proto_to_core_attribute_type(inner)),
            ordered: *ordered,
        },
        ProtoAttributeType::Map { inner } => {
            CoreAttributeType::Map(Box::new(proto_to_core_attribute_type(inner)))
        }
        ProtoAttributeType::Struct { name, fields } => CoreAttributeType::Struct {
            name: name.clone(),
            fields: fields.iter().map(proto_to_core_struct_field).collect(),
        },
        ProtoAttributeType::Union { members } => {
            CoreAttributeType::Union(members.iter().map(proto_to_core_attribute_type).collect())
        }
        ProtoAttributeType::Custom {
            name,
            base,
            namespace,
        } => CoreAttributeType::Custom {
            name: name.clone(),
            base: Box::new(proto_to_core_attribute_type(base)),
            validate: |_| Ok(()),
            namespace: namespace.clone(),
            to_dsl: None,
        },
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
        data_source: s.data_source,
        name_attribute: s.name_attribute.clone(),
        force_replace: s.force_replace,
        operation_config: s.operation_config.as_ref().map(|c| CoreOperationConfig {
            delete_timeout_secs: c.delete_timeout_secs,
            delete_max_retries: c.delete_max_retries,
            create_timeout_secs: c.create_timeout_secs,
            create_max_retries: c.create_max_retries,
        }),
    }
}

fn core_to_proto_attribute_type(t: &CoreAttributeType) -> ProtoAttributeType {
    match t {
        CoreAttributeType::String => ProtoAttributeType::String,
        CoreAttributeType::Int => ProtoAttributeType::Int,
        CoreAttributeType::Float => ProtoAttributeType::Float,
        CoreAttributeType::Bool => ProtoAttributeType::Bool,
        CoreAttributeType::StringEnum {
            values,
            name,
            namespace,
            ..
        } => ProtoAttributeType::StringEnum {
            values: values.clone(),
            name: name.clone(),
            namespace: namespace.clone(),
        },
        CoreAttributeType::List { inner, ordered } => ProtoAttributeType::List {
            inner: Box::new(core_to_proto_attribute_type(inner)),
            ordered: *ordered,
        },
        CoreAttributeType::Map(inner) => ProtoAttributeType::Map {
            inner: Box::new(core_to_proto_attribute_type(inner)),
        },
        CoreAttributeType::Struct { name, fields } => ProtoAttributeType::Struct {
            name: name.clone(),
            fields: fields.iter().map(core_to_proto_struct_field).collect(),
        },
        CoreAttributeType::Custom {
            name,
            base,
            namespace,
            ..
        } => ProtoAttributeType::Custom {
            name: name.clone(),
            base: Box::new(core_to_proto_attribute_type(base)),
            namespace: namespace.clone(),
        },
        CoreAttributeType::Union(members) => ProtoAttributeType::Union {
            members: members.iter().map(core_to_proto_attribute_type).collect(),
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
        data_source: s.data_source,
        name_attribute: s.name_attribute.clone(),
        force_replace: s.force_replace,
        operation_config: s.operation_config.as_ref().map(|c| ProtoOperationConfig {
            delete_timeout_secs: c.delete_timeout_secs,
            delete_max_retries: c.delete_max_retries,
            create_timeout_secs: c.create_timeout_secs,
            create_max_retries: c.create_max_retries,
        }),
        validators: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_enum_name_roundtrip() {
        let core_type = CoreAttributeType::StringEnum {
            name: "VersioningStatus".to_string(),
            values: vec!["Enabled".to_string(), "Suspended".to_string()],
            namespace: Some("aws.s3.bucket".to_string()),
            to_dsl: None,
        };
        let proto_type = core_to_proto_attribute_type(&core_type);
        match &proto_type {
            ProtoAttributeType::StringEnum {
                values,
                name,
                namespace,
            } => {
                assert_eq!(name, "VersioningStatus");
                assert_eq!(
                    values,
                    &vec!["Enabled".to_string(), "Suspended".to_string()]
                );
                assert_eq!(namespace.as_deref(), Some("aws.s3.bucket"));
            }
            _ => panic!("Expected StringEnum"),
        }
        let roundtrip = proto_to_core_attribute_type(&proto_type);
        match roundtrip {
            CoreAttributeType::StringEnum { name, values, .. } => {
                assert_eq!(name, "VersioningStatus");
                assert_eq!(values, vec!["Enabled", "Suspended"]);
            }
            _ => panic!("Expected StringEnum"),
        }
    }
}
