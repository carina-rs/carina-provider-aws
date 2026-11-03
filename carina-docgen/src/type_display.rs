use std::fmt;
use std::hash::Hash;

use crate::{PageBuilder, TableCell};

/// A type description composed of literal text, code, and typed links.
///
/// Rendering is deferred until the display is written to a page. In particular,
/// a collection can become one code span only when all its contents are literal
/// text. A link or an explicitly styled code part is never flattened into it.
///
/// Displays cannot be interpolated into already-rendered Markdown:
///
/// ```compile_fail
/// use carina_docgen::TypeDisplay;
/// let link = TypeDisplay::link((), "Struct(X)");
/// let wrapped = format!("`List<{}>`", link);
/// ```
#[derive(Clone, Debug)]
pub struct TypeDisplay<K> {
    kind: DisplayKind<K>,
}

#[derive(Clone, Debug)]
enum DisplayKind<K> {
    Text(String),
    Code(String),
    Link { target: K, label: String },
    Sequence(Vec<TypeDisplay<K>>),
    Union(Vec<TypeDisplay<K>>),
    List(Box<TypeDisplay<K>>),
    Map(Box<TypeDisplay<K>>),
}

impl<K> TypeDisplay<K> {
    /// Write literal text, escaping inline Markdown syntax when rendered.
    pub fn text(literal: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Text(literal.into()),
        }
    }

    /// Style literal text as code, choosing delimiters that preserve backticks.
    ///
    /// A link display cannot be passed to this literal constructor:
    ///
    /// ```compile_fail
    /// use carina_docgen::TypeDisplay;
    /// let link = TypeDisplay::link((), "Struct(X)");
    /// let code = TypeDisplay::<()>::code(link);
    /// ```
    pub fn code(literal: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Code(literal.into()),
        }
    }

    pub fn link(target: K, label: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Link {
                target,
                label: label.into(),
            },
        }
    }

    /// Compose inline parts. The page writer coalesces consecutive code literals
    /// before choosing delimiters, including across nested sequences.
    pub fn sequence(parts: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: DisplayKind::Sequence(parts.into_iter().collect()),
        }
    }

    pub fn union(members: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: DisplayKind::Union(members.into_iter().collect()),
        }
    }

    pub fn list(item: Self) -> Self {
        Self {
            kind: DisplayKind::List(Box::new(item)),
        }
    }

    pub fn map(value: Self) -> Self {
        Self {
            kind: DisplayKind::Map(Box::new(value)),
        }
    }

    // This extracts source literals, never rendered Markdown. Links and code
    // parts have no representation here, even inside nested collections.
    fn literal_text(&self) -> Option<String> {
        match &self.kind {
            DisplayKind::Text(text) => Some(text.clone()),
            DisplayKind::Code(_) | DisplayKind::Link { .. } => None,
            DisplayKind::Sequence(parts) => parts
                .iter()
                .map(Self::literal_text)
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.concat()),
            DisplayKind::Union(members) => members
                .iter()
                .map(Self::literal_text)
                .collect::<Option<Vec<_>>>()
                .map(|members| members.join(" | ")),
            DisplayKind::List(item) => item.literal_text().map(|item| format!("List<{item}>")),
            DisplayKind::Map(value) => value.literal_text().map(|value| {
                if value == "String" {
                    "Map".to_string()
                } else {
                    format!("Map<String, {value}>")
                }
            }),
        }
    }
}

impl<K: Clone> TypeDisplay<K> {
    pub fn table_cell(&self) -> TableCell<K> {
        let mut cell = TableCell::new();
        self.write_parts(&mut cell);
        cell
    }

    fn write_parts(&self, cell: &mut TableCell<K>) {
        match &self.kind {
            DisplayKind::Text(text) => cell.push_literal(text),
            DisplayKind::Code(literal) => cell.push_code(literal),
            DisplayKind::Link { target, label } => cell.push_link(target.clone(), label),
            DisplayKind::Sequence(parts) => {
                for part in parts {
                    part.write_parts(cell);
                }
            }
            DisplayKind::Union(members) => {
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        cell.push_text(" \\| ");
                    }
                    member.write_parts(cell);
                }
            }
            DisplayKind::List(item) => {
                if let Some(literal) = self.literal_text() {
                    cell.push_code(literal);
                } else {
                    cell.push_text("List\\<");
                    item.write_parts(cell);
                    cell.push_text("\\>");
                }
            }
            DisplayKind::Map(value) => match self.literal_text() {
                Some(literal) if literal == "Map" => cell.push_text(literal),
                Some(literal) => cell.push_code(literal),
                None => {
                    cell.push_text("Map\\<String, ");
                    value.write_parts(cell);
                    cell.push_text("\\>");
                }
            },
        }
    }
}

impl<K: Clone + Eq + Hash + fmt::Debug> TypeDisplay<K> {
    pub fn write_to(&self, page: &mut PageBuilder<K>) {
        page.write_inline_parts(self.table_cell(), false);
    }
}

// Shared by ordinary literals and link labels. Keep in step with the renderer
// extensions in registry_options: tables, strikethrough, and autolinks.
pub(super) fn escape_literal_text(literal: &str) -> String {
    let mut escaped = String::with_capacity(literal.len());
    for (index, character) in literal.char_indices() {
        let autolink = match character {
            ':' => literal[index..].starts_with("://"),
            '.' => literal[..index].ends_with("www"),
            _ => false,
        };
        let block_marker = match character {
            '-' | '+' | '=' => literal[..index]
                .rsplit(['\r', '\n'])
                .next()
                .is_some_and(|prefix| prefix.trim().is_empty()),
            '.' | ')' => {
                let prefix = literal[..index]
                    .rsplit(['\r', '\n'])
                    .next()
                    .unwrap_or_default()
                    .trim_start();
                !prefix.is_empty()
                    && prefix.chars().all(|c| c.is_ascii_digit())
                    && (index + 1 == literal.len() || literal[index + 1..].starts_with([' ', '\t']))
            }
            _ => false,
        };
        match character {
            '&' => escaped.push_str("&amp;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '!' | '|' | '~' | '@' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ if autolink || block_marker => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}
