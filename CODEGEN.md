# Schema Code Generation

This document explains how to generate and maintain resource schemas from AWS Smithy models.

## Overview

The AWS provider uses three principal Rust codegen outputs:

1. **Schema definitions** (`schemas/generated/`) — resource attribute schemas generated from Smithy models
2. **Provider boilerplate** (`provider_generated.rs`) — delete/update method stubs generated from `ResourceDef` metadata
3. **Typed send models** (`send_models_generated.rs`) — exhaustive create/update inputs generated from the same writable schema attributes

All are produced by the `carina-codegen-aws` crate.

Provider reference Markdown is also generated from the same Smithy models with
`./scripts/generate-docs.sh`. `carina-docgen` allocates registry-compatible
heading ids as each heading is written, resolves typed link targets when the
page is finished, and validates the finished page. Generation fails on missing
targets, dead local fragments, Markdown links hidden inside code spans, or
duplicated enum/struct section headings. Markdown generation requires
`--examples-dir`, and every generated resource or data-source page must have a
matching `<service>_<page-stem>/main.crn`; generation fails if one is missing.
The shared `carina_docgen::example` module removes provider blocks and top-level
comment lines while preserving heredocs and quoted strings. Put reader-facing
notes in indented comments inside a resource or data-source body so they remain
on the generated page. The module parses both the authored source and the exact
text passed to the renderer; generation fails closed when those parses differ,
apart from intentionally removed unnamed provider blocks. The rendered text
must be non-empty and must declare the documented resource or read the
documented data source. Generation also fails when an example directory has no
corresponding rendered page.
Codegen inserts each required example before validation. The script adds
frontmatter and strips the generated H1 because Starlight renders the
frontmatter title as the page heading. The resulting `generated-docs/` tree is
a transient build artifact and is not committed. The docs-bundle CI job also
runs `carina-docgen-check` on the built tarball, after the bundle has
reconstructed each page's H1 from its frontmatter title.

## Prerequisites

- Rust toolchain (to build the codegen binary)
- Internet access (to download Smithy model JSON files from GitHub)

No AWS credentials are needed — Smithy models are downloaded from the public `aws/api-models-aws` GitHub repository.

## Generating Schemas

From the project root:

```bash
./scripts/generate-schemas-smithy.sh
```

This will:

1. Download Smithy model JSON files if not already cached (`scripts/download-smithy-models.sh`)
2. Build and run `carina-codegen-aws` with `--format rust`
3. Generate Rust files in `carina-provider-aws/src/schemas/generated/`
4. Run `cargo fmt`

## Generating Provider Boilerplate

```bash
./scripts/generate-provider.sh
```

This generates `carina-provider-aws/src/provider_generated.rs` with:

- Simple delete methods (single API call + identifier)
- No-op update methods (read-back for resources with all create-only attributes)
- Extract attribute methods (from SDK response types to `HashMap<String, Value>`)

It also generates `send_models_generated.rs`, which binds every writable schema
attribute to the hand-written create/update implementation, and
`resend_safety_generated.rs`, which binds AWS operations to resend decisions.

## Adding a New Resource

### Step 1: Add Smithy Model Download

If the service is new, add the model download to `scripts/download-smithy-models.sh`:

```bash
download "organizations" "organizations/service/2016-11-28/organizations-2016-11-28.json"
```

Models use the pinned revision and cache described below in **Smithy Model Cache**.

### Step 2: Add ResourceDef

Add a resource definition to `carina-codegen-aws/src/resource_defs.rs`:

```rust
pub fn organizations_resources() -> Vec<ResourceDef> {
    vec![
        ResourceDef {
            name: "organizations.organization",
            replacement: ReplacementClass::Conflicting,
            service_namespace: "com.amazonaws.organizations",
            create_op: "CreateOrganization",
            read_structure: Some("Organization"),
            delete_op: "DeleteOrganization",
            identifier: "Id",
            has_tags: false,
            // ... other fields
        },
    ]
}
```

Key `ResourceDef` fields:

| Field | Description |
|---|---|
| `name` | DSL resource name (e.g., `"ec2.vpc"`) |
| `replacement` | `UniqueName("SmithyMemberName")` takes the Smithy member name (matched case-insensitively via `snake_case`); codegen fails if the attribute is missing, read-only, or not a plain string. If `identity_overrides` is non-empty, it must contain exactly this unique-name attribute. Use `Coexisting` only if the replacement can coexist with the original under every combination of create-only changes; use `Conflicting` otherwise. |
| `service_namespace` | Smithy namespace (e.g., `"com.amazonaws.ec2"`) |
| `create_op` | Create operation name from Smithy model |
| `read_structure` | Smithy structure name for read state |
| `read_ops` | Multi-operation reads (when `read_structure` is None) |
| `delete_op` | Delete operation name |
| `update_ops` | Update operations with field layouts |
| `identifier` | Primary identifier field name |
| `has_tags` | Whether the resource supports tags |
| `simple_delete` | Whether delete is a single API call |
| `noop_update` | Whether update just reads back current state |
| `exclude_fields` | Fields to skip (e.g., `"DryRun"`, `"TagSpecifications"`) |
| `create_only_overrides` | Fields forced as create-only |
| `required_overrides` | Fields forced as required |
| `extra_read_only` | Additional read-only fields from read structure |
| `type_overrides` | Custom type mappings (e.g., CIDR → `ipv4_cidr()`) |

### Read and send dispositions

`ResourceDef` has two exhaustive contracts. `ReadDisposition` says how each
schema attribute is recovered from AWS (or why it is deliberately omitted).
`SendDisposition` says how every writable attribute reaches AWS:

- `Sent { via: CreateRequest, ... }` means the value is carried by the create
  request accepted by the cloud API. Only these fields are recorded in the
  protocol create attempt's `Sent` set.
- `Sent { via: FollowUp, ... }` means create consumes the value, but a later
  mutating request carries it (for example, EC2 attribute modification or S3
  bucket tagging).
- `NotSent { reason }` is reserved for a deliberate provider-side resolution,
  such as an attribute used only to construct an identifier. The reason is
  required and generation fails if it is empty. An unsupported attribute
  should normally be removed from the schema with `ReadDisposition::OmitSchema`
  and its own reason, rather than classified `NotSent`.

Generation fails when a writable attribute is unclassified, names an unknown
or read-only attribute, or has conflicting classifications. Field matching and
capture mappings use the same Smithy-to-snake-case normalization.

For every managed resource, codegen emits `XCreateInput` containing all `Sent`
attributes and `XUpdateInput` containing the non-create-only subset. Each input
decodes its source: create inputs use the desired `Resource`, while update
inputs use `UpdateResource`. Known shapes use typed values—including
`Vec<String>` and string-to-string tag maps—so malformed values fail at this
boundary instead of being silently ignored.

Hand-written code can open an input only through its generated named
`XCreateParts` or `XUpdateParts` struct. It must destructure every named field
without `..`; this makes swapped same-typed fields and newly generated fields
compile errors. Every part is a `Sent<T>` guard and must be consumed with
`into_inner()`. Under tests and debug assertions, dropping an unconsumed guard
panics, catching `_`, `drop(part)`, and unused-binding holes when that path is
exercised. A Drop guard can still be deliberately bypassed with
`mem::forget`/`ManuallyDrop`, and it cannot prove what happens after
`into_inner()`; the Winterbaume send-contract test covers that semantic gap. It
runs every generated create and update path and looks for the synthesized value
in the URI, query, or body of a mutating request. General headers and read calls
are excluded; modeled `@httpHeader` inputs require an explicit mapping and an
exact header-name/value match because those values have no URI/query/body form.

Optional update parts use `Sent<AttributeChange<T>>`, preserving `Set`, `Removed`,
and `Unchanged`; required update fields use `decode_required` and remain
`Sent<T>`. After `into_inner()`, the handler must handle `Removed` explicitly
through a match or a `resolve` callback and choose a reset value, a delete call,
or a typed `removal_error`. Removal must have the same effect as create-time
omission. If no reset is documented, return
`removal_error`.

### Step 3: Register in Codegen

Add to `carina-codegen-aws/src/main.rs`:

```rust
let mut all_resources = resource_defs::ec2_resources();
all_resources.extend(resource_defs::s3_resources());
all_resources.extend(resource_defs::sts_resources());
all_resources.extend(resource_defs::organizations_resources()); // Add this
```

### Step 4: Run Codegen

```bash
./scripts/generate-schemas-smithy.sh
./scripts/generate-provider.sh
```

### Step 5: Write Service Implementation

The codegen generates **schemas** and **provider boilerplate**, but the actual API call logic lives in `carina-provider-aws/src/services/`. Write the service implementation manually:

- `services/{service}/{resource}.rs` — read, create, update, delete methods
- `services/{service}/mod.rs` — module declaration

Service code reaches AWS only through the generated wrapper clients in
`resend_safety_generated.rs` (`Ec2Client`, `S3Client`, ...). A wrapper has a
method only for operations that have a resend decision. To call a new AWS
operation:

1. Add its decision to `OVERRIDES` in `carina-codegen-aws/src/resend_safety.rs`.
   Codegen checks the decision against the Smithy `@readonly`, `@idempotent`
   and `@idempotencyToken` traits, and fails on a contradiction.
2. Run `./scripts/generate-provider.sh`. It regenerates
   `resend_safety_generated.rs`.
3. Call the generated method. Operations without a token return the SDK
   builder, as before. Token-bearing operations return a `TokenOperation`.
   Set its request fields with `.configure(|input| ...)` and send it with
   `retry_aws_operation_with_token` or `send_aws_operation_with_token`. These
   apply the invocation's one `ClientToken`; a `TokenOperation` has no `send`
   of its own.

Calling an operation that has no decision fails to compile, because the
wrapper has no method for it. A raw `aws_sdk_*::Client` cannot be created at
all: the provider has no `aws-sdk-*` or `aws-config` dependency, and
`carina-aws-types` exports only each SDK's non-client modules plus guarded
handles whose sole constructor installs `ResendGuard`.

Each facade service requires its same-named `carina-aws-types` feature; none are
enabled by default. When adding a service, enable its feature in the provider's
normal dependency. Tests inherit those features; the dev-dependency adds only
`test-utils`. Codegen itself uses shared types without service features. Feature
selection never provides an alternative client constructor.

### Step 6: Add Provider Dispatch

Add dispatch entries to `carina-provider-aws/src/provider.rs` for read, create, update, and delete.

### Step 7: Verify

```bash
cargo test -p carina-provider-aws
cargo clippy --workspace --all-targets -- -D warnings
```

## Generated File Structure

```
carina-provider-aws/src/
├── schemas/
│   └── generated/           # Auto-generated by generate-schemas-smithy.sh
│       ├── mod.rs            # configs(), enum lookups
│       ├── ec2/
│       │   ├── mod.rs
│       │   ├── vpc.rs
│       │   └── ...
│       ├── organizations/
│       │   ├── mod.rs
│       │   ├── organization.rs
│       │   └── account.rs
│       └── ...
├── provider_generated.rs     # Auto-generated by generate-provider.sh
├── send_models_generated.rs  # Auto-generated by generate-provider.sh (typed create/update contract)
├── resend_safety_generated.rs # Auto-generated by generate-provider.sh (wrapper clients, resend decisions)
└── services/                 # Hand-written service implementations
    ├── ec2/
    ├── organizations/
    └── ...
```

## Crate Structure

- **carina-codegen-aws** — Codegen binary (`smithy-codegen`) and resource definitions
- **carina-docgen** — Shared registry-compatible Markdown page builder and checker
- **carina-smithy** — Smithy 2.0 JSON AST parser

## AWSCC Identity Manifest

The [identity inventory guard](./carina-codegen-aws/tests/support/identity_inventory_guard.rs)
uses the checked-in [awscc manifest](./carina-codegen-aws/tests/fixtures/awscc-identified-inputs.json).
Refresh it whenever awscc changes identified inputs or bumps its carina-aws-types
dependency, and whenever this repository bumps carina-aws-types.

From this repository's root, with a clean awscc checkout at
`../carina-provider-awscc`, run:

```bash
CARGO_NET_OFFLINE=true ./scripts/update-awscc-identity-manifest.py ../carina-provider-awscc
```

When working in a worktree, pass the awscc checkout's absolute path instead.
The [updater](./scripts/update-awscc-identity-manifest.py) compiles local schema
modules offline into this checkout's target directory. It requires the recorded
commit to be reachable from the locally available awscc `origin/main` ref and
does not fetch refs. Use `--allow-unpublished` only for an intentional local
snapshot of unpublished work. Review the refreshed manifest and update its
explicit commit/count assertions when the source inventory changes.

The manifest records upstream byte hashes as provenance in
`source.upstream_file_sha256`. Its `source.files` hashes cover each file's
checked-in inventory entries, referenced identity definitions, and upstream
digest. `source.content_sha256` covers the entire manifest except itself.
Both use SHA-256 over compact, recursively key-sorted UTF-8 JSON. The guard
recomputes these inventory checksums, so hand edits and partial refreshes fail
without a sibling checkout or network access. Ordinary CI never runs the
refresh command; its updater tests use offline Git and Cargo stubs.

## Smithy Model Cache

[`smithy-models.rev`](./smithy-models.rev) is the sole source of the full
40-character commit SHA of `aws/api-models-aws`. Downloads use that commit,
and [`scripts/smithy-models-dir.sh`](./scripts/smithy-models-dir.sh) resolves
`carina-provider-aws/tests/fixtures/smithy/<rev>/` for generators and Rust tests.
This cache is gitignored; a changed pin selects a separate directory and never
reads the old flat cache. Delete the resolved directory to force re-download.

To bump the pin, edit `smithy-models.rev`, then run:

```bash
./scripts/generate-schemas-smithy.sh
./scripts/generate-provider.sh
./scripts/generate-docs.sh
```

Review the schemas, provider code, generated docs, and test goldens together.
Commit the pin and all tracked schema, doc, and golden updates in one PR and
list the model-driven changes. `generated-docs/` remains an uncommitted artifact.

## What Is and Isn't Generated

| Component | Generated? | Script |
|---|---|---|
| `schemas/generated/*.rs` | Yes | `generate-schemas-smithy.sh` |
| `provider_generated.rs` | Yes | `generate-provider.sh` |
| `send_models_generated.rs` (typed create/update inputs and capture contract) | Yes | `generate-provider.sh` |
| `resend_safety_generated.rs` (wrapper clients, resend decisions) | Yes | `generate-provider.sh` |
| `clippy.toml` (workspace root) | No | Hand-written |
| `services/*.rs` | No | Hand-written |
| `provider.rs` dispatch | No | Hand-written |
| Client construction | Yes, as `from_bounded_config` on each wrapper | `generate-provider.sh` |
| `tests.rs` | No | Hand-written |
