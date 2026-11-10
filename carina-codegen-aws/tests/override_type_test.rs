//! Compile-fail coverage without a nested Cargo invocation.
use std::path::PathBuf;
use std::process::Command;
mod support;

#[test]
fn identity_662_override_table_cannot_hold_constructor_text() {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let library = support::linked_library;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ui");
    for (fixture, valid) in [
        ("replacement.rs", false),
        ("struct-replacement.rs", false),
        ("data-source-replacement.rs", false),
        ("attribute-type-replacement.rs", false),
        ("closure-replacement.rs", false),
        ("catalog-structural-replacement.rs", false),
        ("identity-decoration.rs", true),
    ] {
        let temporary = support::build_tempdir();
        let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2024")
            .arg("--crate-type=bin")
            .arg("--extern")
            .arg(format!(
                "carina_codegen_aws={}",
                library("carina_codegen_aws").display()
            ))
            .arg("--extern")
            .arg(format!("carina_core={}", library("carina_core").display()))
            .arg("--extern")
            .arg(format!(
                "carina_aws_types={}",
                library("carina_aws_types").display()
            ))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--out-dir")
            .arg(temporary.path())
            .arg(fixtures.join(fixture))
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), valid, "{fixture}: {error}");
        if !valid {
            assert!(
                error.contains("mismatched types"),
                "{fixture}: unexpected compiler failure: {error}"
            );
        }
    }
}

#[test]
fn compiler_fixture_resolution_uses_hashes_instead_of_newest_mtime() {
    use std::time::{Duration, SystemTime};
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("debug");
    let deps = profile.join("deps");
    std::fs::create_dir_all(&deps).unwrap();
    for (hash, checksum, seconds) in [("old", 11u64, 1), ("new", 22u64, 2)] {
        let fingerprint = profile.join(".fingerprint").join(format!("example-{hash}"));
        std::fs::create_dir_all(&fingerprint).unwrap();
        let text = checksum
            .to_le_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        std::fs::write(fingerprint.join("lib-example"), text).unwrap();
        let file = std::fs::File::create(deps.join(format!("libexample-{hash}.rlib"))).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }
    assert_eq!(
        support::resolve_library(&deps, "example", 11),
        deps.join("libexample-old.rlib")
    );
}
