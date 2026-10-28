//! Regression test for the closed set of called AWS operations.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::resend_safety_generated::{DECIDED_OPERATIONS, TOKEN_OPERATIONS};

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

fn locked_sdk_operations() -> Vec<(String, String)> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let metadata = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .current_dir(manifest_dir)
        .env("RUSTC_WRAPPER", "")
        .output()
        .expect("run cargo metadata for the locked AWS SDK crates");
    assert!(
        metadata.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("parse cargo metadata");
    let mut operations = Vec::new();
    for package in metadata["packages"]
        .as_array()
        .expect("cargo metadata packages array")
    {
        let Some(package_name) = package["name"].as_str() else {
            continue;
        };
        let Some(service) = package_name.strip_prefix("aws-sdk-") else {
            continue;
        };
        let manifest_path = PathBuf::from(
            package["manifest_path"]
                .as_str()
                .expect("AWS SDK package manifest_path"),
        );
        let operation_dir = manifest_path
            .parent()
            .expect("AWS SDK manifest has a parent")
            .join("src/operation");
        for entry in std::fs::read_dir(&operation_dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", operation_dir.display()))
        {
            let entry = entry.expect("read AWS SDK operation entry");
            if entry
                .file_type()
                .expect("AWS SDK operation file type")
                .is_dir()
            {
                operations.push((
                    service.to_string(),
                    entry.file_name().to_string_lossy().into_owned(),
                ));
            }
        }
    }
    operations.sort();
    operations.dedup();
    operations
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
    for (_, operation) in locked_sdk_operations() {
        let call = Regex::new(&format!(r"\.{}\s*\(", regex::escape(&operation)))
            .expect("generated operation name is valid regex text");
        if call.is_match(&source) {
            called.push(operation);
        }
    }
    called.sort_unstable();
    called.dedup();

    let undecided = called
        .iter()
        .filter(|operation| !decided_names.contains(operation.as_str()))
        .cloned()
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

#[test]
fn token_bearing_operations_use_only_token_entry_points() {
    let mut paths = Vec::new();
    handwritten_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut paths,
    );
    paths.sort();

    let mut violations = Vec::new();
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let source = strip_cfg_test_items(&source);
        let explicit_setters = source.matches(".client_token(").count()
            + source.matches(".idempotency_token(").count();
        let token_factories = TOKEN_OPERATIONS
            .iter()
            .map(|(_, operation)| {
                Regex::new(&format!(r"\.{}\s*\(", regex::escape(operation)))
                    .expect("generated operation name is valid regex text")
                    .find_iter(&source)
                    .count()
            })
            .sum::<usize>();
        let token_entry_points = source.matches("send_aws_operation_with_token(").count()
            + source.matches("retry_aws_operation_with_token(").count();

        // AWS fluent builders expose an inherent `send`, so the type system
        // cannot hide that bypass. This scan pairs the generated WithToken set
        // with two syntactic invariants: callers never set tokens themselves,
        // and every token-bearing builder is routed through a token entry point.
        if explicit_setters != 0 || token_entry_points < token_factories {
            violations.push(format!(
                "{}: {token_factories} token operation(s), {token_entry_points} token entry point(s), {explicit_setters} explicit token setter(s)",
                path.display()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "WithToken operations must be sent only through token entry points:\n{}",
        violations.join("\n")
    );
}
