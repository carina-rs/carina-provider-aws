use std::fmt;
use std::hash::Hash;

use crate::{PageBuilder, TableCell};

/// A type description composed of literal text, code, and typed links.
///
/// Rendering is deferred until the display is written to a page. Collection
/// compositors preserve each child's code styling and typed links.
///
/// Displays cannot be interpolated into already-rendered Markdown:
///
/// ```compile_fail
/// use carina_docgen::{ScalarDoc, TypeDisplay, scalar_display};
/// let display: TypeDisplay<()> = scalar_display(ScalarDoc::Bool, None);
/// let wrapped = format!("`List<{}>`", display);
/// ```
///
/// Providers can create displays only through shared semantic compositors:
///
/// ```compile_fail
/// carina_docgen::TypeDisplay::<()>::text("provider wording");
/// ```
/// ```compile_fail
/// carina_docgen::TypeDisplay::<()>::code("provider wording");
/// ```
/// ```compile_fail
/// carina_docgen::TypeDisplay::link((), "Enum(X)");
/// ```
/// ```compile_fail
/// carina_docgen::TypeDisplay::<()>::sequence([]);
/// ```
/// ```compile_fail
/// use carina_docgen::{ScalarDoc, TypeDisplay, scalar_display};
/// let display: TypeDisplay<()> = scalar_display(ScalarDoc::Bool, None);
/// display.with_note(None);
/// ```
#[derive(Clone, Debug)]
pub struct TypeDisplay<K> {
    kind: DisplayKind<K>,
    note: Option<String>,
}

#[derive(Clone, Debug)]
enum DisplayKind<K> {
    Text(String),
    Code(String),
    Link { target: K, label: String },
    Sequence(Vec<TypeDisplay<K>>),
}

impl<K> TypeDisplay<K> {
    /// Write literal text, escaping inline Markdown syntax when rendered.
    pub(crate) fn text(literal: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Text(literal.into()),
            note: None,
        }
    }

    /// Style literal text as code, choosing delimiters that preserve backticks.
    ///
    pub(crate) fn code(literal: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Code(literal.into()),
            note: None,
        }
    }

    pub(crate) fn link(target: K, label: impl Into<String>) -> Self {
        Self {
            kind: DisplayKind::Link {
                target,
                label: label.into(),
            },
            note: None,
        }
    }

    /// Compose inline parts. The page writer coalesces consecutive code literals
    /// before choosing delimiters, including across nested sequences.
    pub(crate) fn sequence(parts: impl IntoIterator<Item = Self>) -> Self {
        Self {
            kind: DisplayKind::Sequence(parts.into_iter().collect()),
            note: None,
        }
    }

    /// Attach supplemental validation information as literal text. The shared
    /// renderer owns its suffix and Markdown escaping in every type position.
    pub(crate) fn with_note(mut self, note: Option<crate::ValidationNote<'_>>) -> Self {
        self.note = note.map(|note| note.text().to_owned());
        self
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
        }
        if let Some(note) = self.note.as_ref().filter(|note| !note.is_empty()) {
            cell.push_literal("; ");
            cell.push_literal(note);
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
