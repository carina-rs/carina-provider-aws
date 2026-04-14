# DataSourceDef: Codegen Support for Data Sources

## Goal

Enable codegen-generated schemas, docs, and dispatch for data sources (read-only resources with user-supplied lookup inputs), eliminating the need for hand-written schema modules.

## Chosen Approach: C — Separate `DataSourceDef` struct

A new `DataSourceDef` struct models data sources as a first-class concept, distinct from managed resources (`ResourceDef`). This avoids perpetuating the ad-hoc pattern of `ResourceDef` with empty `create_op`.

### Rationale

- `ResourceDef` was designed for CRUD lifecycle resources. Using empty `create_op` to mean "data source" is an implicit convention, not a type-level guarantee.
- A separate struct gives data sources their own purpose-built fields (lookup inputs, no create/update/delete noise).
- Shared codegen logic (Smithy type resolution, attribute generation, docs formatting) is extracted into helpers used by both paths.

## Design

### `DataSourceDef` struct

```rust
pub struct DataSourceDef {
    pub name: &'static str,                    // "identitystore.user"
    pub service_namespace: &'static str,       // "com.amazonaws.identitystore"
    pub inputs: Vec<DataSourceInput>,          // User-supplied lookup fields
    pub read_ops: Vec<ReadOp>,                 // API operations for output fields
    pub type_overrides: Vec<(&'static str, &'static str)>,
    pub exclude_fields: Vec<&'static str>,
}

pub struct DataSourceInput {
    pub name: &'static str,                    // "user_name"
    pub provider_name: &'static str,           // "UserName"
    pub description: &'static str,
    pub required: bool,
    pub type_override: Option<&'static str>,   // e.g., "AttributeType::String"
}
```

### Migration targets

- `sts.caller_identity`: move from `sts_resources()` to `sts_data_sources()`. Zero inputs, one `ReadOp`.
- `identitystore.user`: new `DataSourceDef` entry with 3 inputs. Remove hand-written `schemas/generated/identitystore/user.rs`.

### Codegen changes

The codegen binary (`smithy-codegen`) currently processes `Vec<ResourceDef>`. It will additionally accept `Vec<DataSourceDef>` and generate:

1. **Schema** (`.rs`): `ResourceSchema::new(...).as_data_source()` with input attributes (writable) and output attributes (read-only from `read_ops`)
2. **Docs** (`.md`): Markdown with "Lookup Inputs" and "Attributes" sections
3. **Dispatch**: NOT generated — lookup logic is hand-written in `services/{service}/{resource}.rs`

### Shared helpers

Extract from existing codegen into reusable functions:
- `resolve_smithy_type()` — Smithy shape → Carina AttributeType string
- `generate_attribute_line()` — attribute schema code generation
- `generate_markdown_attribute()` — docs attribute formatting
- `snake_case_name()` — PascalCase → snake_case conversion

### What stays hand-written

- `services/identitystore/user.rs` — the lookup implementation (GetUserId → DescribeUser chain)
- `provider.rs` — the `read_data_source` dispatch to hand-written methods

### File changes

| File | Change |
|------|--------|
| `resource_defs.rs` | Add `DataSourceDef`, `DataSourceInput`. Add `sts_data_sources()`, `identitystore_data_sources()`. Remove `sts.caller_identity` from `sts_resources()`. |
| `main.rs` | Accept `Vec<DataSourceDef>`. Generate schema/docs for data sources. Add to `cf_type_name()`. |
| `schemas/generated/identitystore/user.rs` | Replaced by codegen output |
| `schemas/generated/sts/caller_identity.rs` | Replaced by codegen output (may differ slightly) |

## Edge Cases

- `sts.caller_identity` has no inputs and no Smithy read structure — uses `read_ops` only. The data source codegen must handle zero-input case.
- `identitystore.user` inputs come from different Smithy operations than the outputs. Inputs may need `type_override` when the Smithy type can't be auto-resolved.
- Download of `identitystore` Smithy model needed for `identitystore.user` codegen.
