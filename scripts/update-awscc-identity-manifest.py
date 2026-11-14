#!/usr/bin/env python3
"""Explicitly refresh the pinned awscc schema inventory from a clean local checkout.

Usage: python3 scripts/update-awscc-identity-manifest.py /path/to/carina-provider-awscc
Compiles only schema modules, offline, under this worktree's target. No AWS access.
Ordinary tests use the checked-in result and never run this updater.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path(sys.argv[1]).resolve()
SCRATCH = ROOT / "target/codex/awscc-identity-manifest"
OUTPUT = ROOT / "carina-codegen-aws/tests/fixtures/awscc-identified-inputs.json"


def git(*args):
    return subprocess.check_output(["git", "-C", str(SOURCE), *args], text=True).strip()


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
    "commit": git("rev-parse", "HEAD"),
    "carina_revision": core["rev"],
    "aws_types_revision": types["rev"],
    "content_sha256": hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
    "files": hashes,
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
    let mut inputs = Vec::new();
    let mut declarations = Vec::new();
    for config in schemas::generated::configs() {{
        for leaf in inventory::schema_leaves("awscc", config.schema.as_schema()) {{
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
# Keep one portable definition per identity and one readable line per occurrence.
def pretty(value):
    return json.dumps(value, indent=2, sort_keys=True).replace("\n", "\n  ")


lines = ['{', '  "format": 1,', '  "source": ' + pretty(provenance) + ',',
         '  "definitions": ' + pretty(definitions) + ',']
for name, rows in (("inputs", inputs), ("outputs", outputs)):
    lines.append(f'  "{name}": [')
    lines.append(',\n'.join('    ' + json.dumps(row, sort_keys=True) for row in rows))
    lines.append('  ]' + (',' if name == "inputs" else ''))
lines.append('}')
OUTPUT.write_text('\n'.join(lines) + '\n')
print(f"{len(inputs)} identified inputs; {len(outputs)} outputs; {len(definitions)} definitions -> {OUTPUT}")
