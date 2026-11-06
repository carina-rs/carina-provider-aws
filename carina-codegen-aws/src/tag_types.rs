//! Project each resource's Smithy tag collection into its DSL string map.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use carina_core::schema::AttributeType;
use carina_smithy::{Shape, ShapeRef, SmithyModel};

use super::{
    Bounds, ResourceDef, ValueTypeOverride, length_from_traits, map_type_code, string_refinement,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TagString {
    pattern: Option<String>,
    length: Bounds<u64>,
}

impl TagString {
    fn from_member(model: &SmithyModel, member: &ShapeRef) -> Result<Self> {
        let traits = match model.get_shape(&member.target) {
            Some(Shape::String(shape)) => &shape.traits,
            None if member.target == "smithy.api#String" => &member.traits,
            other => anyhow::bail!(
                "Tag member {} must target a string, got {other:?}",
                member.target
            ),
        };
        let pattern = member
            .traits
            .get("smithy.api#pattern")
            .or_else(|| traits.get("smithy.api#pattern"))
            .and_then(|pattern| pattern.as_str())
            .map(str::to_string);
        Ok(Self {
            pattern,
            length: length_from_traits(&member.traits).or_else(|| length_from_traits(traits)),
        })
    }

    fn refinement(&self) -> ValueTypeOverride {
        string_refinement(self.pattern.as_deref(), self.length)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagMapType {
    key: TagString,
    value: TagString,
    length: Bounds<u64>,
}

impl TagMapType {
    /// Lists retain their entry-count constraint when converted to a map.
    /// A standalone Tag structure has only its Key/Value refinements.
    pub fn from_shape(model: &SmithyModel, target: &str) -> Result<Option<Self>> {
        let (key, value, length) = if let Some(Shape::Map(shape)) = model.get_shape(target) {
            (&shape.key, &shape.value, length_from_traits(&shape.traits))
        } else if let Some(Shape::List(shape)) = model.get_shape(target) {
            let Some(Shape::Structure(tag)) = model.get_shape(&shape.member.target) else {
                return Ok(None);
            };
            let (Some(key), Some(value)) = (tag.members.get("Key"), tag.members.get("Value"))
            else {
                return Ok(None);
            };
            (key, value, length_from_traits(&shape.traits))
        } else if let Some(Shape::Structure(shape)) = model.get_shape(target) {
            let (Some(key), Some(value)) = (shape.members.get("Key"), shape.members.get("Value"))
            else {
                return Ok(None);
            };
            (key, value, None)
        } else {
            return Ok(None);
        };
        Ok(Some(Self {
            key: TagString::from_member(model, key)?,
            value: TagString::from_member(model, value)?,
            length,
        }))
    }

    pub fn code(&self) -> String {
        map_type_code(
            &self.key.refinement().type_code,
            &self.value.refinement().type_code,
            self.length,
        )
    }

    pub fn attribute_type(&self) -> AttributeType {
        AttributeType::refined_map(
            self.key.refinement().attribute_type,
            self.value.refinement().attribute_type,
            self.length,
        )
    }
}

fn collect_sources(
    model: &SmithyModel,
    target: &str,
    sources: &mut BTreeMap<String, TagMapType>,
    visiting: &mut HashSet<String>,
) -> Result<()> {
    if !visiting.insert(target.to_string()) {
        anyhow::bail!("Recursive tag source {target}");
    }
    if let Some(tags) = TagMapType::from_shape(model, target)? {
        sources.insert(target.to_string(), tags);
    } else if let Some(Shape::Structure(shape)) = model.get_shape(target) {
        for (name, member) in &shape.members {
            if ["tags", "tagset", "tagging", "tagspecifications"]
                .contains(&name.to_ascii_lowercase().as_str())
            {
                collect_sources(model, &member.target, sources, visiting)?;
            }
        }
    } else if let Some(Shape::List(shape)) = model.get_shape(target) {
        collect_sources(model, &shape.member.target, sources, visiting)?;
    }
    visiting.remove(target);
    Ok(())
}

/// All operations contributing tags must agree; never pick one set of bounds.
pub fn resource_tag_type(res: &ResourceDef, model: &SmithyModel) -> Result<TagMapType> {
    let mut sources = BTreeMap::new();
    let mut visiting = HashSet::new();
    for structure in res.schema_structure.into_iter().chain(res.read_structure) {
        collect_sources(
            model,
            &format!("{}#{structure}", res.service_namespace),
            &mut sources,
            &mut visiting,
        )?;
    }
    for operation in std::iter::once(res.create_op)
        .chain(
            res.update_ops
                .iter()
                .filter(|op| op.fields.cross_service_namespace().is_none())
                .map(|op| op.operation),
        )
        .chain(res.read_ops.iter().map(|op| op.operation))
        .chain(res.tag_operations().iter().copied())
    {
        let id = format!("{}#{operation}", res.service_namespace);
        if let Some(Shape::Operation(op)) = model.get_shape(&id) {
            for member in op.input.iter().chain(op.output.iter()) {
                collect_sources(model, &member.target, &mut sources, &mut visiting)?;
            }
        }
    }
    let Some((source, tags)) = sources.first_key_value() else {
        anyhow::ensure!(
            !model
                .shapes
                .keys()
                .any(|id| model.shape_kind(id) == Some(carina_smithy::ShapeKind::Service)),
            "{} has no tag source in its Smithy service model",
            res.name
        );
        // Partial models (including schema-only fixtures) may declare no tags.
        // They supply no key, value, or collection refinements.
        return Ok(TagMapType::default());
    };
    for (other_source, other_tags) in &sources {
        anyhow::ensure!(
            tags == other_tags,
            "{} has conflicting tag bounds: {source} {tags:?}; {other_source} {other_tags:?}",
            res.name
        );
    }
    // Compile the model's patterns at the same admission gate as generated schemas.
    carina_core::schema::ResourceSchema::new(res.name)
        .attribute(
            carina_core::schema::AttributeSchema::optional("tags", tags.attribute_type()).build(),
        )
        .validate()
        .with_context(|| format!("{} tag source {source}", res.name))?;
    Ok(tags.clone())
}
