use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use carina_aws_types::Partition;
use serde_json::Value;

fn locked_s3_endpoint_source() -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args(["metadata", "--format-version", "1", "--offline", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|error| panic!("could not run {cargo:?} metadata offline: {error}"));
    assert!(
        output.status.success(),
        "cargo metadata --offline --locked failed ({}):\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .expect("cargo metadata must return a valid JSON document");
    let workspace_root = metadata["workspace_root"]
        .as_str()
        .expect("cargo metadata must include workspace_root");
    let lock_path = Path::new(workspace_root).join("Cargo.lock");
    let lock_source = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", lock_path.display()));
    let lock: toml::Value = toml::from_str(&lock_source)
        .unwrap_or_else(|error| panic!("could not parse {}: {error}", lock_path.display()));
    let versions: BTreeSet<_> = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .expect("Cargo.lock must contain a package array")
        .iter()
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some("aws-sdk-s3"))
        .map(|package| {
            package
                .get("version")
                .and_then(toml::Value::as_str)
                .expect("aws-sdk-s3 in Cargo.lock must have a version")
        })
        .collect();
    assert_eq!(
        versions.len(),
        1,
        "expected one locked aws-sdk-s3 version in {}, found {versions:?}",
        lock_path.display()
    );
    let version = versions.first().expect("one locked aws-sdk-s3 version");
    let packages: Vec<_> = metadata["packages"]
        .as_array()
        .expect("cargo metadata must include a packages array")
        .iter()
        .filter(|package| {
            package["name"].as_str() == Some("aws-sdk-s3")
                && package["version"].as_str() == Some(*version)
        })
        .collect();
    let [package] = packages.as_slice() else {
        panic!(
            "expected one aws-sdk-s3 {version} package in offline cargo metadata, found {}",
            packages.len()
        );
    };
    let manifest = package["manifest_path"]
        .as_str()
        .expect("aws-sdk-s3 metadata must include manifest_path");
    Path::new(manifest)
        .parent()
        .expect("aws-sdk-s3 manifest_path must have a package directory")
        .join("src/endpoint_lib.rs")
}

fn embedded_partitions(path: &Path) -> Value {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read SDK source {}: {error}", path.display()));
    let pattern = regex::Regex::new(
        r#"(?s)PartitionResolver::new_from_json\s*\(\s*(b"(?:\\.|[^"\\])*")\s*\)"#,
    )
    .expect("valid SDK partition byte-string matcher");
    let literals: Vec<_> = pattern.captures_iter(&source).collect();
    let [literal] = literals.as_slice() else {
        panic!(
            "expected one PartitionResolver::new_from_json(b\"...\") literal in {}, found {}",
            path.display(),
            literals.len()
        );
    };
    // Parse Rust byte-string escapes, including \xNN, before parsing the JSON.
    let bytes = syn::parse_str::<syn::LitByteStr>(&literal[1])
        .unwrap_or_else(|error| panic!("invalid SDK byte string in {}: {error}", path.display()))
        .value();
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("invalid SDK partitions JSON in {}: {error}", path.display())
    })
}

#[test]
fn partitions_match_locked_s3_sdk_metadata() {
    let endpoint_source = locked_s3_endpoint_source();
    let metadata = embedded_partitions(&endpoint_source);
    let partitions = metadata["partitions"]
        .as_array()
        .expect("SDK partitions JSON must contain a partitions array");
    let sdk_ids: BTreeSet<_> = partitions
        .iter()
        .map(|partition| {
            partition["id"]
                .as_str()
                .expect("every SDK partition must have a string id")
        })
        .collect();
    assert_eq!(
        sdk_ids.len(),
        partitions.len(),
        "duplicate SDK partition ids"
    );
    let supported_ids: BTreeSet<_> = Partition::ALL.into_iter().map(Partition::id).collect();
    assert_eq!(
        supported_ids,
        sdk_ids,
        "Partition::ALL must match SDK metadata in {}",
        endpoint_source.display()
    );

    for sdk_partition in partitions {
        let id = sdk_partition["id"].as_str().expect("SDK partition id");
        let partition = Partition::from_id(id)
            .unwrap_or_else(|| panic!("SDK partition {id} is missing from Partition::ALL"));
        let dns_suffix = sdk_partition["outputs"]["dnsSuffix"]
            .as_str()
            .unwrap_or_else(|| panic!("SDK partition {id} must have outputs.dnsSuffix"));
        assert_eq!(
            partition.dns_suffix(),
            dns_suffix,
            "DNS suffix differs from SDK metadata for {id}"
        );
        let regions = sdk_partition["regions"]
            .as_object()
            .unwrap_or_else(|| panic!("SDK partition {id} must have a regions object"));
        for region in regions.keys().filter(|region| !region.ends_with("-global")) {
            assert_eq!(
                Partition::from_region(region),
                partition,
                "SDK region {region} must resolve to partition {id}"
            );
        }
    }
}
