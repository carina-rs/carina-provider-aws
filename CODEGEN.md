# Schema Code Generation

This document explains how to generate and maintain resource schemas from AWS Smithy models.

## Overview

The AWS provider uses two Rust codegen outputs:

1. **Schema definitions** (`schemas/generated/`) — resource attribute schemas generated from Smithy models
2. **Provider boilerplate** (`provider_generated.rs`) — delete/update method stubs generated from `ResourceDef` metadata

Both are produced by the `carina-codegen-aws` crate.

Provider reference Markdown is also generated from the same Smithy models with
`./scripts/generate-docs.sh`. `carina-docgen` allocates registry-compatible
heading ids as each heading is written, resolves typed link targets when the
page is finished, and validates the finished page. Generation fails on missing
targets, dead local fragments, Markdown links hidden inside code spans, or
duplicated enum/struct section headings. Codegen also inserts examples before
validation; the script only adds frontmatter. The resulting `generated-docs/`
tree is a transient build artifact and is not committed. The docs-bundle CI job
also runs `carina-docgen-check` on the built tarball, after the bundle has
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

## Adding a New Resource

### Step 1: Add Smithy Model Download

If the service is new, add the model download to `scripts/download-smithy-models.sh`:

```bash
download "organizations" "organizations/service/2016-11-28/organizations-2016-11-28.json"
```

Models are cached in `carina-provider-aws/tests/fixtures/smithy/` and gitignored.

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
| `replacement` | `UniqueName("SmithyMemberName")` takes the Smithy member name (matched case-insensitively via `snake_case`); codegen fails if the attribute is missing, read-only, not a plain string, or the resource has a composite identity (`identity_overrides`). Use `Coexisting` only if the replacement can coexist with the original under every combination of create-only changes; use `Conflicting` otherwise. |
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
   `resend_safety_generated.rs` and the workspace `clippy.toml`.
3. Call the generated method. Operations without a token return the SDK
   builder, as before. Token-bearing operations return a `TokenOperation`.
   Set its request fields with `.configure(|input| ...)` and send it with
   `retry_aws_operation_with_token` or `send_aws_operation_with_token`. These
   apply the invocation's one `ClientToken`; a `TokenOperation` has no `send`
   of its own.

Calling an operation that has no decision fails to compile, because the
wrapper has no method for it. Creating a raw `aws_sdk_*::Client` is rejected
by the generated `clippy.toml`.

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

## Smithy Model Cache

Downloaded models are cached in `carina-provider-aws/tests/fixtures/smithy/` with filenames like `ec2.json`, `organizations.json`. This directory is gitignored. Delete cached files to force re-download.

## What Is and Isn't Generated

| Component | Generated? | Script |
|---|---|---|
| `schemas/generated/*.rs` | Yes | `generate-schemas-smithy.sh` |
| `provider_generated.rs` | Yes | `generate-provider.sh` |
| `resend_safety_generated.rs` (wrapper clients, resend decisions) | Yes | `generate-provider.sh` |
| `clippy.toml` (workspace root) | Yes | `generate-provider.sh` |
| `services/*.rs` | No | Hand-written |
| `provider.rs` dispatch | No | Hand-written |
| Client construction | Yes, as `from_bounded_config` on each wrapper | `generate-provider.sh` |
| `tests.rs` | No | Hand-written |
