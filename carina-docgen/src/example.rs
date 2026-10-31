//! Checked examples embedded in generated provider documentation.

use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result};
use carina_core::parser::{ProviderContext, parse_file};

/// The kind of declaration a documentation example must contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExampleKind {
    DataSource,
    Resource,
}

impl ExampleKind {
    fn description(self) -> &'static str {
        match self {
            Self::DataSource => "data source",
            Self::Resource => "resource",
        }
    }

    fn required_action(self) -> &'static str {
        match self {
            Self::DataSource => "read",
            Self::Resource => "declare",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedDeclaration {
    provider_prefix: String,
    dsl_name: String,
    kind: ExampleKind,
}

impl ExpectedDeclaration {
    fn new(provider_prefix: &str, dsl_name: &str, kind: ExampleKind) -> Self {
        Self {
            provider_prefix: provider_prefix.to_string(),
            dsl_name: dsl_name.to_string(),
            kind,
        }
    }

    fn full_name(&self) -> String {
        format!("{}.{}", self.provider_prefix, self.dsl_name)
    }

    fn description(&self) -> String {
        format!("{} {}", self.kind.description(), self.full_name())
    }

    fn parse_requirement(&self) -> String {
        format!(
            "parse and {} {}",
            self.kind.required_action(),
            self.description()
        )
    }

    fn is_present(&self, parsed: &carina_core::parser::ParsedFile) -> bool {
        match self.kind {
            ExampleKind::DataSource => parsed.data_sources.iter().any(|data_source| {
                data_source.id.provider == self.provider_prefix
                    && data_source.id.resource_type == self.dsl_name
            }),
            ExampleKind::Resource => parsed.resources.iter().any(|resource| {
                resource.id.provider == self.provider_prefix
                    && resource.id.resource_type == self.dsl_name
            }),
        }
    }
}

/// Renderable example text that has been parsed and checked for one declaration.
///
/// The fields are private so callers cannot bypass [`check`]. Before embedding an
/// example, renderers should call [`DocExample::ensure_matches`] for the page
/// they are generating.
#[derive(Debug)]
pub struct DocExample {
    rendered: String,
    checked_for: ExpectedDeclaration,
}

impl DocExample {
    /// Ensure this example was checked for the declaration rendered by a page.
    pub fn ensure_matches(
        &self,
        provider_prefix: &str,
        dsl_name: &str,
        kind: ExampleKind,
    ) -> Result<()> {
        let requested = ExpectedDeclaration::new(provider_prefix, dsl_name, kind);
        if self.checked_for != requested {
            anyhow::bail!(
                "Documentation example checked for {} cannot render {}",
                self.checked_for.description(),
                requested.description()
            );
        }
        Ok(())
    }

    /// Return the exact source text that was parsed and declaration-checked.
    pub fn as_str(&self) -> &str {
        &self.rendered
    }
}

/// Load and check `<examples_dir>/<directory>/main.crn`.
pub fn load(
    examples_dir: &Path,
    directory: &str,
    provider_prefix: &str,
    dsl_name: &str,
    kind: ExampleKind,
) -> Result<DocExample> {
    let path = examples_dir.join(directory).join("main.crn");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("Example file not found: {}", path.display())
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()));
        }
    };

    check(&source, &path, provider_prefix, dsl_name, kind)
}

/// Parse, strip, re-parse, and declaration-check one documentation example.
///
/// Parsing the authored source first preserves its real line numbers in errors.
/// The returned text is then parsed independently and the expected declaration
/// is required from that parsed result, so the checked text and rendered text
/// cannot diverge.
pub fn check(
    source: &str,
    path: &Path,
    provider_prefix: &str,
    dsl_name: &str,
    kind: ExampleKind,
) -> Result<DocExample> {
    let expected = ExpectedDeclaration::new(provider_prefix, dsl_name, kind);
    parse_file(source, path, &ProviderContext::default()).with_context(|| {
        format!(
            "Example {} must {}",
            path.display(),
            expected.parse_requirement()
        )
    })?;

    let rendered = preprocess(source).with_context(|| {
        format!(
            "Failed to prepare example {} for expected {}",
            path.display(),
            expected.description()
        )
    })?;
    if rendered.trim().is_empty() {
        anyhow::bail!(
            "Example {} is empty after stripping; expected {}",
            path.display(),
            expected.description()
        );
    }

    let parsed = parse_file(&rendered, path, &ProviderContext::default()).with_context(|| {
        format!(
            "Rendered example from {} must {}",
            path.display(),
            expected.parse_requirement()
        )
    })?;
    if !expected.is_present(&parsed) {
        anyhow::bail!(
            "Example {} does not {} expected {}",
            path.display(),
            expected.kind.required_action(),
            expected.description()
        );
    }

    Ok(DocExample {
        rendered,
        checked_for: expected,
    })
}

fn preprocess(source: &str) -> Result<String> {
    let heredocs = carina_core::heredoc::preprocess_heredocs(source)
        .context("Failed to preprocess heredocs")?
        .heredocs;
    let mut heredoc_ranges = Vec::with_capacity(heredocs.len());
    let mut search_from = 0;
    for heredoc in heredocs {
        let relative_start = source[search_from..].find(&heredoc).ok_or_else(|| {
            anyhow::anyhow!("carina-core returned a heredoc absent from its source")
        })?;
        let start = search_from + relative_start;
        let end = start + heredoc.len();
        heredoc_ranges.push(start..end);
        search_from = end;
    }

    // Preserve byte positions and newlines while hiding heredoc syntax and
    // bodies from the structural scan below.
    let mut masked = source.as_bytes().to_vec();
    for range in heredoc_ranges {
        for byte in &mut masked[range] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }

    let removals = removals(&masked)?;
    let mut filtered = String::with_capacity(source.len());
    let mut kept_from = 0;
    for removal in removals {
        filtered.push_str(&source[kept_from..removal.start]);
        kept_from = removal.end;
    }
    filtered.push_str(&source[kept_from..]);

    let lines: Vec<&str> = filtered.split_inclusive('\n').collect();
    let first_content = lines
        .iter()
        .position(|line| !line.strip_suffix('\n').unwrap_or(line).is_empty())
        .unwrap_or(lines.len());
    Ok(lines[first_content..].concat())
}

fn removals(masked: &[u8]) -> Result<Vec<Range<usize>>> {
    let mut removals = Vec::new();
    let mut cursor = 0;
    let mut line_start = 0;
    let mut brace_depth = 0usize;

    while cursor < masked.len() {
        match masked[cursor] {
            b'\n' => {
                cursor += 1;
                line_start = cursor;
            }
            b' ' | b'\t' | b'\r' => cursor += 1,
            b'#' if brace_depth == 0 && line_prefix_is_whitespace(masked, line_start, cursor) => {
                let end = line_removal_end(masked, cursor);
                removals.push(line_start..end);
                cursor = end;
                line_start = cursor;
            }
            b'/' if masked.get(cursor + 1) == Some(&b'/')
                && brace_depth == 0
                && line_prefix_is_whitespace(masked, line_start, cursor) =>
            {
                let end = line_removal_end(masked, cursor);
                removals.push(line_start..end);
                cursor = end;
                line_start = cursor;
            }
            b'#' => cursor = skip_line_comment(masked, cursor),
            b'/' if masked.get(cursor + 1) == Some(&b'/') => {
                cursor = skip_line_comment(masked, cursor);
            }
            b'/' if masked.get(cursor + 1) == Some(&b'*') => {
                let end = skip_block_comment(masked, cursor);
                update_line_start(masked, cursor, end, &mut line_start);
                cursor = end;
            }
            b'\'' => {
                let end = skip_single_quoted_string(masked, cursor);
                update_line_start(masked, cursor, end, &mut line_start);
                cursor = end;
            }
            b'"' => {
                let end = skip_double_quoted_string(masked, cursor);
                update_line_start(masked, cursor, end, &mut line_start);
                cursor = end;
            }
            b'{' => {
                brace_depth += 1;
                cursor += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                cursor += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let token_start = cursor;
                cursor += 1;
                while masked
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    cursor += 1;
                }
                let is_provider = &masked[token_start..cursor] == b"provider"
                    && brace_depth == 0
                    && line_prefix_is_whitespace(masked, line_start, token_start)
                    && masked
                        .get(cursor)
                        .is_some_and(|byte| byte.is_ascii_whitespace());
                if !is_provider {
                    continue;
                }

                let block_end = find_provider_block_end(masked, cursor).ok_or_else(|| {
                    anyhow::anyhow!("parsed provider block has no balanced closing brace")
                })?;
                let removal_end = provider_removal_end(masked, block_end);
                removals.push(line_start..removal_end);
                update_line_start(masked, cursor, removal_end, &mut line_start);
                cursor = removal_end;
            }
            _ => cursor += 1,
        }
    }

    Ok(removals)
}

fn line_prefix_is_whitespace(source: &[u8], start: usize, end: usize) -> bool {
    source[start..end]
        .iter()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

fn line_removal_end(source: &[u8], start: usize) -> usize {
    source[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |relative| start + relative + 1)
}

fn provider_removal_end(source: &[u8], block_end: usize) -> usize {
    let line_end = source[block_end..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |relative| block_end + relative);
    if line_prefix_is_whitespace(source, block_end, line_end) {
        (line_end + usize::from(line_end < source.len())).min(source.len())
    } else {
        block_end
    }
}

fn find_provider_block_end(source: &[u8], mut cursor: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut found_opening = false;
    while cursor < source.len() {
        match source[cursor] {
            b'#' => cursor = skip_line_comment(source, cursor),
            b'/' if source.get(cursor + 1) == Some(&b'/') => {
                cursor = skip_line_comment(source, cursor);
            }
            b'/' if source.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_block_comment(source, cursor);
            }
            b'\'' => cursor = skip_single_quoted_string(source, cursor),
            b'"' => cursor = skip_double_quoted_string(source, cursor),
            b'{' => {
                found_opening = true;
                depth += 1;
                cursor += 1;
            }
            b'}' if found_opening => {
                depth -= 1;
                cursor += 1;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => cursor += 1,
        }
    }
    None
}

fn skip_line_comment(source: &[u8], start: usize) -> usize {
    source[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |relative| start + relative)
}

fn skip_block_comment(source: &[u8], start: usize) -> usize {
    let mut cursor = start + 2;
    let mut depth = 1usize;
    while cursor < source.len() {
        if source.get(cursor..cursor + 2) == Some(b"/*") {
            depth += 1;
            cursor += 2;
        } else if source.get(cursor..cursor + 2) == Some(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                return cursor;
            }
        } else {
            cursor += 1;
        }
    }
    source.len()
}

fn skip_single_quoted_string(source: &[u8], start: usize) -> usize {
    let mut cursor = start + 1;
    while cursor < source.len() {
        match source[cursor] {
            b'\\' => cursor = (cursor + 2).min(source.len()),
            b'\'' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    source.len()
}

fn skip_double_quoted_string(source: &[u8], start: usize) -> usize {
    let mut cursor = start + 1;
    while cursor < source.len() {
        match source[cursor] {
            b'\\' => cursor = (cursor + 2).min(source.len()),
            b'"' => return cursor + 1,
            b'$' if source.get(cursor + 1) == Some(&b'{') => {
                cursor = skip_interpolation(source, cursor + 2);
            }
            _ => cursor += 1,
        }
    }
    source.len()
}

fn skip_interpolation(source: &[u8], mut cursor: usize) -> usize {
    let mut depth = 1usize;
    while cursor < source.len() {
        match source[cursor] {
            b'#' => cursor = skip_line_comment(source, cursor),
            b'/' if source.get(cursor + 1) == Some(&b'/') => {
                cursor = skip_line_comment(source, cursor);
            }
            b'/' if source.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_block_comment(source, cursor);
            }
            b'\'' => cursor = skip_single_quoted_string(source, cursor),
            b'"' => cursor = skip_double_quoted_string(source, cursor),
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth -= 1;
                cursor += 1;
                if depth == 0 {
                    return cursor;
                }
            }
            _ => cursor += 1,
        }
    }
    source.len()
}

fn update_line_start(source: &[u8], start: usize, end: usize, line_start: &mut usize) {
    if let Some(relative) = source[start..end].iter().rposition(|byte| *byte == b'\n') {
        *line_start = start + relative + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_fixture(examples_dir: &Path, directory: &str, source: &str) -> PathBuf {
        let directory = examples_dir.join(directory);
        std::fs::create_dir(&directory).expect("example directory");
        let path = directory.join("main.crn");
        std::fs::write(&path, source).expect("example source");
        path
    }

    fn load_fixture(source: &str, kind: ExampleKind) -> Result<DocExample> {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        write_fixture(examples_dir.path(), "ec2_vpc", source);
        load(examples_dir.path(), "ec2_vpc", "aws", "ec2.Vpc", kind)
    }

    #[test]
    fn missing_file_reports_path() {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        let expected = examples_dir.path().join("ec2_vpc/main.crn");

        let error = load(
            examples_dir.path(),
            "ec2_vpc",
            "aws",
            "ec2.Vpc",
            ExampleKind::Resource,
        )
        .expect_err("missing example must fail");

        assert_eq!(
            error.to_string(),
            format!("Example file not found: {}", expected.display())
        );
    }

    #[test]
    fn empty_after_stripping_is_rejected() {
        let error = load_fixture(
            concat!(
                "# VPC example\n",
                "provider aws {\n",
                "  region = aws.Region.ap_northeast_1\n",
                "}\n",
            ),
            ExampleKind::Resource,
        )
        .expect_err("stripped example must not be empty");

        assert!(
            error.to_string().contains("is empty after stripping"),
            "{error:#}"
        );
        assert!(
            error.to_string().contains("resource aws.ec2.Vpc"),
            "{error:#}"
        );
    }

    #[test]
    fn wrong_provider_is_rejected() {
        let error = load_fixture("awscc.ec2.Vpc {}\n", ExampleKind::Resource)
            .expect_err("provider must match");

        assert!(
            error
                .to_string()
                .contains("does not declare expected resource aws.ec2.Vpc"),
            "{error:#}"
        );
    }

    #[test]
    fn wrong_type_is_rejected() {
        let error = load_fixture("aws.ec2.Subnet {}\n", ExampleKind::Resource)
            .expect_err("resource type must match");

        assert!(
            error
                .to_string()
                .contains("does not declare expected resource aws.ec2.Vpc"),
            "{error:#}"
        );
    }

    #[test]
    fn resource_and_data_source_kinds_must_match() {
        let resource_error =
            load_fixture("let fixture = read aws.ec2.Vpc {}\n", ExampleKind::Resource)
                .expect_err("read must not satisfy a resource example");
        assert!(
            resource_error
                .to_string()
                .contains("does not declare expected resource aws.ec2.Vpc"),
            "{resource_error:#}"
        );

        let data_source_error = load_fixture("aws.ec2.Vpc {}\n", ExampleKind::DataSource)
            .expect_err("resource must not satisfy a data-source example");
        assert!(
            data_source_error
                .to_string()
                .contains("does not read expected data source aws.ec2.Vpc"),
            "{data_source_error:#}"
        );
    }

    #[test]
    fn one_line_provider_block_keeps_resource() {
        let example = load_fixture(
            concat!(
                "provider aws { region = aws.Region.ap_northeast_1 }\n",
                "aws.ec2.Vpc {}\n",
            ),
            ExampleKind::Resource,
        )
        .expect("valid example");

        assert_eq!(example.as_str(), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn indented_provider_closing_brace_keeps_resource() {
        let example = load_fixture(
            concat!(
                "provider aws {\n",
                "  region = aws.Region.ap_northeast_1\n",
                "  }\n",
                "aws.ec2.Vpc {}\n",
            ),
            ExampleKind::Resource,
        )
        .expect("valid example");

        assert_eq!(example.as_str(), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn hash_line_in_heredoc_body_is_preserved() {
        let example = load_fixture(
            concat!(
                "  # page-only heading\n",
                "aws.ec2.Vpc {\n",
                "  description = <<EOT\n",
                "# not a comment\n",
                "EOT\n",
                "}\n",
            ),
            ExampleKind::Resource,
        )
        .expect("valid heredoc example");

        assert_eq!(
            example.as_str(),
            concat!(
                "aws.ec2.Vpc {\n",
                "  description = <<EOT\n",
                "# not a comment\n",
                "EOT\n",
                "}\n",
            )
        );
    }

    #[test]
    fn braces_in_quoted_strings_do_not_change_comment_depth() {
        let source = concat!(
            "aws.ec2.Vpc {\n",
            "  description = \"}\"\n",
            "  # reader-facing note\n",
            "}\n",
        );
        let example = load_fixture(source, ExampleKind::Resource).expect("valid quoted string");

        assert_eq!(example.as_str(), source);
    }

    #[test]
    fn parse_error_uses_raw_line_and_keeps_error_chain() {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        let path = write_fixture(
            examples_dir.path(),
            "ec2_vpc",
            concat!(
                "# Header line 1\n",
                "# Header line 2\n",
                "# Header line 3\n",
                "# Header line 4\n",
                "provider aws {\n",
                "  region = aws.Region.ap_northeast_1\n",
                "}\n",
                "@\n",
            ),
        );

        let error = load(
            examples_dir.path(),
            "ec2_vpc",
            "aws",
            "ec2.Vpc",
            ExampleKind::Resource,
        )
        .expect_err("invalid raw source must fail");

        assert!(
            error.to_string().contains(&path.display().to_string()),
            "{error:#}"
        );
        assert!(
            error.to_string().contains("resource aws.ec2.Vpc"),
            "{error:#}"
        );
        assert!(error.chain().count() > 1, "{error:#}");
        assert!(
            error
                .chain()
                .skip(1)
                .any(|cause| cause.to_string().contains("8:1")),
            "{error:#}"
        );
    }
}
