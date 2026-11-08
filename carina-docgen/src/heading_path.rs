//! Schema-walk positions shared by declaration identity and heading text.

use std::fmt;

use crate::TypeEdge;

/// A schema field name, created only from a field edge of the type walk.
/// Rendered heading text cannot be used as a segment directly.
///
/// ```compile_fail,E0451
/// use carina_docgen::HeadingField;
/// let field = HeadingField { name: "a[].b".into() };
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HeadingField {
    name: String,
}

impl HeadingField {
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

/// One position in the typed schema walk, including identity-only union hops.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MarkdownTypePosition {
    Field(HeadingField),
    Item,
    Key,
    Value,
    Member(usize),
}

impl From<TypeEdge<'_>> for MarkdownTypePosition {
    fn from(edge: TypeEdge<'_>) -> Self {
        match edge {
            TypeEdge::Field(name) => Self::Field(HeadingField { name: name.into() }),
            TypeEdge::Item => Self::Item,
            TypeEdge::Key => Self::Key,
            TypeEdge::Value => Self::Value,
            TypeEdge::Member(index) => Self::Member(index),
        }
    }
}

/// A heading path built exclusively from typed schema edges.
///
/// Segments are private and can only be added by following a [`TypeEdge`].
/// Map keys and values have distinct spellings. Union member indices remain
/// part of equality and ordering, but are omitted from text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HeadingPath {
    segments: Vec<MarkdownTypePosition>,
}

impl HeadingPath {
    /// Start at a root field of the schema walk.
    pub fn root(field_name: &str) -> Self {
        Self {
            segments: vec![TypeEdge::Field(field_name).into()],
        }
    }

    /// Follow a typed schema edge without accepting rendered path fragments.
    pub fn child(&self, edge: TypeEdge<'_>) -> Self {
        let mut path = self.clone();
        path.segments.push(edge.into());
        path
    }

    pub fn segments(&self) -> &[MarkdownTypePosition] {
        &self.segments
    }

    /// Keep the visible schema edges while removing identity-only union hops.
    pub fn visible(&self) -> Self {
        Self {
            segments: self
                .segments
                .iter()
                .filter(|segment| !matches!(segment, MarkdownTypePosition::Member(_)))
                .cloned()
                .collect(),
        }
    }

    pub fn last_field(&self) -> Option<&str> {
        self.segments
            .iter()
            .rev()
            .find_map(|segment| match segment {
                MarkdownTypePosition::Field(field) => Some(field.as_str()),
                MarkdownTypePosition::Item
                | MarkdownTypePosition::Key
                | MarkdownTypePosition::Value
                | MarkdownTypePosition::Member(_) => None,
            })
    }

    pub fn has_union_member(&self) -> bool {
        self.segments
            .iter()
            .any(|segment| matches!(segment, MarkdownTypePosition::Member(_)))
    }
}

impl fmt::Display for HeadingPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for segment in &self.segments {
            match segment {
                MarkdownTypePosition::Field(field) => {
                    if !first {
                        f.write_str(".")?;
                    }
                    f.write_str(field.as_str())?;
                    first = false;
                }
                MarkdownTypePosition::Item => f.write_str("[]")?,
                MarkdownTypePosition::Key => f.write_str(" keys")?,
                MarkdownTypePosition::Value => f.write_str("['<key>']")?,
                // Not rendered; identity only.
                MarkdownTypePosition::Member(_) => {}
            }
        }
        Ok(())
    }
}
