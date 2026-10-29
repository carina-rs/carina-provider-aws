//! Registry-compatible Markdown page construction and generated-page checks.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::sync::Mutex;

use comrak::adapters::{HeadingAdapter, HeadingMeta};
use comrak::nodes::{NodeValue, Sourcepos};
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
