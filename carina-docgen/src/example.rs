//! Checked examples embedded in generated provider documentation.

use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result};
use carina_core::parser::{
    DeferredForBody, DeferredForTarget, ParsedDeferredForExpression, ParsedFile, ProviderConfig,
    ProviderContext, parse_file,
};
use carina_core::resource::{
    DataSource, ModuleSource, ParserPlaceholder, Resource, ResourceId, ResourceIdentityState,
};

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
/// The fields are private so callers cannot bypass [`check`] or obtain the text
/// without checking its declaration identity.
#[derive(Debug)]
pub struct DocExample {
    rendered: String,
    checked_for: ExpectedDeclaration,
}

impl DocExample {
    /// Return the checked text only when it belongs to the requested page.
    pub fn text_for(
        &self,
        provider_prefix: &str,
        dsl_name: &str,
        kind: ExampleKind,
    ) -> Result<&str> {
        let requested = ExpectedDeclaration::new(provider_prefix, dsl_name, kind);
        if self.checked_for != requested {
            anyhow::bail!(
                "Documentation example checked for {} cannot render {}",
                self.checked_for.description(),
                requested.description()
            );
        }
        Ok(&self.rendered)
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
    let authored = parse_file(source, path, &ProviderContext::default()).with_context(|| {
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

    check_rendered(&authored, rendered, path, expected)
}

fn check_rendered(
    authored: &ParsedFile,
    rendered: String,
    path: &Path,
    expected: ExpectedDeclaration,
) -> Result<DocExample> {
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
    ensure_semantically_equal(authored, &parsed, path)?;

    Ok(DocExample {
        rendered,
        checked_for: expected,
    })
}

fn ensure_semantically_equal(
    authored: &ParsedFile,
    rendered: &ParsedFile,
    path: &Path,
) -> Result<()> {
    let authored_deferred_for_expressions = authored.parsed_deferred_for_expressions();
    let rendered_deferred_for_expressions = rendered.parsed_deferred_for_expressions();
    let ParsedFile {
        // Unnamed provider blocks are the only intentional parsed difference: rendering removes them.
        providers: authored_providers,
        resources: authored_resources,
        data_sources: authored_data_sources,
        compositions: authored_compositions,
        variables: authored_variables,
        uses: authored_uses,
        module_calls: authored_module_calls,
        arguments: authored_arguments,
        attribute_params: authored_attribute_params,
        export_params: authored_export_params,
        backend: authored_backend,
        state_blocks: authored_state_blocks,
        user_functions: authored_user_functions,
        upstream_states: authored_upstream_states,
        wait_bindings: authored_wait_bindings,
        requires: authored_requires,
        structural_bindings: authored_structural_bindings,
        // Evaluator-only parser bookkeeping is not authored DSL semantics.
        parser_binding_metadata: _,
        warnings: authored_warnings,
        deferred_module_ifs: authored_deferred_module_ifs,
        // Module-expansion lineage is not produced when parsing these standalone files.
        expansion_trace: _,
        // `..` stands only for crate-private `parsed_binding_uses`, `deferred_for_phase`, and
        // `template_placeholder`; new public fields must be added by name.
        ..
    } = authored;
    let ParsedFile {
        providers: rendered_providers,
        resources: rendered_resources,
        data_sources: rendered_data_sources,
        compositions: rendered_compositions,
        variables: rendered_variables,
        uses: rendered_uses,
        module_calls: rendered_module_calls,
        arguments: rendered_arguments,
        attribute_params: rendered_attribute_params,
        export_params: rendered_export_params,
        backend: rendered_backend,
        state_blocks: rendered_state_blocks,
        user_functions: rendered_user_functions,
        upstream_states: rendered_upstream_states,
        wait_bindings: rendered_wait_bindings,
        requires: rendered_requires,
        structural_bindings: rendered_structural_bindings,
        // Evaluator-only parser bookkeeping is not authored DSL semantics.
        parser_binding_metadata: _,
        warnings: rendered_warnings,
        deferred_module_ifs: rendered_deferred_module_ifs,
        // Module-expansion lineage is not produced when parsing these standalone files.
        expansion_trace: _,
        // `..` stands only for crate-private `parsed_binding_uses`, `deferred_for_phase`, and
        // `template_placeholder`; new public fields must be added by name.
        ..
    } = rendered;

    if authored_deferred_module_ifs.len() != rendered_deferred_module_ifs.len() {
        anyhow::bail!(
            "Rendered example from {} changed parsed deferred structural `if` expressions",
            path.display()
        );
    }
    if !authored_deferred_module_ifs.is_empty() || !rendered_deferred_module_ifs.is_empty() {
        anyhow::bail!(
            "Example {} contains a deferred structural `if`; such examples cannot be checked for \
             render equivalence",
            path.display()
        );
    }

    let mismatch = if !same_named_providers(authored_providers, rendered_providers) {
        Some("named provider blocks")
    } else if !same_resources(authored_resources, rendered_resources) {
        Some("resources")
    } else if !same_data_sources(authored_data_sources, rendered_data_sources) {
        Some("data sources")
    } else if authored_compositions != rendered_compositions {
        Some("compositions")
    } else if authored_variables != rendered_variables {
        Some("let bindings")
    } else if !same_uses(authored_uses, rendered_uses) {
        Some("imports")
    } else if !same_module_calls(authored_module_calls, rendered_module_calls) {
        Some("module calls")
    } else if !same_arguments(authored_arguments, rendered_arguments) {
        Some("arguments")
    } else if !same_attribute_params(authored_attribute_params, rendered_attribute_params) {
        Some("attribute parameters")
    } else if authored_export_params != rendered_export_params {
        Some("exports")
    } else if !same_backend(authored_backend.as_ref(), rendered_backend.as_ref()) {
        Some("backend")
    } else if authored_state_blocks != rendered_state_blocks {
        Some("state blocks")
    } else if !same_user_functions(authored_user_functions, rendered_user_functions) {
        Some("functions")
    } else if !same_upstream_states(authored_upstream_states, rendered_upstream_states) {
        Some("upstream states")
    } else if !same_wait_bindings(authored_wait_bindings, rendered_wait_bindings) {
        Some("wait bindings")
    } else if !same_requires(authored_requires, rendered_requires) {
        Some("requirements")
    } else if authored_structural_bindings != rendered_structural_bindings {
        Some("structural bindings")
    } else if !same_warnings(authored_warnings, rendered_warnings) {
        Some("warnings")
    } else if !same_deferred_for_expressions(
        authored_deferred_for_expressions,
        rendered_deferred_for_expressions,
    ) {
        Some("deferred for expressions")
    } else {
        None
    };

    if let Some(section) = mismatch {
        anyhow::bail!(
            "Rendered example from {} changed parsed {section}",
            path.display()
        );
    }
    Ok(())
}

fn same_named_providers(
    left: &[ProviderConfig<ParserPlaceholder>],
    right: &[ProviderConfig<ParserPlaceholder>],
) -> bool {
    left.iter()
        .filter(|provider| provider.binding.is_some())
        .eq(right.iter().filter(|provider| provider.binding.is_some()))
}

fn same_resources(
    left: &[Resource<ModuleSource, ParserPlaceholder>],
    right: &[Resource<ModuleSource, ParserPlaceholder>],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let mut right = right.clone();
            normalize_pending_identity(&left.id, &mut right.id);
            left == &right
        })
}

fn same_data_sources(
    left: &[DataSource<ModuleSource, ParserPlaceholder>],
    right: &[DataSource<ModuleSource, ParserPlaceholder>],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let mut right = right.clone();
            normalize_pending_identity(&left.id, &mut right.id);
            left == &right
        })
}

fn normalize_pending_identity(left: &ResourceId, right: &mut ResourceId) {
    if matches!(left.identity_state(), ResourceIdentityState::Pending(_))
        && matches!(right.identity_state(), ResourceIdentityState::Pending(_))
    {
        let provider = std::mem::take(&mut right.provider);
        let resource_type = std::mem::take(&mut right.resource_type);
        let provider_instance = right.provider_instance.take();
        *right = left.clone();
        right.provider = provider;
        right.resource_type = resource_type;
        right.provider_instance = provider_instance;
    }
}

fn same_uses(
    left: &[carina_core::parser::UseStatement],
    right: &[carina_core::parser::UseStatement],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.path == right.path && left.alias == right.alias)
}

fn same_module_calls(
    left: &[carina_core::parser::ModuleCall],
    right: &[carina_core::parser::ModuleCall],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.module_name == right.module_name
                && left.binding_name == right.binding_name
                && left.arguments().count() == right.arguments().count()
                && left
                    .arguments()
                    .all(|(name, argument)| right.argument(name) == Some(argument))
        })
}

fn same_arguments(
    left: &[carina_core::parser::ArgumentParameter],
    right: &[carina_core::parser::ArgumentParameter],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.static_type == right.static_type
                && left.default == right.default
                && left.description == right.description
                && left.validations.len() == right.validations.len()
                && left
                    .validations
                    .iter()
                    .zip(&right.validations)
                    .all(|(left, right)| {
                        left.condition == right.condition
                            && left.error_message == right.error_message
                    })
        })
}

fn same_attribute_params(
    left: &[carina_core::parser::AttributeParameter],
    right: &[carina_core::parser::AttributeParameter],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.static_type == right.static_type
                && left.value == right.value
        })
}

fn same_backend(
    left: Option<&carina_core::parser::BackendConfig<ParserPlaceholder>>,
    right: Option<&carina_core::parser::BackendConfig<ParserPlaceholder>>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left.backend_type == right.backend_type && left.attributes == right.attributes
        }
        (None, None) => true,
        _ => false,
    }
}

fn same_user_functions(
    left: &std::collections::HashMap<String, carina_core::parser::UserFunction>,
    right: &std::collections::HashMap<String, carina_core::parser::UserFunction>,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(name, left)| {
            right.get(name).is_some_and(|right| {
                left.name == right.name
                    && left.return_type == right.return_type
                    && left.params.len() == right.params.len()
                    && left.params.iter().zip(&right.params).all(|(left, right)| {
                        left.name == right.name
                            && left.param_type == right.param_type
                            && left.default == right.default
                    })
                    && left.local_lets.len() == right.local_lets.len()
                    && left.local_lets.iter().zip(&right.local_lets).all(
                        |((left_name, left_value, _), (right_name, right_value, _))| {
                            left_name == right_name && left_value == right_value
                        },
                    )
                    && left.body.value == right.body.value
            })
        })
}

fn same_upstream_states(
    left: &[carina_core::parser::UpstreamState],
    right: &[carina_core::parser::UpstreamState],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.binding == right.binding && left.source == right.source)
}

fn same_wait_bindings(
    left: &[carina_core::parser::WaitBinding],
    right: &[carina_core::parser::WaitBinding],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.binding == right.binding
                && left.target == right.target
                && left.until_raw == right.until_raw
                && left.until_predicate.lhs_segments == right.until_predicate.lhs_segments
                && left.until_predicate.rhs == right.until_predicate.rhs
                && left.timeout_secs == right.timeout_secs
                && left.depends_on == right.depends_on
        })
}

fn same_requires(
    left: &[carina_core::parser::RequireBlock<ParserPlaceholder>],
    right: &[carina_core::parser::RequireBlock<ParserPlaceholder>],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.condition == right.condition && left.error_message == right.error_message
        })
}

fn same_warnings(
    left: &[carina_core::parser::ParseWarning],
    right: &[carina_core::parser::ParseWarning],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.kind == right.kind && left.message == right.message)
}

fn same_deferred_for_expressions(
    left: &[ParsedDeferredForExpression],
    right: &[ParsedDeferredForExpression],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.header == right.header
                && same_deferred_for_targets(&left.target, &right.target)
                && left.iterable == right.iterable
                && left.binding == right.binding
                && same_deferred_for_bodies(&left.body, &right.body)
        })
}

fn same_deferred_for_targets(left: &DeferredForTarget, right: &DeferredForTarget) -> bool {
    left == right
}

fn same_deferred_for_bodies(left: &DeferredForBody, right: &DeferredForBody) -> bool {
    match (left, right) {
        (DeferredForBody::Resource(left), DeferredForBody::Resource(right)) => same_resources(
            std::slice::from_ref(left.as_ref()),
            std::slice::from_ref(right.as_ref()),
        ),
        (DeferredForBody::Read(left), DeferredForBody::Read(right)) => same_data_sources(
            std::slice::from_ref(left.as_ref()),
            std::slice::from_ref(right.as_ref()),
        ),
        (DeferredForBody::ModuleCall(left), DeferredForBody::ModuleCall(right)) => {
            same_module_calls(std::slice::from_ref(left), std::slice::from_ref(right))
        }
        (DeferredForBody::Resource(_), _)
        | (DeferredForBody::Read(_), _)
        | (DeferredForBody::ModuleCall(_), _) => false,
    }
}

fn preprocess(source: &str) -> Result<String> {
    let removals = removals(source.as_bytes())?;
    let mut filtered = String::with_capacity(source.len());
    let mut kept_from = 0;
    for removal in removals {
        filtered.push_str(&source[kept_from..removal.start]);
        kept_from = removal.end;
    }
    filtered.push_str(&source[kept_from..]);

    let lines: Vec<&str> = filtered.split_inclusive('\n').collect();
    let Some(first_content) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return Ok(String::new());
    };
    let last_content = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .expect("first content line guarantees a last content line")
        + 1;
    let mut rendered = lines[first_content..last_content].concat();
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

fn removals(masked: &[u8]) -> Result<Vec<Range<usize>>> {
    let mut removals = Vec::new();
    let mut cursor = 0;
    let mut line_start = 0;
    let mut nesting_depth = 0usize;
    let mut line_has_code = false;

    while cursor < masked.len() {
        match masked[cursor] {
            b'\n' => {
                cursor += 1;
                line_start = cursor;
                line_has_code = false;
            }
            b' ' | b'\t' | b'\r' => cursor += 1,
            b'#' if nesting_depth == 0 && !line_has_code => {
                let end = line_removal_end(masked, cursor);
                removals.push(line_start..end);
                cursor = end;
                line_start = cursor;
                line_has_code = false;
            }
            b'/' if masked.get(cursor + 1) == Some(&b'/')
                && nesting_depth == 0
                && !line_has_code =>
            {
                let end = line_removal_end(masked, cursor);
                removals.push(line_start..end);
                cursor = end;
                line_start = cursor;
                line_has_code = false;
            }
            b'#' => cursor = skip_line_comment(masked, cursor),
            b'/' if masked.get(cursor + 1) == Some(&b'/') => {
                cursor = skip_line_comment(masked, cursor);
            }
            b'/' if masked.get(cursor + 1) == Some(&b'*') => {
                let end = skip_block_comment(masked, cursor);
                if update_line_start(masked, cursor, end, &mut line_start) {
                    line_has_code = true;
                }
                cursor = end;
            }
            b'\'' => {
                let end = skip_single_quoted_string(masked, cursor);
                update_line_start(masked, cursor, end, &mut line_start);
                line_has_code = true;
                cursor = end;
            }
            b'"' => {
                let end = skip_double_quoted_string(masked, cursor);
                if contains_heredoc_syntax(masked, cursor, end) {
                    anyhow::bail!("heredoc text inside a multiline string is ambiguous");
                }
                update_line_start(masked, cursor, end, &mut line_start);
                line_has_code = true;
                cursor = end;
            }
            b'<' if masked.get(cursor + 1) == Some(&b'<') => {
                let Some(end) = heredoc_end(masked, cursor) else {
                    line_has_code = true;
                    cursor += 1;
                    continue;
                };
                update_line_start(masked, cursor, end, &mut line_start);
                line_has_code = true;
                cursor = end;
            }
            b'{' | b'[' | b'(' => {
                nesting_depth += 1;
                line_has_code = true;
                cursor += 1;
            }
            b'}' | b']' | b')' => {
                nesting_depth = nesting_depth.saturating_sub(1);
                line_has_code = true;
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
                    && nesting_depth == 0
                    && !line_has_code
                    && masked
                        .get(cursor)
                        .is_some_and(|byte| byte.is_ascii_whitespace());
                if !is_provider {
                    line_has_code = true;
                    continue;
                }

                let block_end = find_provider_block_end(masked, cursor).ok_or_else(|| {
                    anyhow::anyhow!("parsed provider block has no balanced closing brace")
                })?;
                let removal_end = provider_removal_end(masked, block_end);
                removals.push(line_start..removal_end);
                update_line_start(masked, cursor, removal_end, &mut line_start);
                cursor = removal_end;
                line_has_code = false;
            }
            _ => {
                line_has_code = true;
                cursor += 1;
            }
        }
    }

    Ok(removals)
}

fn contains_heredoc_syntax(source: &[u8], start: usize, end: usize) -> bool {
    let mut cursor = start;
    while cursor + 1 < end {
        if source[cursor..].starts_with(b"<<")
            && heredoc_end(source, cursor).is_some_and(|heredoc_end| heredoc_end <= end)
        {
            return true;
        }
        cursor += 1;
    }
    false
}

fn heredoc_end(source: &[u8], start: usize) -> Option<usize> {
    let opening_line_end = source[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |relative| start + relative);
    let mut cursor = start + 2;
    if source.get(cursor) == Some(&b'-') {
        cursor += 1;
    }
    let quoted = source.get(cursor) == Some(&b'\'');
    if quoted {
        cursor += 1;
    }
    let marker_start = cursor;
    while source
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        cursor += 1;
    }
    if cursor == marker_start {
        return None;
    }
    let marker = &source[marker_start..cursor];
    if quoted {
        if source.get(cursor) != Some(&b'\'') {
            return None;
        }
        cursor += 1;
    }
    if !source[cursor..opening_line_end]
        .iter()
        .all(u8::is_ascii_whitespace)
    {
        return None;
    }

    let mut line_start = opening_line_end.checked_add(1)?;
    while line_start <= source.len() {
        let line_end = source[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(source.len(), |relative| line_start + relative);
        if trim_ascii_whitespace(&source[line_start..line_end]) == marker {
            return Some(line_end);
        }
        if line_end == source.len() {
            return None;
        }
        line_start = line_end + 1;
    }
    None
}

fn trim_ascii_whitespace(mut source: &[u8]) -> &[u8] {
    while source.first().is_some_and(u8::is_ascii_whitespace) {
        source = &source[1..];
    }
    while source.last().is_some_and(u8::is_ascii_whitespace) {
        source = &source[..source.len() - 1];
    }
    source
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
    let trailing = trim_ascii_whitespace(&source[block_end..line_end]);
    if trailing.is_empty() || trailing.starts_with(b"#") || trailing.starts_with(b"//") {
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
            b'<' if source.get(cursor + 1) == Some(&b'<') => {
                cursor = heredoc_end(source, cursor)?;
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

fn update_line_start(source: &[u8], start: usize, end: usize, line_start: &mut usize) -> bool {
    if let Some(relative) = source[start..end].iter().rposition(|byte| *byte == b'\n') {
        *line_start = start + relative + 1;
        true
    } else {
        false
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

    fn resource_text(example: &DocExample) -> &str {
        example
            .text_for("aws", "ec2.Vpc", ExampleKind::Resource)
            .expect("fixture identity")
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

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
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

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
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
            resource_text(&example),
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

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn text_is_unavailable_for_the_wrong_identity() {
        let example =
            load_fixture("aws.ec2.Vpc {}\n", ExampleKind::Resource).expect("valid checked example");

        let error = example
            .text_for("aws", "ec2.Subnet", ExampleKind::Resource)
            .expect_err("wrong identity must not expose text");

        assert_eq!(
            error.to_string(),
            "Documentation example checked for resource aws.ec2.Vpc cannot render resource \
             aws.ec2.Subnet"
        );
    }

    #[test]
    fn block_comment_tokens_do_not_affect_top_level_scanning() {
        let source = concat!(
            "/*\n",
            "provider aws {\n",
            "# not a page comment\n",
            "}\n",
            "*/\n",
            "# page-only note\n",
            "aws.ec2.Vpc {}\n",
        );
        let example = load_fixture(source, ExampleKind::Resource).expect("valid block comment");

        assert_eq!(
            resource_text(&example),
            concat!(
                "/*\n",
                "provider aws {\n",
                "# not a page comment\n",
                "}\n",
                "*/\n",
                "aws.ec2.Vpc {}\n",
            )
        );
    }

    #[test]
    fn multiline_block_comment_before_top_level_hash_comment_loads() {
        let source = "/* note\n   more */ # trailing\naws.ec2.Vpc {}\n";
        let example = load_fixture(source, ExampleKind::Resource)
            .expect("valid comment after a multiline block comment");

        assert!(resource_text(&example).contains("more */"));
        assert!(resource_text(&example).contains("aws.ec2.Vpc {}\n"));
    }

    #[test]
    fn multiline_block_comment_before_provider_loads() {
        let source = concat!(
            "/* n\n",
            " */ provider aws { region = aws.Region.ap_northeast_1 }\n",
            "aws.ec2.Vpc {}\n",
        );
        let example = load_fixture(source, ExampleKind::Resource)
            .expect("valid provider after a multiline block comment");

        assert!(resource_text(&example).contains(" */"));
        assert!(resource_text(&example).contains("aws.ec2.Vpc {}\n"));
    }

    #[test]
    fn interpolation_braces_and_quotes_do_not_change_comment_depth() {
        let source = concat!(
            "aws.ec2.Vpc {\n",
            "  description = \"${ \"}\" }\"\n",
            "  # reader-facing note\n",
            "}\n",
            "# page-only note\n",
        );
        let example = load_fixture(source, ExampleKind::Resource).expect("valid interpolation");

        assert_eq!(
            resource_text(&example),
            concat!(
                "aws.ec2.Vpc {\n",
                "  description = \"${ \"}\" }\"\n",
                "  # reader-facing note\n",
                "}\n",
            )
        );
    }

    #[test]
    fn escaped_single_quote_does_not_change_comment_depth() {
        let source = concat!(
            "aws.ec2.Vpc {\n",
            "  description = 'it\\'s }'\n",
            "  another = 'it\\'s quoted'\n",
            "  # reader-facing note\n",
            "}\n",
        );
        let example = load_fixture(source, ExampleKind::Resource).expect("valid quoted string");

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn top_level_let_heredoc_is_preserved() {
        let source = concat!(
            "let x = <<EOT\n",
            "# not a comment\n",
            "EOT\n",
            "aws.ec2.Vpc { description = x }\n",
        );
        let example = load_fixture(source, ExampleKind::Resource).expect("valid let heredoc");

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn top_level_slash_comment_is_stripped() {
        let example = load_fixture("// page-only note\naws.ec2.Vpc {}\n", ExampleKind::Resource)
            .expect("valid slash comment");

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn provider_prefixed_identifier_is_not_a_provider_block() {
        let source = concat!(
            "let provider_x = \"x\"\n",
            "let p = provider_x\n",
            "aws.ec2.Vpc { description = p }\n",
        );
        let example = load_fixture(source, ExampleKind::Resource)
            .expect("valid provider-prefixed identifier");

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn let_bound_provider_is_not_a_top_level_provider_block() {
        let source = "let p = provider aws {}\naws.ec2.Vpc {}\n";
        let example =
            load_fixture(source, ExampleKind::Resource).expect("valid named provider block");

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn provider_keyword_as_a_resource_prefix_is_not_a_provider_block() {
        let source = "provider.foo.Bar {}\naws.ec2.Vpc {}\n";
        let example = load_fixture(source, ExampleKind::Resource)
            .expect("valid resource whose provider is named provider");

        assert_eq!(resource_text(&example), source);
    }

    #[test]
    fn provider_block_in_the_middle_leaves_no_blank_line() {
        let example = load_fixture(
            concat!(
                "aws.ec2.Subnet {}\n",
                "provider aws { region = aws.Region.ap_northeast_1 }\n",
                "aws.ec2.Vpc {}\n",
            ),
            ExampleKind::Resource,
        )
        .expect("valid mid-file provider");

        assert_eq!(
            resource_text(&example),
            "aws.ec2.Subnet {}\naws.ec2.Vpc {}\n"
        );
    }

    #[test]
    fn provider_trailing_comment_is_removed_with_the_block() {
        let example = load_fixture(
            concat!("provider aws {} # provider-only note\n", "aws.ec2.Vpc {}\n",),
            ExampleKind::Resource,
        )
        .expect("valid provider comment");

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn block_comment_before_provider_is_removed_with_the_block() {
        let example = load_fixture(
            "/* provider note */ provider aws {}\naws.ec2.Vpc {}\n",
            ExampleKind::Resource,
        )
        .expect("valid provider prefix comment");

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn crlf_blank_lines_after_provider_are_trimmed() {
        let example = load_fixture(
            concat!(
                "provider aws {}\r\n",
                "\r\n",
                "# page-only note\r\n",
                "aws.ec2.Vpc {}\r\n",
            ),
            ExampleKind::Resource,
        )
        .expect("valid CRLF example");

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\r\n");
    }

    #[test]
    fn trailing_blank_lines_are_trimmed_to_one_newline() {
        let example = load_fixture("\n\t\naws.ec2.Vpc {}\n\n \t\r\n", ExampleKind::Resource)
            .expect("valid example surrounded by blank lines");

        assert_eq!(resource_text(&example), "aws.ec2.Vpc {}\n");
    }

    #[test]
    fn comments_inside_square_and_round_brackets_are_preserved() {
        let source = concat!(
            "let xs = [\n",
            "  # list note\n",
            "]\n",
            "let x = (\n",
            "  // expression note\n",
            ")\n",
            "# page-only note\n",
            "aws.ec2.Vpc {}\n",
        );

        assert_eq!(
            preprocess(source).expect("preprocess succeeds"),
            concat!(
                "let xs = [\n",
                "  # list note\n",
                "]\n",
                "let x = (\n",
                "  // expression note\n",
                ")\n",
                "aws.ec2.Vpc {}\n",
            )
        );
    }

    #[test]
    fn identical_heredoc_text_in_a_block_comment_does_not_mask_the_real_body() {
        let source = concat!(
            "/* copy:\n",
            "  d = <<EOT\n",
            "}\n",
            "EOT\n",
            "*/\n",
            "aws.ec2.Vpc {\n",
            "  d = <<EOT\n",
            "}\n",
            "EOT\n",
            "  # reader-facing note\n",
            "}\n",
            "# page-only note\n",
        );
        let example = load_fixture(source, ExampleKind::Resource)
            .expect("comment copy must not relocate the heredoc");

        assert_eq!(
            resource_text(&example),
            source.strip_suffix("# page-only note\n").unwrap()
        );
    }

    #[test]
    fn heredoc_text_inside_multiline_string_fails_closed() {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        let source = concat!(
            "aws.ec2.Vpc {\n",
            "  a = \"<<EOT\n",
            "}\n",
            "EOT\n",
            "\"\n",
            "  b = <<EOT\n",
            "}\n",
            "EOT\n",
            "  # inner note kept?\n",
            "  provider = \"x\"\n",
            "  cidr_block = \"10.0.0.0/16\"\n",
            "  tags = { Name = \"y\" }\n",
            "}\n",
            "aws.ec2.Subnet {\n",
            "}\n",
        );
        let path = write_fixture(examples_dir.path(), "ec2_vpc", source);

        let error = load(
            examples_dir.path(),
            "ec2_vpc",
            "aws",
            "ec2.Vpc",
            ExampleKind::Resource,
        )
        .expect_err("ambiguous heredoc text must fail closed");

        assert!(error.to_string().contains(&path.display().to_string()));
        assert!(
            error.chain().any(|cause| cause
                .to_string()
                .contains("heredoc text inside a multiline string")),
            "{error:#}"
        );
    }

    #[test]
    fn semantic_mismatch_reports_the_example_path() {
        let path = Path::new("semantic-mismatch.crn");
        let authored = parse_file(
            "let x = \"original\"\naws.ec2.Vpc { description = x }\n",
            path,
            &ProviderContext::default(),
        )
        .expect("authored source parses");
        let error = check_rendered(
            &authored,
            "let x = \"changed\"\naws.ec2.Vpc { description = x }\n".to_string(),
            path,
            ExpectedDeclaration::new("aws", "ec2.Vpc", ExampleKind::Resource),
        )
        .expect_err("semantic mismatch must fail");

        assert_eq!(
            error.to_string(),
            "Rendered example from semantic-mismatch.crn changed parsed resources"
        );
    }

    #[test]
    fn deferred_structural_if_cannot_be_checked_for_render_equivalence() {
        let path = Path::new("deferred-structural-if.crn");
        let parsed = parse_file(
            concat!(
                "arguments { enabled: Bool }\n",
                "let selected = if enabled {\n",
                "  aws.ec2.Vpc {}\n",
                "}\n",
            ),
            path,
            &ProviderContext::default(),
        )
        .expect("deferred structural if parses");
        assert_eq!(parsed.deferred_module_ifs.len(), 1);

        let error = ensure_semantically_equal(&parsed, &parsed, path)
            .expect_err("deferred structural if must fail closed");

        assert_eq!(
            error.to_string(),
            "Example deferred-structural-if.crn contains a deferred structural `if`; such \
             examples cannot be checked for render equivalence"
        );
    }

    #[test]
    fn deferred_structural_if_length_difference_is_a_semantic_mismatch() {
        let path = Path::new("deferred-structural-if-mismatch.crn");
        let without_deferred_if =
            parse_file("", path, &ProviderContext::default()).expect("empty file parses");
        let with_deferred_if = parse_file(
            concat!(
                "arguments { enabled: Bool }\n",
                "let selected = if enabled {\n",
                "  aws.ec2.Vpc {}\n",
                "}\n",
            ),
            path,
            &ProviderContext::default(),
        )
        .expect("deferred structural if parses");

        let error = ensure_semantically_equal(&without_deferred_if, &with_deferred_if, path)
            .expect_err("different deferred structural if counts must be a mismatch");

        assert_eq!(
            error.to_string(),
            "Rendered example from deferred-structural-if-mismatch.crn changed parsed deferred \
             structural `if` expressions"
        );
    }

    #[test]
    fn different_unbound_deferred_for_templates_are_a_semantic_mismatch() {
        let source = concat!(
            "let orgs = upstream_state { source = \"../orgs\" }\n",
            "for account_id in orgs.accounts {\n",
            "  aws.ec2.Vpc { description = account_id }\n",
            "}\n",
        );
        let authored = parse_file(
            source,
            Path::new("authored-deferred-for.crn"),
            &ProviderContext::default(),
        )
        .expect("authored deferred for parses");
        let rendered = parse_file(
            source,
            Path::new("rendered-deferred-for.crn"),
            &ProviderContext::default(),
        )
        .expect("rendered deferred for parses");
        let [authored_deferred] = authored.parsed_deferred_for_expressions() else {
            panic!("authored fixture must contain one deferred for")
        };
        let [rendered_deferred] = rendered.parsed_deferred_for_expressions() else {
            panic!("rendered fixture must contain one deferred for")
        };
        assert!(matches!(
            authored_deferred.target,
            DeferredForTarget::Unbound { .. }
        ));
        assert!(matches!(
            rendered_deferred.target,
            DeferredForTarget::Unbound { .. }
        ));
        assert_ne!(authored_deferred.target, rendered_deferred.target);

        let path = Path::new("unbound-template-mismatch.crn");
        let error = ensure_semantically_equal(&authored, &rendered, path)
            .expect_err("different unbound templates must be a mismatch");

        assert_eq!(
            error.to_string(),
            "Rendered example from unbound-template-mismatch.crn changed parsed deferred for \
             expressions"
        );
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
