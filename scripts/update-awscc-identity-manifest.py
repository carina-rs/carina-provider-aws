#!/usr/bin/env python3
"""Explicitly refresh the pinned awscc schema inventory from a clean local checkout.

Usage: ./scripts/update-awscc-identity-manifest.py /path/to/carina-provider-awscc
Compiles only schema modules, offline, under this worktree's target. No AWS access.
Ordinary tests use the checked-in result and never run this updater.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path, help="Clean local awscc checkout")
parser.add_argument("--allow-unpublished", action="store_true",
                    help="Explicitly allow a commit not reachable from local origin/main")
arguments = parser.parse_args()
SOURCE = arguments.source.resolve()
SCRATCH = ROOT / "target/codex/awscc-identity-manifest"
OUTPUT = ROOT / "carina-codegen-aws/tests/fixtures/awscc-identified-inputs.json"


def git(*args):
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


commit = git("rev-parse", "HEAD")
if not arguments.allow_unpublished:
    ancestry = subprocess.run(
        ["git", "-C", str(SOURCE), "merge-base", "--is-ancestor", commit, "origin/main"],
        capture_output=True, text=True,
    )
    if ancestry.returncode:
        raise SystemExit(
            f"Refusing manifest commit {commit}: not reachable from origin/main "
            "or ancestry could not be verified. No refs were fetched. "
            "Use --allow-unpublished only for an intentional unpublished snapshot."
        )


manifest = tomllib.loads((SOURCE / "Cargo.toml").read_text())
dependencies = manifest["workspace"]["dependencies"]
core = dependencies["carina-core"]
types = dependencies["carina-aws-types"]
schema_root = SOURCE / "carina-provider-awscc/src/schemas"
files = sorted({
    SOURCE / "Cargo.toml", SOURCE / "Cargo.lock",
    SOURCE / "carina-provider-awscc/src/partition.rs",
    *(schema_root / name for name in (
        "config.rs", "catalog.rs", "awscc_types.rs", "pattern.rs",
        "scheduler.rs", "string_identity.rs")),
    *(schema_root / "generated").rglob("*.rs"),
})
relative = [str(path.relative_to(SOURCE)) for path in files]
if git("status", "--porcelain", "--", *relative):
    raise SystemExit("Manifest provenance requires a clean awscc schema checkout")
hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest()
          for name, path in zip(relative, files)}
provenance = {
    "repository": "https://github.com/carina-rs/carina-provider-awscc",
    "commit": commit,
    "carina_revision": core["rev"],
    "aws_types_revision": types["rev"],
    "upstream_file_sha256": hashes,
}
SCRATCH.mkdir(parents=True, exist_ok=True)
(SCRATCH / "Cargo.toml").write_text(f'''[package]
name = "awscc-identity-manifest"
version = "0.0.0"
edition = "2024"
[workspace]
[features]
test-utils = []
[dependencies]
carina-core = {{ git = {json.dumps(core['git'])}, rev = {json.dumps(core['rev'])} }}
carina-provider-protocol = {{ git = {json.dumps(core['git'])}, rev = {json.dumps(core['rev'])} }}
carina-aws-types = {{ git = {json.dumps(types['git'])}, rev = {json.dumps(types['rev'])}, default-features = false }}
regex = "1"
regex-syntax = "0.8"
base64 = "0.22"
heck = "0.5"
serde_json = "1"
''')
modules = "\n".join(
    f"#[path = {json.dumps(str(schema_root / name / 'mod.rs' if name == 'generated' else schema_root / (name + '.rs')))}] pub mod {name};"
    for name in ("config", "catalog", "awscc_types", "pattern", "scheduler", "string_identity", "generated")
)
rust = f'''#![allow(dead_code)]
#[path = {json.dumps(str(SOURCE / 'carina-provider-awscc/src/partition.rs'))}] mod partition;
#[path = {json.dumps(str(ROOT / 'carina-codegen-aws/tests/support/schema_inventory.rs'))}] mod inventory;
pub mod schemas {{ {modules} }}
fn main() {{
    use heck::ToSnakeCase;
    let mut inputs = Vec::new();
    let mut declarations = Vec::new();
    for config in schemas::generated::configs() {{
        let (service, resource) = config.resource_type_name.split_once('.').unwrap();
        let file = format!("carina-provider-awscc/src/schemas/generated/{{service}}/{{}}.rs", resource.to_snake_case());
        for mut leaf in inventory::schema_leaves("awscc", config.schema.as_schema()) {{
            leaf["file"] = serde_json::json!(file);
            if !leaf["identity"].is_null() {{
                if leaf["writable"] == true {{ inputs.push(leaf.clone()); }}
                declarations.push(leaf);
            }}
        }}
    }}
    inputs.sort_by_key(serde_json::Value::to_string);
    declarations.sort_by_key(serde_json::Value::to_string);
    let result = serde_json::json!({{"format": 1, "inputs": inputs, "declarations": declarations}});
    println!("{{}}", serde_json::to_string(&result).unwrap());
}}
'''
(SCRATCH / "src").mkdir(exist_ok=True)
(SCRATCH / "src/main.rs").write_text(rust)
# Seed the exact transitive dependency versions used by the snapshot.
(SCRATCH / "Cargo.lock").write_bytes((SOURCE / "Cargo.lock").read_bytes())
environment = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="8",
                   NEXTEST_TEST_THREADS="8", CARGO_TARGET_DIR=str(ROOT / "target"))
result = subprocess.check_output([
    "nice", "-n", "19", "cargo", "run", "--offline", "--quiet",
    "--manifest-path", str(SCRATCH / "Cargo.toml")], env=environment, cwd=ROOT, text=True)
snapshot = json.loads(result)
definitions = {}
inputs, outputs = [], []
for original in snapshot["declarations"]:
    leaf = dict(original)
    definition = leaf.pop("definition")
    leaf.pop("anonymous_string")
    identity = leaf["identity"]
    if identity in definitions and definitions[identity] != definition:
        raise SystemExit(f"awscc has conflicting definitions of {identity}")
    definitions[identity] = definition
    (inputs if leaf["writable"] else outputs).append(leaf)


def checksum(value):
    """Canonical JSON shared with the Rust guard: sorted keys, compact UTF-8."""
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


# Upstream byte digests are provenance. Per-file inventory digests can be
# recomputed from the checked-in data without upstream source files or network.
file_data = {path: {"upstream_sha256": digest, "definitions": {}, "inputs": [], "outputs": []}
             for path, digest in hashes.items()}
for kind, rows in (("inputs", inputs), ("outputs", outputs)):
    for leaf in rows:
        data = file_data[leaf["file"]]
        data[kind].append(leaf)
        data["definitions"][leaf["identity"]] = definitions[leaf["identity"]]
provenance["files"] = {path: checksum(data) for path, data in file_data.items()}
content = {"format": 2, "source": provenance, "definitions": definitions,
           "inputs": inputs, "outputs": outputs}
provenance["content_sha256"] = checksum(content)
# Keep one portable definition per identity and one readable line per occurrence.
def pretty(value):
    return json.dumps(value, indent=2, sort_keys=True).replace("\n", "\n  ")


lines = ['{', '  "format": 2,', '  "source": ' + pretty(provenance) + ',',
         '  "definitions": ' + pretty(definitions) + ',']
for name, rows in (("inputs", inputs), ("outputs", outputs)):
    lines.append(f'  "{name}": [')
    lines.append(',\n'.join('    ' + json.dumps(row, sort_keys=True) for row in rows))
    lines.append('  ]' + (',' if name == "inputs" else ''))
lines.append('}')
OUTPUT.write_text('\n'.join(lines) + '\n')
print(f"{len(inputs)} identified inputs; {len(outputs)} outputs; {len(definitions)} definitions -> {OUTPUT}")
