//! Walk declarations, preserving identity layers rather than effective/member restrictions.
//! Also used by the explicit, offline awscc manifest updater.
use std::collections::{BTreeMap, BTreeSet};

use carina_core::schema::{
    InputMode, PatternAttributeType, PatternForm, PatternResourceSchema, PatternSpec, RawShape,
};
use serde_json::{Value, json};

fn pattern(pattern: Option<&PatternSpec>) -> Value {
    pattern.map_or(Value::Null, |pattern| {
        let parts = pattern.parts();
        json!({"text": pattern.as_str(), "mode": format!("{:?}", parts.mode)})
    })
}

pub fn scalar_definition<R: PatternForm>(ty: &PatternAttributeType<R>) -> Option<(String, Value)> {
    let (identity, definition) = match ty.raw_shape() {
        RawShape::String { identity_layer, .. } => (
            identity_layer.identity?,
            json!({"shape": "string", "pattern": pattern(identity_layer.pattern),
                "length": identity_layer.length, "transform": identity_layer.to_dsl,
                "callback": identity_layer.validate.is_some()}),
        ),
        RawShape::Reference {
            identity,
            pattern: source,
            length,
        } => (
            identity,
            json!({"shape": "reference", "pattern": pattern(source), "length": length,
                "transform": null, "callback": false}),
        ),
        RawShape::Enum {
            identity,
            values,
            dsl_aliases,
            to_dsl,
            validate,
            ..
        } => (
            identity,
            json!({"shape": "enum", "values": values, "aliases": dsl_aliases,
                "transform": to_dsl, "callback": validate.is_some()}),
        ),
        RawShape::Int { identity_layer, .. } => (
            identity_layer.identity?,
            json!({"shape": "int", "range": identity_layer.range,
                "callback": identity_layer.validate.is_some()}),
        ),
        RawShape::Float {
            identity,
            range,
            validate,
        } => (
            identity?,
            json!({"shape": "float", "range": format!("{range:?}"), "callback": validate.is_some()}),
        ),
        _ => return None,
    };
    Some((identity.to_string(), definition))
}

pub fn walk<R: PatternForm>(
    ty: &PatternAttributeType<R>,
    defs: &BTreeMap<String, PatternAttributeType<R>>,
    path: &str,
    writable: bool,
    active: &mut BTreeSet<String>,
    visit: &mut impl FnMut(&str, bool, &PatternAttributeType<R>),
) {
    match ty.raw_shape() {
        RawShape::List { element_type, .. } => {
            walk(
                element_type,
                defs,
                &format!("{path}[]"),
                writable,
                active,
                visit,
            );
        }
        RawShape::Map { key, value, .. } => {
            walk(
                key,
                defs,
                &format!("{path}{{key}}"),
                writable,
                active,
                visit,
            );
            walk(value, defs, &format!("{path}{{}}"), writable, active, visit);
        }
        RawShape::Struct { fields, .. } => {
            for field in fields.iter() {
                walk(
                    field.field_type(),
                    defs,
                    &format!("{path}.{}", field.name()),
                    writable && field.parts().declared_input_mode != InputMode::ProviderPopulated,
                    active,
                    visit,
                );
            }
        }
        RawShape::Union(members) => {
            for member in members {
                walk(member, defs, path, writable, active, visit);
            }
        }
        RawShape::Ref(name) => {
            if active.insert(name.to_string()) {
                walk(
                    defs.get(name)
                        .unwrap_or_else(|| panic!("unresolved Ref {name} at {path}")),
                    defs,
                    path,
                    writable,
                    active,
                    visit,
                );
                active.remove(name);
            }
        }
        _ => visit(path, writable, ty),
    }
}

pub fn schema_leaves<R: PatternForm>(owner: &str, schema: &PatternResourceSchema<R>) -> Vec<Value> {
    let view = if schema.is_data_source() {
        "data_source"
    } else {
        "resource"
    };
    let resource = schema.parts().resource_type;
    let service = resource.split('.').next().unwrap();
    let mut leaves = Vec::new();
    for attribute in schema.attributes().values() {
        walk(
            attribute.attr_type(),
            schema.defs(),
            attribute.name(),
            attribute.input_mode() != InputMode::ProviderPopulated,
            &mut BTreeSet::new(),
            &mut |path, writable, ty| {
                let scalar = scalar_definition(ty);
                leaves.push(json!({"owner": owner, "resource": resource, "view": view,
                    "service": service, "path": path, "writable": writable,
                    "anonymous_string": matches!(ty.raw_shape(), RawShape::String { identity_layer, .. } if identity_layer.identity.is_none()),
                    "identity": scalar.as_ref().map(|(identity, _)| identity),
                    "definition": scalar.map(|(_, definition)| definition)}));
            },
        );
    }
    leaves.sort_by_key(Value::to_string);
    leaves.dedup();
    leaves
}

/// Callback implementations have no portable equality. Owners conjoin them.
pub fn portable_definition(definition: &Value) -> Value {
    let mut result = definition.clone();
    if let Some(object) = result.as_object_mut() {
        object.remove("callback");
    }
    result
}

#[test]
fn recursive_refs_containers_unions_and_populated_ancestry_are_walked() {
    use carina_core::schema::{AttributeType, StructField};
    let defs = BTreeMap::from([(
        "Node".into(),
        AttributeType::struct_(
            "Node",
            vec![
                StructField::required("id", carina_aws_types::flow_log_id()).build(),
                StructField::provider_populated("read", AttributeType::string()).build(),
                StructField::optional("child", AttributeType::ref_("Node")).build(),
            ],
        ),
    )]);
    let ty = AttributeType::unordered_list(AttributeType::map(AttributeType::union(vec![
        AttributeType::ref_("Node"),
        AttributeType::string(),
    ])));
    let mut found = BTreeMap::new();
    walk(
        &ty,
        &defs,
        "root",
        true,
        &mut BTreeSet::new(),
        &mut |path, writable, ty| {
            found.insert(
                path.to_string(),
                (
                    writable,
                    scalar_definition(ty).map(|(identity, _)| identity),
                ),
            );
        },
    );
    assert_eq!(
        found["root[]{}.id"],
        (true, Some("aws.ec2.FlowLog.Id".into()))
    );
    assert_eq!(found["root[]{}.read"], (false, None));
    assert!(found.contains_key("root[]{key}"));
    assert!(found.contains_key("root[]{}"));
    assert_eq!(
        found.len(),
        4,
        "cycle protection must stop the recursive Node child"
    );
}
