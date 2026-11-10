//! Select the exact dependency artifacts recorded for this test binary.
//! Cargo's fingerprints include dependency checksums; mtimes play no role.
use std::path::{Path, PathBuf};

pub fn resolve_library(deps: &Path, name: &str, expected: u64) -> PathBuf {
    let expected = expected
        .to_le_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let fingerprints = deps
        .parent()
        .expect("profile directory")
        .join(".fingerprint");
    let mut matches = std::fs::read_dir(&fingerprints)
        .expect("Cargo fingerprints")
        .map(|entry| entry.unwrap().path())
        .filter_map(|directory| {
            if std::fs::read_to_string(directory.join(format!("lib-{name}")))
                .ok()
                .as_deref()
                != Some(expected.as_str())
            {
                return None;
            }
            let hash = directory.file_name()?.to_str()?.rsplit('-').next()?;
            let library = deps.join(format!("lib{name}-{hash}.rlib"));
            library.is_file().then_some(library)
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    assert_eq!(
        matches.len(),
        1,
        "expected one Cargo-matched rlib for {name}, found {matches:?}"
    );
    matches.pop().unwrap()
}

pub fn linked_library(name: &str) -> PathBuf {
    let executable = std::env::current_exe().expect("current test binary");
    let deps = executable.parent().expect("dependency directory");
    let hash = executable
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .rsplit('-')
        .next()
        .unwrap();
    let directory = deps
        .parent()
        .unwrap()
        .join(".fingerprint")
        .join(format!("{}-{hash}", env!("CARGO_PKG_NAME")));
    let mut targets = std::fs::read_dir(&directory)
        .expect("current test's Cargo fingerprint")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("test-")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets.len(),
        1,
        "expected one test target fingerprint in {directory:?}"
    );
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(targets.pop().unwrap()).unwrap()).unwrap();
    let expected = metadata["deps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dep| dep[1] == name)
        .unwrap_or_else(|| panic!("{name} is not linked by this test"))[3]
        .as_u64()
        .expect("dependency checksum");
    resolve_library(deps, name, expected)
}

/// Compiler fixtures share Cargo's artifact root instead of the system tmp dir.
pub fn build_tempdir() -> tempfile::TempDir {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        });
    std::fs::create_dir_all(&target).unwrap();
    tempfile::tempdir_in(target).unwrap()
}
