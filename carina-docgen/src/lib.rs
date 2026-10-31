//! Registry-compatible Markdown page construction and generated-page checks.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::sync::Mutex;

use comrak::adapters::{HeadingAdapter, HeadingMeta};
use comrak::nodes::{AstNode, NodeValue, Sourcepos};
use comrak::options::Plugins;
use comrak::{Arena, Options, format_html_with_plugins, parse_document};
use finl_unicode::categories::CharacterCategories;

/// A generated Markdown heading whose flattened text is known without parsing
/// it independently from the page that contains it.
#[derive(Clone, Debug)]
pub struct Heading {
    level: u8,
    content: String,
    style: HeadingStyle,
}

#[derive(Clone, Copy, Debug)]
enum HeadingStyle {
    Plain,
    Code,
}

impl Heading {
    pub fn plain(level: u8, content: impl Into<String>) -> Self {
        Self {
            level,
            content: content.into(),
            style: HeadingStyle::Plain,
        }
    }

    pub fn code(level: u8, content: impl Into<String>) -> Self {
        Self {
            level,
            content: content.into(),
            style: HeadingStyle::Code,
        }
    }

    fn markdown(&self) -> Result<String, DocgenError> {
        if !(1..=6).contains(&self.level) {
            return Err(DocgenError::build(format!(
                "Markdown heading level must be between 1 and 6, got {}",
                self.level
            )));
        }
        if self.content.contains(['\n', '\r']) {
            return Err(DocgenError::build(
                "Markdown heading content must fit on one line",
            ));
        }
        if matches!(self.style, HeadingStyle::Code) && self.content.contains('`') {
            return Err(DocgenError::build(
                "Markdown code heading content must not contain a backtick",
            ));
        }

        let marker = "#".repeat(usize::from(self.level));
        let content = match self.style {
            HeadingStyle::Plain => self.content.clone(),
            HeadingStyle::Code => format!("`{}`", self.content),
        };
        Ok(format!("{marker} {content}"))
    }
}

#[derive(Debug)]
enum PageChunk<K> {
    Markdown(String),
    Link { target: K, label: String },
}

#[derive(Debug)]
enum TableCellPart<K> {
    Text(String),
    Code(String),
    Link { target: K, label: String },
}

/// One table cell made only from table-safe inline parts.
///
/// Text parts remain inline Markdown: this is not a general Markdown escaper.
/// Only table delimiters (`|`) are escaped and line breaks are collapsed when
/// the table is written. There is intentionally no raw part that can bypass
/// that table-specific escaping.
#[derive(Debug)]
pub struct TableCell<K> {
    parts: Vec<TableCellPart<K>>,
}

impl<K> Default for TableCell<K> {
    fn default() -> Self {
        Self { parts: Vec::new() }
    }
}

impl<K> TableCell<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(text: impl Into<String>) -> Self {
        let mut cell = Self::new();
        cell.push_text(text);
        cell
    }

    pub fn code(code: impl Into<String>) -> Self {
        let mut cell = Self::new();
        cell.push_code(code);
        cell
    }

    pub fn link(target: K, label: impl Into<String>) -> Self {
        let mut cell = Self::new();
        cell.push_link(target, label);
        cell
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        self.parts.push(TableCellPart::Text(text.into()));
    }

    pub fn push_code(&mut self, code: impl Into<String>) {
        self.parts.push(TableCellPart::Code(code.into()));
    }

    pub fn push_link(&mut self, target: K, label: impl Into<String>) {
        self.parts.push(TableCellPart::Link {
            target,
            label: label.into(),
        });
    }
}

/// Builds one Markdown page while owning heading order and typed link targets.
///
/// A target link may be written before its heading. The link is held as a
/// typed placeholder until [`PageBuilder::finish`], which resolves it to the
/// slug allocated when the target heading was actually written.
#[derive(Debug)]
pub struct PageBuilder<K> {
    chunks: Vec<PageChunk<K>>,
    slugs: HeadingSlugs,
    heading_ids: Vec<String>,
    targets: HashMap<K, String>,
}

impl<K> Default for PageBuilder<K> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            slugs: HeadingSlugs::default(),
            heading_ids: Vec::new(),
            targets: HashMap::new(),
        }
    }
}

impl<K> PageBuilder<K>
where
    K: Eq + Hash + fmt::Debug,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_str(&mut self, markdown: &str) {
        match self.chunks.last_mut() {
            Some(PageChunk::Markdown(existing)) => existing.push_str(markdown),
            _ => self.chunks.push(PageChunk::Markdown(markdown.to_string())),
        }
    }

    /// Write an ordinary heading and allocate its registry slug immediately.
    pub fn heading(&mut self, heading: Heading) -> Result<(), DocgenError> {
        self.write_heading(heading).map(|_| ())
    }

    /// Write a keyed target heading and bind the key to its allocated slug.
    pub fn target_heading(&mut self, key: K, heading: Heading) -> Result<(), DocgenError> {
        if self.targets.contains_key(&key) {
            return Err(DocgenError::build(format!(
                "heading target {key:?} was written more than once"
            )));
        }
        let slug = self.write_heading(heading)?;
        self.targets.insert(key, slug);
        Ok(())
    }

    /// Write a link placeholder whose fragment can only come from a keyed
    /// heading written through this builder.
    pub fn link(&mut self, target: K, label: impl Into<String>) {
        self.chunks.push(PageChunk::Link {
            target,
            label: label.into(),
        });
    }

    /// Write a GFM table whose rows must all match the fixed header width.
    pub fn table<H, R, C>(&mut self, header: H, rows: R) -> Result<(), DocgenError>
    where
        H: IntoIterator,
        H::Item: AsRef<str>,
        R: IntoIterator<Item = C>,
        C: IntoIterator<Item = TableCell<K>>,
    {
        let header: Vec<String> = header
            .into_iter()
            .map(|label| escape_table_cell(label.as_ref()))
            .collect();
        if header.is_empty() {
            return Err(DocgenError::build(
                "Markdown tables must have at least one column",
            ));
        }

        let rows: Vec<Vec<TableCell<K>>> = rows
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect();
        for (index, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(DocgenError::build(format!(
                    "table row {} expected {} cells, got {}",
                    index + 1,
                    header.len(),
                    row.len()
                )));
            }
        }

        self.push_str("|");
        for label in &header {
            self.push_str(" ");
            self.push_str(label);
            self.push_str(" |");
        }
        self.push_str("\n|");
        for label in &header {
            self.push_str(&"-".repeat((label.chars().count() + 2).max(3)));
            self.push_str("|");
        }
        self.push_str("\n");

        for row in rows {
            self.push_str("|");
            for cell in row {
                self.push_str(" ");
                self.write_table_cell(cell);
                self.push_str(" |");
            }
            self.push_str("\n");
        }
        Ok(())
    }

    /// Resolve links, verify the builder's headings against Comrak's flattened
    /// heading text, and run the generated-page checker over the finished page.
    pub fn finish(self) -> Result<String, DocgenError> {
        let mut markdown = String::new();
        for chunk in self.chunks {
            match chunk {
                PageChunk::Markdown(text) => markdown.push_str(&text),
                PageChunk::Link { target, label } => {
                    let slug = self.targets.get(&target).ok_or_else(|| {
                        DocgenError::build(format!(
                            "link target {target:?} has no heading: it was never written"
                        ))
                    })?;
                    markdown.push_str(&format!("[{label}](#{slug})"));
                }
            }
        }

        let analysis = analyze_page(&markdown);
        let actual_ids: Vec<&str> = analysis
            .headings
            .iter()
            .map(|heading| heading.id.as_str())
            .collect();
        let expected_ids: Vec<&str> = self.heading_ids.iter().map(String::as_str).collect();
        if actual_ids != expected_ids {
            return Err(DocgenError::build(format!(
                "generated heading order differed from the page builder: expected {expected_ids:?}, got {actual_ids:?}"
            )));
        }
        finish_check(analysis)?;
        Ok(markdown)
    }

    fn write_heading(&mut self, heading: Heading) -> Result<String, DocgenError> {
        let markdown = heading.markdown()?;
        let slug = self.slugs.unique(heading_slug(&heading.content));
        self.heading_ids.push(slug.clone());
        self.push_str(&markdown);
        self.push_str("\n\n");
        Ok(slug)
    }

    fn write_table_cell(&mut self, cell: TableCell<K>) {
        for part in cell.parts {
            match part {
                TableCellPart::Text(text) => self.push_str(&escape_table_cell(&text)),
                TableCellPart::Code(code) => self.push_str(&inline_code(&code)),
                TableCellPart::Link { target, label } => {
                    self.link(target, escape_table_link_label(&label));
                }
            }
        }
    }
}

fn escape_table_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    let mut preceding_backslashes = 0_usize;
    let mut characters = value.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                escaped.push(' ');
                preceding_backslashes = 0;
            }
            '\n' => {
                escaped.push(' ');
                preceding_backslashes = 0;
            }
            '\\' => {
                escaped.push(character);
                preceding_backslashes += 1;
            }
            '|' => {
                if preceding_backslashes.is_multiple_of(2) {
                    escaped.push('\\');
                }
                escaped.push(character);
                preceding_backslashes = 0;
            }
            _ => {
                escaped.push(character);
                preceding_backslashes = 0;
            }
        }
    }

    escaped
}

fn escape_table_link_label(value: &str) -> String {
    let value = escape_table_cell(value);
    let mut escaped = String::with_capacity(value.len());
    let mut preceding_backslashes = 0_usize;

    for character in value.chars() {
        match character {
            '\\' => {
                escaped.push(character);
                preceding_backslashes += 1;
            }
            '[' | ']' => {
                if preceding_backslashes.is_multiple_of(2) {
                    escaped.push('\\');
                }
                escaped.push(character);
                preceding_backslashes = 0;
            }
            _ => {
                escaped.push(character);
                preceding_backslashes = 0;
            }
        }
    }

    escaped
}

fn inline_code(value: &str) -> String {
    let value = escape_table_cell(value);
    if value.is_empty() {
        return value;
    }

    let mut longest_run = 0_usize;
    let mut current_run = 0_usize;
    for character in value.chars() {
        if character == '`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }

    let delimiter = "`".repeat(longest_run + 1);
    let needs_padding = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ')
            && value.ends_with(' ')
            && !value.chars().all(char::is_whitespace));
    if needs_padding {
        format!("{delimiter} {value} {delimiter}")
    } else {
        format!("{delimiter}{value}{delimiter}")
    }
}

/// An error encountered while constructing or validating a generated page.
#[derive(Debug)]
pub enum DocgenError {
    Build(String),
    Check(PageCheckError),
}

impl DocgenError {
    fn build(message: impl Into<String>) -> Self {
        Self::Build(message.into())
    }
}

impl fmt::Display for DocgenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(message) => formatter.write_str(message),
            Self::Check(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DocgenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Build(_) => None,
            Self::Check(error) => Some(error),
        }
    }
}

impl From<PageCheckError> for DocgenError {
    fn from(error: PageCheckError) -> Self {
        Self::Check(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageViolationKind {
    UnresolvedFragment,
    LinkInsideCodeSpan,
    DuplicateSectionHeading,
    TableColumnMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageViolation {
    pub kind: PageViolationKind,
    pub line: usize,
    pub detail: String,
}

#[derive(Debug)]
pub struct PageCheckError {
    violations: Vec<PageViolation>,
}

impl PageCheckError {
    pub fn violations(&self) -> &[PageViolation] {
        &self.violations
    }
}

impl fmt::Display for PageCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} generated documentation violation(s):",
            self.violations.len()
        )?;
        for violation in &self.violations {
            writeln!(formatter, "  line {}: {}", violation.line, violation.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for PageCheckError {}

/// Validate the invariants required of one generated Markdown page.
pub fn check_page(markdown: &str) -> Result<(), PageCheckError> {
    finish_check(analyze_page(markdown))
}

/// Return the registry-compatible heading ids from one complete Markdown page.
///
/// This is primarily exposed for the shared registry drift fixture and for
/// asserting that packaging transformations preserve the validated headings.
pub fn heading_ids(markdown: &str) -> Vec<String> {
    analyze_page(markdown)
        .headings
        .into_iter()
        .map(|heading| heading.id)
        .collect()
}

fn finish_check(mut analysis: PageAnalysis) -> Result<(), PageCheckError> {
    if analysis.violations.is_empty() {
        Ok(())
    } else {
        analysis.violations.sort_by_key(|violation| violation.line);
        Err(PageCheckError {
            violations: analysis.violations,
        })
    }
}

#[derive(Debug)]
struct PageAnalysis {
    headings: Vec<CollectedHeading>,
    violations: Vec<PageViolation>,
}

#[derive(Clone, Debug)]
struct CollectedHeading {
    level: u8,
    content: String,
    base_slug: String,
    id: String,
    line: usize,
}

/// Parse a page once, then reuse that AST both for heading flattening and for
/// link/code-span validation.
fn analyze_page(markdown: &str) -> PageAnalysis {
    let options = registry_options();
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options);
    let mut heading_lines = Vec::new();
    let mut raw_links = Vec::new();
    let mut code_links = Vec::new();
    let mut tables = Vec::new();

    for node in root.descendants() {
        let data = node.data.borrow();
        match &data.value {
            NodeValue::Heading(_) => heading_lines.push(data.sourcepos.start.line),
            NodeValue::Link(link) => {
                raw_links.push((data.sourcepos.start.line, link.url.clone()));
            }
            NodeValue::Code(code) if contains_markdown_link(&code.literal) => {
                code_links.push((data.sourcepos.start.line, code.literal.clone()));
            }
            NodeValue::Table(_) => tables.push(node),
            _ => {}
        }
    }

    let collector = FlattenedHeadingCollector::default();
    let mut plugins = Plugins::default();
    plugins.render.heading_adapter = Some(&collector);
    let mut rendered = String::new();
    format_html_with_plugins(root, &options, &mut rendered, &plugins)
        .expect("formatting Markdown headings into a String cannot fail");

    let flattened = collector
        .headings
        .into_inner()
        .expect("heading collector lock poisoned");
    assert_eq!(
        flattened.len(),
        heading_lines.len(),
        "Comrak heading adapter and AST traversal must see the same headings"
    );
    let mut slugs = HeadingSlugs::default();
    let headings: Vec<CollectedHeading> = flattened
        .into_iter()
        .zip(heading_lines)
        .map(|(heading, line)| {
            let base_slug = heading_slug(&heading.content);
            let id = slugs.unique(base_slug.clone());
            CollectedHeading {
                level: heading.level,
                content: heading.content,
                base_slug,
                id,
                line,
            }
        })
        .collect();

    let heading_ids: HashSet<&str> = headings.iter().map(|heading| heading.id.as_str()).collect();
    let mut violations = duplicate_section_headings(&headings);
    let source_lines: Vec<&str> = markdown.lines().collect();
    for table in tables {
        violations.extend(table_column_mismatches(table, &source_lines));
    }
    for (line, url) in raw_links {
        if let Some(fragment) = url.strip_prefix('#')
            && !heading_ids.contains(fragment)
        {
            violations.push(PageViolation {
                kind: PageViolationKind::UnresolvedFragment,
                line,
                detail: format!("fragment-only link does not resolve: #{fragment}"),
            });
        }
    }
    for (line, literal) in code_links {
        violations.push(PageViolation {
            kind: PageViolationKind::LinkInsideCodeSpan,
            line,
            detail: format!("Markdown link is inside a code span: `{literal}`"),
        });
    }

    PageAnalysis {
        headings,
        violations,
    }
}

fn table_column_mismatches<'a>(
    table: &'a AstNode<'a>,
    source_lines: &[&str],
) -> Vec<PageViolation> {
    let rows: Vec<(bool, Sourcepos)> = table
        .children()
        .filter_map(|row| {
            let data = row.data.borrow();
            match &data.value {
                NodeValue::TableRow(is_header) => Some((*is_header, data.sourcepos)),
                _ => None,
            }
        })
        .collect();
    let Some((_, header_sourcepos)) = rows.iter().find(|(is_header, _)| *is_header) else {
        return Vec::new();
    };
    let Some(header_source) = table_row_source(source_lines, *header_sourcepos) else {
        return Vec::new();
    };
    let expected = table_cell_count(header_source);

    rows.into_iter()
        .filter(|(is_header, _)| !is_header)
        .filter_map(|(_, sourcepos)| {
            let source = table_row_source(source_lines, sourcepos)?;
            let actual = table_cell_count(source);
            (actual != expected).then(|| PageViolation {
                kind: PageViolationKind::TableColumnMismatch,
                line: sourcepos.start.line,
                detail: format!("table row has {actual} columns, but its header has {expected}"),
            })
        })
        .collect()
}

fn table_row_source<'a>(source_lines: &'a [&'a str], sourcepos: Sourcepos) -> Option<&'a str> {
    let line = *source_lines.get(sourcepos.start.line.checked_sub(1)?)?;
    let start = sourcepos.start.column.saturating_sub(1);
    line.get(start..).or(Some(line))
}

fn table_cell_count(source: &str) -> usize {
    let source = source.trim();
    let bytes = source.as_bytes();
    let delimiters = bytes
        .iter()
        .enumerate()
        .filter(|(index, _)| is_unescaped_pipe(bytes, *index))
        .count();
    let leading = usize::from(is_unescaped_pipe(bytes, 0));
    let trailing = usize::from(
        bytes
            .len()
            .checked_sub(1)
            .is_some_and(|index| is_unescaped_pipe(bytes, index)),
    );

    (delimiters + 1)
        .saturating_sub(leading)
        .saturating_sub(trailing)
}

fn is_unescaped_pipe(bytes: &[u8], index: usize) -> bool {
    bytes.get(index) == Some(&b'|') && !is_escaped(bytes, index)
}

fn registry_options() -> Options<'static> {
    // Keep this identical to registry/src/docs_bundle.rs::render_markdown.
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.render.r#unsafe = false;
    options
}

#[derive(Default)]
struct FlattenedHeadingCollector {
    headings: Mutex<Vec<HeadingMeta>>,
}

impl HeadingAdapter for FlattenedHeadingCollector {
    fn enter(
        &self,
        output: &mut dyn fmt::Write,
        heading: &HeadingMeta,
        _sourcepos: Option<Sourcepos>,
    ) -> fmt::Result {
        self.headings
            .lock()
            .expect("heading collector lock poisoned")
            .push(heading.clone());
        write!(output, "<h{}>", heading.level)
    }

    fn exit(&self, output: &mut dyn fmt::Write, heading: &HeadingMeta) -> fmt::Result {
        writeln!(output, "</h{}>", heading.level)
    }
}

fn contains_markdown_link(markdown: &str) -> bool {
    let bytes = markdown.as_bytes();
    let mut open = 0;
    while open < bytes.len() {
        if bytes[open] != b'[' || is_escaped(bytes, open) {
            open += 1;
            continue;
        }

        let mut bracket_depth = 1_usize;
        let mut cursor = open + 1;
        while cursor < bytes.len() && bracket_depth != 0 {
            if !is_escaped(bytes, cursor) {
                match bytes[cursor] {
                    b'[' => bracket_depth += 1,
                    b']' => bracket_depth -= 1,
                    _ => {}
                }
            }
            cursor += 1;
        }
        if bracket_depth != 0 || cursor >= bytes.len() || bytes[cursor] != b'(' {
            open += 1;
            continue;
        }

        let mut paren_depth = 1_usize;
        cursor += 1;
        while cursor < bytes.len() {
            if !is_escaped(bytes, cursor) {
                match bytes[cursor] {
                    b'(' => paren_depth += 1,
                    b')' => {
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            cursor += 1;
        }
        open += 1;
    }
    false
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let preceding_backslashes = bytes[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    preceding_backslashes % 2 == 1
}

fn duplicate_section_headings(headings: &[CollectedHeading]) -> Vec<PageViolation> {
    let mut violations = Vec::new();
    let mut current_section: Option<&str> = None;
    let mut emitted_sections = HashSet::new();
    let mut emitted_children: HashSet<(&str, &str)> = HashSet::new();

    for heading in headings {
        if heading.level <= 2 {
            current_section = None;
        }
        if heading.level == 2
            && matches!(
                heading.content.as_str(),
                "Enum Values" | "Struct Definitions"
            )
        {
            let section = heading.content.as_str();
            if !emitted_sections.insert(section) {
                violations.push(PageViolation {
                    kind: PageViolationKind::DuplicateSectionHeading,
                    line: heading.line,
                    detail: format!("section heading is emitted more than once: {section}"),
                });
            }
            current_section = Some(section);
        } else if heading.level == 3
            && let Some(section) = current_section
            && !emitted_children.insert((section, heading.base_slug.as_str()))
        {
            violations.push(PageViolation {
                kind: PageViolationKind::DuplicateSectionHeading,
                line: heading.line,
                detail: format!(
                    "heading is emitted more than once under {section}: {}",
                    heading.content
                ),
            });
        }
    }

    violations
}

#[derive(Debug, Default)]
struct HeadingSlugs {
    used: HashSet<String>,
    next_suffix: HashMap<String, usize>,
}

impl HeadingSlugs {
    fn unique(&mut self, base: String) -> String {
        let mut suffix = self.next_suffix.get(&base).copied().unwrap_or(0);
        loop {
            let candidate = if suffix == 0 {
                base.clone()
            } else {
                format!("{base}-{suffix}")
            };
            let following_suffix = suffix + 1;
            if self.used.insert(candidate.clone()) {
                self.next_suffix.insert(base, following_suffix);
                return candidate;
            }
            suffix = following_suffix;
        }
    }
}

/// Apply the registry's base heading-slug rule without page-level deduplication.
pub fn heading_slug(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|character| {
            if character.is_letter()
                || character.is_number_decimal()
                || matches!(character, ' ' | '-' | '_')
            {
                Some(if character == ' ' { '-' } else { character })
            } else {
                None
            }
        })
        .collect()
}
