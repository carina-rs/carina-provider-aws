//! Regression test for the closed set of called AWS operations.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::resend_safety_generated::{ALL_OPERATIONS, DECIDED_OPERATIONS};

fn handwritten_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read provider source directory") {
        let entry = entry.expect("read provider source entry");
        let path = entry.path();
        if path.is_dir() {
            if path.ends_with("schemas/generated") {
                continue;
            }
            handwritten_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| {
                name != "tests.rs"
                    && name != "called_operation_decisions.rs"
                    && !name.to_string_lossy().ends_with("_generated.rs")
            })
        {
            sources.push(path);
        }
    }
}

fn quoted_literal_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            byte if byte == quote => return cursor + 1,
            _ => cursor += 1,
        }
    }
    bytes.len()
}

fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let hashes = cursor - hashes_start;
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes.get(cursor + 1..cursor + 1 + hashes)
                == Some(&bytes[hashes_start..hashes_start + hashes])
        {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn non_code_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start..start + 2) == Some(b"//") {
        return Some(
            bytes[start + 2..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| start + 2 + offset + 1),
        );
    }
    if bytes.get(start..start + 2) == Some(b"/*") {
        let mut cursor = start + 2;
        let mut depth = 1_u32;
        while cursor < bytes.len() && depth > 0 {
            if bytes.get(cursor..cursor + 2) == Some(b"/*") {
                depth += 1;
                cursor += 2;
            } else if bytes.get(cursor..cursor + 2) == Some(b"*/") {
                depth -= 1;
                cursor += 2;
            } else {
                cursor += 1;
            }
        }
        return Some(cursor);
    }
    if bytes[start] == b'"' {
        return Some(quoted_literal_end(bytes, start, b'"'));
    }
    if bytes[start] == b'b' && bytes.get(start + 1) == Some(&b'"') {
        return Some(quoted_literal_end(bytes, start + 1, b'"'));
    }
    raw_string_end(bytes, start)
}

fn cfg_test_item_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = start;
    let mut depth = 0_u32;
    let mut body_started = false;
    while cursor < bytes.len() {
        if let Some(end) = non_code_end(bytes, cursor) {
            cursor = end;
            continue;
        }
        match bytes[cursor] {
            b'{' => {
                body_started = true;
                depth += 1;
            }
            b'}' if body_started => {
                depth -= 1;
                if depth == 0 {
                    return cursor + 1;
                }
            }
            b';' if !body_started => return cursor + 1,
            _ => {}
        }
        cursor += 1;
    }
    bytes.len()
}

/// Remove only items guarded by the exact `cfg(test)` attribute.
///
/// Keeping the surrounding production source matters for files such as
/// `lib.rs` and `sdk_config.rs`, which intentionally define production items
/// after an inline test module.
fn strip_cfg_test_items(source: &str) -> String {
    const ATTRIBUTE: &str = "#[cfg(test)]";
    let mut production = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find(ATTRIBUTE) {
        let item_start = cursor + relative_start;
        production.push_str(&source[cursor..item_start]);
        cursor = cfg_test_item_end(source, item_start + ATTRIBUTE.len());
    }
    production.push_str(&source[cursor..]);
    production
}

#[test]
fn cfg_test_stripping_keeps_production_calls_after_mid_file_test_items() {
    let source = r#"
fn before(client: &Client) { client.describe_vpcs(); }

#[cfg(test)]
mod tests {
    fn test_only(client: &Client) { client.delete_vpc(); }
}

fn after(client: &Client) { client.create_vpc(); }
"#;

    let production = strip_cfg_test_items(source);

    assert!(production.contains("describe_vpcs"));
    assert!(production.contains("create_vpc"));
    assert!(!production.contains("delete_vpc"));
}

#[test]
fn every_called_sdk_operation_has_an_explicit_decision() {
    let mut paths = Vec::new();
    handwritten_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut paths,
    );
    paths.sort();

    let source = paths
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            strip_cfg_test_items(&source)
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Generated SDK method names are matched without a service qualifier.
    // A same-named call in another service can therefore satisfy this scan;
    // the generated per-service policy remains authoritative at runtime.
    let decided_names = DECIDED_OPERATIONS
        .iter()
        .map(|(_, operation)| *operation)
        .collect::<HashSet<_>>();

    let mut called = Vec::new();
    for (_, operation) in ALL_OPERATIONS {
        let call = Regex::new(&format!(r"\.{}\s*\(", regex::escape(operation)))
            .expect("generated operation name is valid regex text");
        if call.is_match(&source) {
            called.push(*operation);
        }
    }
    called.sort_unstable();
    called.dedup();

    let undecided = called
        .iter()
        .filter(|operation| !decided_names.contains(**operation))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        undecided.is_empty(),
        "called AWS operations need resend-safety decisions: {}",
        undecided.join(", ")
    );
    assert!(
        called.len() >= 100,
        "source scan unexpectedly found only {} SDK calls",
        called.len()
    );
}
