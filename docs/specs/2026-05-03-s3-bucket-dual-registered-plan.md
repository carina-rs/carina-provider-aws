# `aws.s3.Bucket` Dual-Registered: Implementation Plan

<!-- derived-from ./2026-05-03-s3-bucket-dual-registered-design.md -->

Decomposes the design into TDD-ready tasks. Each task is one Red→Green cycle: write/extend a test that fails, then write the minimal code to make it pass. Tasks are ordered so that each task's prerequisites are already in place.

Worktree: `/Users/mizzy/src/github.com/carina-rs/carina-provider-aws/.worktrees/issue-163-s3-dual-registered`

Test runner: `cargo nextest run` (project default; `cargo test` works too).
Lint: `cargo clippy --workspace --all-targets -- -D warnings`.

## File map

| Path | Created/Modified | Phase |
|---|---|---|
| `carina-codegen-aws/src/resource_defs.rs` | M | 1, 2, 4 |
| `carina-codegen-aws/src/main.rs` | M | 3, 5 |
| `carina-provider-aws/src/schemas/generated/sts/caller_identity.rs` | M (regen) | 3 |
| `carina-provider-aws/src/schemas/generated/identitystore/user.rs` | M (regen) | 3 |
| `carina-provider-aws/src/schemas/generated/s3/bucket_data_source.rs` | C (regen) | 4 |
| `carina-provider-aws/src/schemas/generated/s3/mod.rs` | M (regen) | 4 |
| `carina-provider-aws/src/schemas/generated/mod.rs` | M (regen) | 4 |
| `carina-provider-aws/src/provider_generated.rs` | M (regen) | 5 |
| `carina-provider-aws/src/provider.rs` | M | 6 |
| `carina-provider-aws/src/services/sts/caller_identity.rs` | M | 6 |
| `carina-provider-aws/src/services/identitystore/user.rs` | M | 6 |
| `carina-provider-aws/src/services/s3/bucket.rs` | M | 7 |
| `acceptance-tests/s3_bucket_data_source/basic.crn.template` | C | 8 |
| `acceptance-tests/s3_bucket_data_source/run.sh` | C | 8 |
| `acceptance-tests/s3_bucket_data_source/tests/` | C | 8 |

## Phases

- **Phase 1**: Add `DataSourceOutput` type and `output_attributes` field (no behaviour change yet — existing `read_ops` path still produces schemas).
- **Phase 2**: Migrate existing DataSourceDefs (`sts.CallerIdentity`, `identitystore.User`) to populate `output_attributes`. Schema output stays byte-identical.
- **Phase 3**: Switch `generate_data_source` to consume `output_attributes`. Delete `read_ops`-driven inference.
- **Phase 4**: Add `s3.Bucket` DataSourceDef + regenerate schemas.
- **Phase 5**: Codegen `DataSourceLookups` trait + dispatcher into `provider_generated.rs`.
- **Phase 6**: Move existing `read_*` data source methods into `impl DataSourceLookups`. Delete hand-written `Provider::read_data_source`.
- **Phase 7**: Implement `read_s3_bucket_data_source` + `s3_hosted_zone_id` table.
- **Phase 8**: Acceptance test.

---

## Phase 1: `DataSourceOutput` type

### Task 1.1: Add `DataSourceOutput` struct

**Goal**: Introduce the type that will represent a declared output attribute on a DataSource. No callers yet.

**Files**: `carina-codegen-aws/src/resource_defs.rs`

**Test** (in `resource_defs.rs` `#[cfg(test)] mod tests`):
```rust
#[test]
fn data_source_output_field_round_trip() {
    let o = DataSourceOutput {
        name: "arn",
        provider_name: None,
        description: "Bucket ARN",
        type_code: "AttributeType::String",
    };
    assert_eq!(o.name, "arn");
    assert!(o.provider_name.is_none());
    assert_eq!(o.type_code, "AttributeType::String");
}
```

**Implementation**:
```rust
/// One declared output attribute on a `DataSourceDef`.
///
/// `provider_name = None` means the value is computed (e.g. ARN built from
/// inputs); `provider_name = Some("...")` means the value comes from a
/// `read_ops` API field of that name.
pub struct DataSourceOutput {
    pub name: &'static str,
    pub provider_name: Option<&'static str>,
    pub description: &'static str,
    /// Rust type expression for codegen, e.g. `"AttributeType::String"` or
    /// `"super::aws_account_id()"`. Required: codegen does not infer.
    pub type_code: &'static str,
}
```

**Verification**: `cargo test -p carina-codegen-aws data_source_output_field_round_trip`

---

### Task 1.2: Add `output_attributes` field to `DataSourceDef`

**Goal**: Extend `DataSourceDef` with the new field. Existing call sites get `output_attributes: vec![]` (empty) so this is non-breaking until Phase 2.

**Files**: `carina-codegen-aws/src/resource_defs.rs`

**Test**:
```rust
#[test]
fn data_source_def_carries_output_attributes() {
    let def = DataSourceDef {
        name: "test.X",
        service_namespace: "com.test",
        inputs: vec![],
        output_attributes: vec![DataSourceOutput {
            name: "arn",
            provider_name: None,
            description: "",
            type_code: "AttributeType::String",
        }],
        read_ops: vec![],
        type_overrides: vec![],
        exclude_fields: vec![],
    };
    assert_eq!(def.output_attributes.len(), 1);
    assert_eq!(def.output_attributes[0].name, "arn");
}
```

**Implementation**: Add field on the existing `DataSourceDef` struct:
```rust
pub struct DataSourceDef {
    pub name: &'static str,
    pub service_namespace: &'static str,
    pub inputs: Vec<DataSourceInput>,
    pub output_attributes: Vec<DataSourceOutput>,
    pub read_ops: Vec<ReadOp>,
    pub type_overrides: Vec<(&'static str, &'static str)>,
    pub exclude_fields: Vec<&'static str>,
}
```

Update existing literals (`sts_data_sources()`, `identitystore_data_sources()`) by inserting `output_attributes: vec![],` so the project still compiles.

**Verification**: `cargo build -p carina-codegen-aws` AND `cargo test -p carina-codegen-aws data_source_def_carries_output_attributes`

---

## Phase 2: Migrate existing DataSourceDefs to `output_attributes`

### Task 2.1: Populate `output_attributes` on `sts.CallerIdentity`

**Goal**: Move the three fields (account_id, arn, user_id) from `read_ops`-inferred to explicit `output_attributes`. `read_ops` and `type_overrides` stay as-is for now (used by markdown docs and consumed by Phase-3-removed inference; both remain valid until then).

**Files**: `carina-codegen-aws/src/resource_defs.rs::sts_data_sources()`

**Test**: A unit test against `sts_data_sources()` confirming the three outputs are present with the right shape:
```rust
#[test]
fn sts_caller_identity_declares_outputs_explicitly() {
    let defs = sts_data_sources();
    let ds = &defs[0];
    let names: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
    assert_eq!(names, vec!["account_id", "arn", "user_id"]);
    let account_id = &ds.output_attributes[0];
    assert_eq!(account_id.provider_name, Some("Account"));
    assert_eq!(account_id.type_code, "super::aws_account_id()");
    let arn = &ds.output_attributes[1];
    assert_eq!(arn.type_code, "super::arn()");
}
```

**Implementation**:
```rust
pub fn sts_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "sts.CallerIdentity",
        service_namespace: "com.amazonaws.sts",
        inputs: vec![],
        output_attributes: vec![
            DataSourceOutput {
                name: "account_id",
                provider_name: Some("Account"),
                description: "The Amazon Web Services account ID number of the account that owns or contains the calling entity.",
                type_code: "super::aws_account_id()",
            },
            DataSourceOutput {
                name: "arn",
                provider_name: Some("Arn"),
                description: "The Amazon Web Services ARN associated with the calling entity.",
                type_code: "super::arn()",
            },
            DataSourceOutput {
                name: "user_id",
                provider_name: Some("UserId"),
                description: "The unique identifier of the calling entity.",
                type_code: "AttributeType::String",
            },
        ],
        read_ops: vec![ReadOp {
            operation: "GetCallerIdentity",
            fields: vec![("Account", Some("AccountId")), ("Arn", None), ("UserId", None)],
            defaults: vec![],
        }],
        type_overrides: vec![
            ("AccountId", "super::aws_account_id()"),
            ("Arn", "super::arn()"),
        ],
        exclude_fields: vec![],
    }]
}
```

**Verification**: `cargo test -p carina-codegen-aws sts_caller_identity_declares_outputs_explicitly`

---

### Task 2.2: Populate `output_attributes` on `identitystore.User`

**Goal**: Same migration as 2.1, for the User data source. Inputs (identity_store_id, user_id, user_name) stay in `inputs`; outputs (display_name, emails) go into `output_attributes`.

**Files**: `carina-codegen-aws/src/resource_defs.rs::identitystore_data_sources()`

**Test**:
```rust
#[test]
fn identitystore_user_declares_outputs_explicitly() {
    let defs = identitystore_data_sources();
    let ds = &defs[0];
    let names: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
    assert_eq!(names, vec!["display_name", "emails"]);
    assert_eq!(ds.output_attributes[0].provider_name, Some("DisplayName"));
    assert_eq!(ds.output_attributes[1].provider_name, Some("Emails"));
    // Inputs stay where they are
    assert_eq!(ds.inputs.len(), 3);
}
```

**Implementation**:
```rust
pub fn identitystore_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "identitystore.User",
        service_namespace: "com.amazonaws.identitystore",
        inputs: vec![
            DataSourceInput { name: "identity_store_id", provider_name: "IdentityStoreId",
                description: "The globally unique identifier for the identity store.",
                required: true, type_override: None },
            DataSourceInput { name: "user_id", provider_name: "UserId",
                description: "The identifier for the user. Provide either user_id or user_name.",
                required: false, type_override: None },
            DataSourceInput { name: "user_name", provider_name: "UserName",
                description: "The user's user name. Provide either user_id or user_name.",
                required: false, type_override: None },
        ],
        output_attributes: vec![
            DataSourceOutput {
                name: "display_name",
                provider_name: Some("DisplayName"),
                description: "Display name of the user.",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "emails",
                provider_name: Some("Emails"),
                description: "Email addresses associated with the user.",
                type_code: "AttributeType::String",
            },
        ],
        read_ops: vec![ReadOp {
            operation: "DescribeUser",
            fields: vec![("DisplayName", None), ("Emails", None)],
            defaults: vec![],
        }],
        type_overrides: vec![],
        exclude_fields: vec![],
    }]
}
```

**Verification**: `cargo test -p carina-codegen-aws identitystore_user_declares_outputs_explicitly`

---

## Phase 3: Switch codegen to `output_attributes`

### Task 3.1: Add codegen test pinning sts.CallerIdentity schema output

**Goal**: Lock down the current generated text for `sts_caller_identity_config()` so Phase 3 can refactor `generate_data_source` without behaviour drift.

**Files**: `carina-codegen-aws/src/main.rs::tests`

**Test**: Snapshot-equivalent (string equality) test:
```rust
#[test]
fn generate_data_source_for_sts_caller_identity_matches_expected_shape() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../carina-provider-aws/tests/fixtures/smithy/sts.json");
    if !fixture.exists() {
        eprintln!("Skipping: Smithy fixture not found: {}", fixture.display());
        return;
    }
    let file = std::fs::File::open(&fixture).expect("open Smithy fixture");
    let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).expect("parse");
    let ds = resource_defs::sts_data_sources().into_iter().next().unwrap();

    let generated = generate_data_source(&ds, &model).expect("generate_data_source");

    // Pin the structural promises rather than the full string (which has
    // long descriptions). These are the contracts that must survive the
    // Phase-3 rewrite to `output_attributes`.
    assert!(generated.contains(".as_data_source()"), "must mark as data source: {generated}");
    assert!(generated.contains(r#"AttributeSchema::new("account_id", super::aws_account_id())"#),
        "account_id must use aws_account_id(): {generated}");
    assert!(generated.contains(r#"AttributeSchema::new("arn", super::arn())"#),
        "arn must use arn(): {generated}");
    assert!(generated.contains(r#"AttributeSchema::new("user_id", AttributeType::String)"#),
        "user_id must be String: {generated}");
    assert!(generated.contains(".with_provider_name(\"Account\")"),
        "account_id keeps Account provider_name: {generated}");
}
```

**Implementation**: None — this just pins existing behaviour. The current `generate_data_source` reads `read_ops` and produces this output; the test must pass NOW (before Phase 3.2).

**Verification**: `cargo test -p carina-codegen-aws generate_data_source_for_sts_caller_identity_matches_expected_shape`

If this test fails today, it points to a discrepancy that Phase 3.2 must preserve.

---

### Task 3.2: Rewrite `generate_data_source` to consume `output_attributes`

**Goal**: Make `generate_data_source` build the read-only attribute list from `ds.output_attributes` instead of walking `ds.read_ops` and resolving Smithy types. The pinning test from 3.1 must keep passing.

**Files**: `carina-codegen-aws/src/main.rs`

**Test**: The Phase-3.1 pinning test continues to pass. Add one more positive case for `identitystore.User` to exercise the inputs-and-outputs combined path:
```rust
#[test]
fn generate_data_source_for_identitystore_user_emits_inputs_and_outputs() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../carina-provider-aws/tests/fixtures/smithy/identitystore.json");
    if !fixture.exists() { return; }
    let file = std::fs::File::open(&fixture).unwrap();
    let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
    let ds = resource_defs::identitystore_data_sources().into_iter().next().unwrap();

    let generated = generate_data_source(&ds, &model).expect("generate_data_source");

    // Inputs are writable
    assert!(generated.contains(r#"AttributeSchema::new("identity_store_id", AttributeType::String)"#));
    assert!(generated.contains(".required()"), "identity_store_id is required");
    // Outputs are read-only
    assert!(generated.contains(r#"AttributeSchema::new("display_name", AttributeType::String)"#));
    assert!(generated.contains(r#"AttributeSchema::new("emails", AttributeType::String)"#));
}
```

**Implementation**: In `generate_data_source`, replace the loop that iterates `ds.read_ops` and consults `model.operation_output(...)` with a loop over `ds.output_attributes`. The output attribute path becomes a straight emission:

```rust
for output in &ds.output_attributes {
    ds_attrs.push(DsAttr {
        name: output.name.to_string(),
        provider_name: output.provider_name.unwrap_or("").to_string(),
        type_str: output.type_code.to_string(),
        description: output.description.to_string(),
        required: false,
        read_only: true,
    });
}
```

The inputs loop stays. The Smithy `model.operation_output(...)` lookup for outputs is removed (the output side no longer needs the model). Markdown generation (`generate_markdown_data_source`) is updated in Task 3.3.

The argument `model: &SmithyModel` becomes unused for the output side but stays in the signature because inputs may still consult it via `is_email_property`. Keep the argument.

**Verification**: `cargo test -p carina-codegen-aws generate_data_source_for_sts_caller_identity_matches_expected_shape generate_data_source_for_identitystore_user_emits_inputs_and_outputs`

---

### Task 3.3: Update `generate_markdown_data_source` to read `output_attributes`

**Goal**: Markdown docs follow the same source of truth.

**Files**: `carina-codegen-aws/src/main.rs::generate_markdown_data_source`

**Test**:
```rust
#[test]
fn markdown_data_source_lists_explicit_output_attributes() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../carina-provider-aws/tests/fixtures/smithy/sts.json");
    if !fixture.exists() { return; }
    let file = std::fs::File::open(&fixture).unwrap();
    let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
    let ds = resource_defs::sts_data_sources().into_iter().next().unwrap();

    let md = generate_markdown_data_source(&ds, &model).expect("md");

    assert!(md.contains("### `account_id`"), "{md}");
    assert!(md.contains("### `arn`"), "{md}");
    assert!(md.contains("### `user_id`"), "{md}");
}
```

**Implementation**: In `generate_markdown_data_source`, replace the read_ops walk with `for output in &ds.output_attributes` and emit the same `### \`<name>\`` heading + description block.

**Verification**: `cargo test -p carina-codegen-aws markdown_data_source_lists_explicit_output_attributes`

---

### Task 3.4: Regenerate sts/identitystore schemas + verify byte-equivalent output

**Goal**: Run the codegen scripts and confirm the resulting `.rs` files are unchanged (or only trivially changed in whitespace, which `cargo fmt` already normalises).

**Files**: `carina-provider-aws/src/schemas/generated/sts/caller_identity.rs`, `carina-provider-aws/src/schemas/generated/identitystore/user.rs` (regenerated outputs).

**Test**: A meta-test in `carina-codegen-aws/tests/smithy_models_test.rs` (or a new `tests/regen_stable_test.rs`) that calls `generate_data_source` for `sts.CallerIdentity` and `identitystore.User` and `assert_eq!`s the output to the on-disk file:
```rust
#[test]
fn sts_caller_identity_generated_file_matches_codegen() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../carina-provider-aws/tests/fixtures/smithy/sts.json");
    if !fixture.exists() { return; }
    let file = std::fs::File::open(&fixture).unwrap();
    let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
    let ds = carina_codegen_aws::resource_defs::sts_data_sources().into_iter().next().unwrap();

    let generated = carina_codegen_aws::generate_data_source(&ds, &model).expect("generate");
    let on_disk = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/src/schemas/generated/sts/caller_identity.rs"),
    ).expect("read on-disk");

    assert_eq!(generated, on_disk, "regenerate with scripts/generate-schemas-smithy.sh");
}
```

**Implementation**: Run `./scripts/generate-schemas-smithy.sh` (or directly the smithy-codegen binary) so the on-disk files are refreshed under the new output_attributes path. Then `cargo fmt`. The expected outcome is the on-disk files are byte-identical to before; if Task 3.2/3.3 has any subtle drift, this test surfaces it.

If `carina_codegen_aws` is not currently a `[lib]` consumed by the test crate, expose `resource_defs` and `generate_data_source` via `pub use` from a `lib.rs` (small refactor). Add to Cargo.toml:
```toml
[lib]
path = "src/lib.rs"
doctest = false
```
with `lib.rs` containing `pub mod resource_defs; pub use main::generate_data_source;`. Renames in `main.rs` may be required to make `generate_data_source` `pub`.

**Verification**:
- `cargo test -p carina-codegen-aws sts_caller_identity_generated_file_matches_codegen`
- `cargo build -p carina-provider-aws` (the regenerated files still compile)
- `cargo nextest run -p carina-provider-aws` (existing provider tests pass)

---

## Phase 4: Add `s3.Bucket` DataSourceDef

### Task 4.1: Add `s3_data_sources()` returning the new DataSourceDef

**Goal**: Declare the `s3.Bucket` DataSource in `resource_defs.rs` with all six output attributes.

**Files**: `carina-codegen-aws/src/resource_defs.rs`

**Test**:
```rust
#[test]
fn s3_data_sources_declares_s3_bucket() {
    let defs = s3_data_sources();
    assert_eq!(defs.len(), 1);
    let ds = &defs[0];
    assert_eq!(ds.name, "s3.Bucket");
    let inputs: Vec<&str> = ds.inputs.iter().map(|i| i.name).collect();
    assert_eq!(inputs, vec!["bucket"]);
    assert!(ds.inputs[0].required);
    let outputs: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
    assert_eq!(outputs, vec![
        "bucket", "arn", "region",
        "bucket_domain_name", "bucket_regional_domain_name", "hosted_zone_id"
    ]);
    // Computed fields have provider_name = None
    let arn = ds.output_attributes.iter().find(|o| o.name == "arn").unwrap();
    assert!(arn.provider_name.is_none());
    // region comes from the GetBucketLocation API
    let region = ds.output_attributes.iter().find(|o| o.name == "region").unwrap();
    assert_eq!(region.provider_name, Some("LocationConstraint"));
}
```

**Implementation**:
```rust
pub fn s3_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "s3.Bucket",
        service_namespace: "com.amazonaws.s3",
        inputs: vec![DataSourceInput {
            name: "bucket",
            provider_name: "Bucket",
            description: "Name of the S3 bucket to look up.",
            required: true,
            type_override: None,
        }],
        output_attributes: vec![
            DataSourceOutput { name: "bucket", provider_name: None,
                description: "The bucket name (echo of the input).",
                type_code: "AttributeType::String" },
            DataSourceOutput { name: "arn", provider_name: None,
                description: "ARN of the bucket.",
                type_code: "super::arn()" },
            DataSourceOutput { name: "region", provider_name: Some("LocationConstraint"),
                description: "AWS region the bucket is in.",
                type_code: "AttributeType::String" },
            DataSourceOutput { name: "bucket_domain_name", provider_name: None,
                description: "Bucket domain name (`<bucket>.s3.amazonaws.com`).",
                type_code: "AttributeType::String" },
            DataSourceOutput { name: "bucket_regional_domain_name", provider_name: None,
                description: "Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`).",
                type_code: "AttributeType::String" },
            DataSourceOutput { name: "hosted_zone_id", provider_name: None,
                description: "Route 53 Hosted Zone ID for the bucket's region.",
                type_code: "AttributeType::String" },
        ],
        read_ops: vec![
            ReadOp { operation: "HeadBucket", fields: vec![], defaults: vec![] },
            ReadOp { operation: "GetBucketLocation",
                fields: vec![("LocationConstraint", None)],
                defaults: vec![("LocationConstraint", "us-east-1")] },
        ],
        type_overrides: vec![],
        exclude_fields: vec![],
    }]
}
```

**Verification**: `cargo test -p carina-codegen-aws s3_data_sources_declares_s3_bucket`

---

### Task 4.2: Wire `s3_data_sources()` into the codegen entry point

**Goal**: Make `cargo run -p carina-codegen-aws` pick up `s3.Bucket` when generating data source schemas.

**Files**: `carina-codegen-aws/src/main.rs` (the section that builds `all_data_sources`)

**Test** (extension of Task 1 / 2 unit tests):
```rust
#[test]
fn all_data_sources_includes_s3_bucket() {
    let mut all = resource_defs::sts_data_sources();
    all.extend(resource_defs::identitystore_data_sources());
    all.extend(resource_defs::s3_data_sources());

    let names: Vec<&str> = all.iter().map(|d| d.name).collect();
    assert!(names.contains(&"s3.Bucket"));
}
```

**Implementation** (in `main.rs` around the existing `let mut all_data_sources = ...`):
```rust
let mut all_data_sources = resource_defs::sts_data_sources();
all_data_sources.extend(resource_defs::identitystore_data_sources());
all_data_sources.extend(resource_defs::s3_data_sources());
```

**Verification**: `cargo test -p carina-codegen-aws all_data_sources_includes_s3_bucket`

---

### Task 4.3: Generate `s3/bucket_data_source.rs` and wire into mod.rs/configs()

**Goal**: Produce the generated schema file for `s3.Bucket` DataSource and register it.

**Files**:
- `carina-provider-aws/src/schemas/generated/s3/bucket_data_source.rs` (new, codegen output)
- `carina-provider-aws/src/schemas/generated/s3/mod.rs` (add `pub mod bucket_data_source;`)
- `carina-provider-aws/src/schemas/generated/mod.rs` (add `s3::bucket_data_source::s3_bucket_data_source_config()` to `configs()` AND its `enum_valid_values()` / `enum_alias_*` calls to the corresponding lists)

**Test**: A unit test in `carina-provider-aws/src/schemas/generated/mod.rs` (or a new `tests.rs` adjacent file) confirming the registry contains both `s3.Bucket` Managed and DataSource:
```rust
#[test]
fn configs_register_s3_bucket_under_both_kinds() {
    use carina_core::schema::SchemaKind;
    let configs = configs();
    let managed = configs.iter().find(|c|
        c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::Managed
    );
    let data_source = configs.iter().find(|c|
        c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::DataSource
    );
    assert!(managed.is_some(), "Managed s3.Bucket missing from configs()");
    assert!(data_source.is_some(), "DataSource s3.Bucket missing from configs()");
}
```

**Implementation**: Run `./scripts/generate-schemas-smithy.sh`. The codegen should now emit `s3/bucket_data_source.rs` with `s3_bucket_data_source_config()`, plus updated `s3/mod.rs` and the top-level `schemas/generated/mod.rs`.

If codegen does not yet write to mod.rs entries for data sources beside their existing s3.Bucket Managed sibling, extend `generate_service_mod_files` and `generate_mod_rs::configs()` to include both. Cross-check by grepping the generated mod.rs after running codegen.

**Verification**:
- `cargo test -p carina-provider-aws configs_register_s3_bucket_under_both_kinds`
- `cargo build -p carina-provider-aws`

---

## Phase 5: Codegen `DataSourceLookups` trait + dispatcher

### Task 5.1: Make `generate_provider_code` accept data sources

**Goal**: Pass the data source list into the provider codegen entry point. Body unchanged for now (no use of the new arg).

**Files**: `carina-codegen-aws/src/main.rs` — `generate_provider_code` signature + the one caller (`"provider"` arm of the format match).

**Test**:
```rust
#[test]
fn generate_provider_code_accepts_data_sources_arg() {
    let resources: Vec<ResourceDef> = vec![];
    let data_sources: Vec<resource_defs::DataSourceDef> = resource_defs::sts_data_sources();
    let models = HashMap::new();
    let manual: std::collections::HashSet<String> = std::collections::HashSet::new();
    let code = generate_provider_code(&resources, &data_sources, &models, &manual);
    assert!(code.contains("Auto-generated provider boilerplate"));
}
```

**Implementation**:
```rust
fn generate_provider_code(
    all_resources: &[ResourceDef],
    all_data_sources: &[resource_defs::DataSourceDef],
    models: &HashMap<&str, SmithyModel>,
    manual_methods: &std::collections::HashSet<String>,
) -> String {
    /* same body as before; ignore all_data_sources for now */
}
```

Caller in the `"provider"` arm:
```rust
let code = generate_provider_code(&all_resources, &all_data_sources, &models, &manual_methods);
```

**Verification**: `cargo test -p carina-codegen-aws generate_provider_code_accepts_data_sources_arg`

---

### Task 5.2: Emit `DataSourceLookups` trait declaration

**Goal**: Generate the trait into `provider_generated.rs`. No `match` dispatcher yet.

**Files**: `carina-codegen-aws/src/main.rs::generate_provider_code`

**Test**:
```rust
#[test]
fn generate_provider_code_emits_data_source_lookups_trait() {
    let resources: Vec<ResourceDef> = vec![];
    let mut data_sources = resource_defs::sts_data_sources();
    data_sources.extend(resource_defs::identitystore_data_sources());
    let models = HashMap::new();
    let manual = std::collections::HashSet::new();
    let code = generate_provider_code(&resources, &data_sources, &models, &manual);

    assert!(code.contains("pub trait DataSourceLookups"));
    assert!(code.contains("fn read_sts_caller_identity_data_source("));
    assert!(code.contains("fn read_identitystore_user_data_source("));
    assert!(code.contains("BoxFuture<'_, ProviderResult<State>>"));
}
```

**Implementation**: Append to `code` near the end of `generate_provider_code`:
```rust
code.push_str("// ===== Generated DataSourceLookups Trait =====\n\n");
code.push_str("pub trait DataSourceLookups {\n");
for ds in all_data_sources {
    let module = module_name(ds.name);
    code.push_str(&format!(
        "\x20   fn read_{}_data_source(\n\
         \x20       &self,\n\
         \x20       resource: &Resource,\n\
         \x20   ) -> BoxFuture<'_, ProviderResult<State>>;\n\n",
        module,
    ));
}
code.push_str("}\n\n");
```

`module_name` exists in main.rs already (used by `generate_data_source` to produce e.g. `sts_caller_identity` from `sts.CallerIdentity`).

Also add `use carina_core::provider::BoxFuture;` to the file header section in `generate_provider_code` (alongside the existing `use` lines).

**Verification**: `cargo test -p carina-codegen-aws generate_provider_code_emits_data_source_lookups_trait`

---

### Task 5.3: Emit `read_data_source` dispatcher

**Goal**: Generate the dispatcher `match` so `Provider::read_data_source` routes to trait methods.

**Files**: `carina-codegen-aws/src/main.rs::generate_provider_code`

**Test**:
```rust
#[test]
fn generate_provider_code_emits_read_data_source_dispatcher() {
    let resources: Vec<ResourceDef> = vec![];
    let mut data_sources = resource_defs::sts_data_sources();
    data_sources.extend(resource_defs::identitystore_data_sources());
    let models = HashMap::new();
    let manual = std::collections::HashSet::new();
    let code = generate_provider_code(&resources, &data_sources, &models, &manual);

    assert!(code.contains("pub(crate) fn dispatch_read_data_source(provider: &AwsProvider, resource: &Resource)"));
    assert!(code.contains("\"sts.CallerIdentity\" => provider.read_sts_caller_identity_data_source(resource),"));
    assert!(code.contains("\"identitystore.User\" => provider.read_identitystore_user_data_source(resource),"));
    // Default arm preserves the carina-core safety rail for unknown types
    assert!(code.contains("aws provider does not implement read_data_source for"));
}
```

**Implementation**: Append:
```rust
code.push_str("// ===== Generated read_data_source dispatcher =====\n\n");
code.push_str(
    "pub(crate) fn dispatch_read_data_source(\n\
     \x20   provider: &AwsProvider,\n\
     \x20   resource: &Resource,\n\
     ) -> BoxFuture<'_, ProviderResult<State>> {\n\
     \x20   match resource.id.resource_type.as_str() {\n",
);
for ds in all_data_sources {
    let module = module_name(ds.name);
    code.push_str(&format!(
        "\x20       \"{}\" => provider.read_{}_data_source(resource),\n",
        ds.name, module,
    ));
}
code.push_str(
    "\x20       _ => {\n\
     \x20           let id = resource.id.clone();\n\
     \x20           let resource_type = resource.id.resource_type.clone();\n\
     \x20           Box::pin(async move {\n\
     \x20               Err(ProviderError::new(format!(\n\
     \x20                   \"aws provider does not implement read_data_source for '{}'\",\n\
     \x20                   resource_type\n\
     \x20               )).for_resource(id))\n\
     \x20           })\n\
     \x20       }\n\
     \x20   }\n\
     }\n\n",
);
```

Note the dispatcher is a free function `dispatch_read_data_source(provider, resource)`, not a method on `AwsProvider`, to keep `provider.rs::Provider::read_data_source` minimally coupled to codegen output.

**Verification**: `cargo test -p carina-codegen-aws generate_provider_code_emits_read_data_source_dispatcher`

---

### Task 5.4: Regenerate `provider_generated.rs`

**Goal**: Refresh the on-disk file. Crate should NOT compile yet because services don't `impl DataSourceLookups`. That's expected — Phase 6 fixes it.

**Files**: `carina-provider-aws/src/provider_generated.rs`

**Test**: Confirm the generated file exists and contains the trait + dispatcher. A grep-based test in `carina-provider-aws/src/provider_generated.rs` itself isn't suitable (it's regenerated). Instead add an `assert!` test in `carina-provider-aws/tests/provider_generated_smoke_test.rs`:
```rust
#[test]
fn provider_generated_contains_data_source_lookups_trait() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/provider_generated.rs");
    let content = std::fs::read_to_string(&path).expect("read provider_generated.rs");
    assert!(content.contains("pub trait DataSourceLookups"));
    assert!(content.contains("fn dispatch_read_data_source"));
}
```

**Implementation**: Run `./scripts/generate-provider.sh`. The on-disk `provider_generated.rs` now contains the trait and dispatcher. The compile failure from missing impls is the expected red bar before Phase 6.

**Verification**:
- `cargo test -p carina-provider-aws provider_generated_contains_data_source_lookups_trait` (this passes — it's a text test, doesn't need linking)
- `cargo build -p carina-provider-aws` is **expected to fail** at the end of this task. This is the red state Phase 6 transitions to green.

---

## Phase 6: Migrate existing data source impls to `DataSourceLookups`

### Task 6.1: Move `sts.CallerIdentity` impl into `DataSourceLookups`

**Goal**: Replace the inherent `read_sts_caller_identity` method with a trait impl matching the codegen-generated trait method name.

**Files**: `carina-provider-aws/src/services/sts/caller_identity.rs`

**Test**: Compile + an existing test in services/sts (or add one if absent) confirming the trait method works against a stubbed/mock client. Use `#[ignore]` if AWS creds are needed; the compile is the primary signal.

```rust
#[test]
fn aws_provider_implements_data_source_lookups_for_sts() {
    fn assert_impl<T: crate::provider_generated::DataSourceLookups>() {}
    assert_impl::<crate::AwsProvider>();
}
```

**Implementation**: Convert
```rust
impl AwsProvider {
    pub(crate) async fn read_sts_caller_identity(&self, id: &ResourceId) -> ProviderResult<State> { ... }
}
```
to:
```rust
impl crate::provider_generated::DataSourceLookups for AwsProvider {
    fn read_sts_caller_identity_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let id = resource.id.clone();
        let provider = self;
        Box::pin(async move {
            let response = provider.sts_client.get_caller_identity().send().await
                .map_err(|e| ProviderError::new(sdk_error_message(
                    "Failed to get STS caller identity", &e
                )).for_resource(id.clone()))?;
            let mut attributes = HashMap::new();
            if let Some(account) = response.account() { attributes.insert("account_id".into(), Value::String(account.into())); }
            if let Some(arn) = response.arn() { attributes.insert("arn".into(), Value::String(arn.into())); }
            if let Some(user_id) = response.user_id() { attributes.insert("user_id".into(), Value::String(user_id.into())); }
            Ok(State::existing(id, attributes))
        })
    }
}
```

The signature takes `&Resource` (per the codegen-generated trait), not `&ResourceId`. Also note: `&self` may not live across `await` if a different impl is added later that captures non-`Send` field references; tests will catch any issues.

Because identitystore.User and s3.Bucket each have their own `impl DataSourceLookups for AwsProvider` in their service files, **Rust requires only one `impl Trait for Type`**. The fix is to consolidate all three trait methods into one impl block in a dedicated file, OR move trait methods to inherent helpers and have a single `impl DataSourceLookups for AwsProvider` that delegates.

Choose the latter (delegation pattern) so each service file keeps its own concrete inherent method:

In `carina-provider-aws/src/data_source_lookups_impl.rs` (new):
```rust
use carina_core::provider::{BoxFuture, ProviderResult};
use carina_core::resource::{Resource, State};
use crate::AwsProvider;
use crate::provider_generated::DataSourceLookups;

impl DataSourceLookups for AwsProvider {
    fn read_sts_caller_identity_data_source(&self, resource: &Resource)
        -> BoxFuture<'_, ProviderResult<State>>
    {
        self.do_read_sts_caller_identity_data_source(resource)
    }
    fn read_identitystore_user_data_source(&self, resource: &Resource)
        -> BoxFuture<'_, ProviderResult<State>>
    {
        self.do_read_identitystore_user_data_source(resource)
    }
}
```

And each service file exposes a `pub(crate) fn do_read_<...>_data_source(&self, resource: &Resource) -> BoxFuture<...>` inherent method (with the actual lookup logic).

`lib.rs` adds `mod data_source_lookups_impl;` so the impl block compiles.

**Verification**:
- `cargo build -p carina-provider-aws`
- `cargo test -p carina-provider-aws aws_provider_implements_data_source_lookups_for_sts`

---

### Task 6.2: Move `identitystore.User` impl into the same `DataSourceLookups` block

**Goal**: Same migration pattern as 6.1.

**Files**: `carina-provider-aws/src/services/identitystore/user.rs`

**Test**: The Task 6.1 type-level test `aws_provider_implements_data_source_lookups_for_sts` already covers this once both methods are present. Add a regression test calling the inherent helper with a fixture-shaped `Resource` (no AWS creds; use `expect_err` to assert the `identity_store_id`-missing error path):
```rust
#[tokio::test(flavor = "current_thread")]
async fn read_identitystore_user_requires_identity_store_id() {
    let provider = crate::AwsProvider::for_tests(); // existing test helper
    let resource = carina_core::resource::Resource::with_provider(
        "aws", "identitystore.User", "lookup",
    );
    let err = provider.do_read_identitystore_user_data_source(&resource).await.unwrap_err();
    assert!(err.message.contains("identity_store_id"));
}
```

(`AwsProvider::for_tests()` needs to exist or be added — check `services/identitystore/user.rs` for existing tests; if none, this regression test can be skipped and rely on integration coverage.)

**Implementation**: Rename the existing async `pub(crate) async fn read_identitystore_user(&self, resource: &Resource) -> ProviderResult<State>` to `do_read_identitystore_user_data_source(&self, resource: &Resource) -> BoxFuture<'_, ProviderResult<State>>` (wrap in `Box::pin(async move { ... })`).

Update the trait `impl DataSourceLookups` in `data_source_lookups_impl.rs` (already added in 6.1) to call the new method name.

**Verification**: `cargo build -p carina-provider-aws` AND `cargo nextest run -p carina-provider-aws`

---

### Task 6.3: Wire `Provider::read_data_source` to the codegen dispatcher

**Goal**: Replace the hand-written body with a single call to `dispatch_read_data_source`.

**Files**: `carina-provider-aws/src/provider.rs`

**Test** (extends an existing provider integration test):
```rust
#[tokio::test]
async fn read_data_source_dispatches_via_codegen() {
    let provider = AwsProvider::for_tests();
    let resource = Resource::with_provider("aws", "unknown.Thing", "x");
    let err = provider.read_data_source(&resource).await.unwrap_err();
    assert!(err.message.contains("does not implement read_data_source"));
}
```

**Implementation**:
```rust
fn read_data_source(&self, resource: &Resource) -> BoxFuture<'_, ProviderResult<State>> {
    let resource = resource.clone();
    let me = self;
    Box::pin(async move {
        let mut state = crate::provider_generated::dispatch_read_data_source(me, &resource).await?;
        if state.exists {
            normalize_state_enums(&resource.id.resource_type, &mut state.attributes);
        }
        Ok(state)
    })
}
```

Delete the hand-written 30-line body (the "drop user inputs" safety rail moves into the codegen `_` arm — already handled in Phase 5.3).

**Verification**:
- `cargo build -p carina-provider-aws`
- `cargo nextest run -p carina-provider-aws`
- `cargo clippy -p carina-provider-aws -- -D warnings`

---

## Phase 7: Implement `s3.Bucket` data source lookup

### Task 7.1: Add `s3_hosted_zone_id` lookup table

**Goal**: A pure function mapping AWS region to S3 website hosted zone ID, with a defined error for unknown regions.

**Files**: `carina-provider-aws/src/services/s3/bucket.rs` (new helper at the bottom)

**Test** (in the same file's `#[cfg(test)] mod tests`):
```rust
#[test]
fn s3_hosted_zone_id_known_regions() {
    assert_eq!(s3_hosted_zone_id("ap-northeast-1").unwrap(), "Z2M4EHUR26P7ZW");
    assert_eq!(s3_hosted_zone_id("us-east-1").unwrap(), "Z3AQBSTGFYJSTF");
    assert_eq!(s3_hosted_zone_id("eu-west-1").unwrap(), "Z1BKCTXD74EZPE");
}

#[test]
fn s3_hosted_zone_id_unknown_region_errors() {
    let err = s3_hosted_zone_id("xx-fake-1").unwrap_err();
    assert!(err.contains("Unknown S3 region"));
}
```

**Implementation**:
```rust
/// Map AWS region → S3 website-endpoint hosted zone ID.
///
/// Source: https://docs.aws.amazon.com/general/latest/gr/s3.html
/// Limited to commercial regions; isolated partitions (GovCloud, China) are out of scope.
pub(crate) fn s3_hosted_zone_id(region: &str) -> Result<&'static str, String> {
    let id = match region {
        "us-east-1" => "Z3AQBSTGFYJSTF",
        "us-east-2" => "Z2O1EMRO9K5GLX",
        "us-west-1" => "Z2F56UZL2M1ACD",
        "us-west-2" => "Z3BJ6K6RIION7M",
        "ap-east-1" => "ZNB98KWMFR0R6",
        "ap-south-1" => "Z11RGJOFQNVJUP",
        "ap-northeast-1" => "Z2M4EHUR26P7ZW",
        "ap-northeast-2" => "Z3W03O7B5YMIYP",
        "ap-northeast-3" => "Z2YQB5RD63NC85",
        "ap-southeast-1" => "Z3O0J2DXBE1FTB",
        "ap-southeast-2" => "Z1WCIGYICN2BYD",
        "ca-central-1" => "Z1QDHH18159H29",
        "eu-central-1" => "Z21DNDUVLTQW6Q",
        "eu-west-1" => "Z1BKCTXD74EZPE",
        "eu-west-2" => "Z3GKZC51ZF0DB4",
        "eu-west-3" => "Z3R1K369G5AVDG",
        "eu-north-1" => "Z3BAZG2TWCNX0D",
        "eu-south-1" => "Z30OZKI7KPW7MI",
        "me-south-1" => "Z1MPMWCPA7YB62",
        "sa-east-1" => "Z7KQH4QJS55SO",
        _ => return Err(format!("Unknown S3 region: '{region}'")),
    };
    Ok(id)
}
```

**Verification**: `cargo test -p carina-provider-aws s3_hosted_zone_id`

---

### Task 7.2: Implement `do_read_s3_bucket_data_source`

**Goal**: The actual S3 lookup. HeadBucket + GetBucketLocation + computed fields.

**Files**: `carina-provider-aws/src/services/s3/bucket.rs`

**Test**: Argument-validation paths only (no AWS creds in unit tests):
```rust
#[tokio::test(flavor = "current_thread")]
async fn read_s3_bucket_data_source_requires_bucket_attribute() {
    let provider = crate::AwsProvider::for_tests();
    let resource = carina_core::resource::Resource::with_provider("aws", "s3.Bucket", "lookup");
    let err = provider.do_read_s3_bucket_data_source(&resource).await.unwrap_err();
    assert!(err.message.contains("`bucket`"));
}
```

**Implementation**:
```rust
impl AwsProvider {
    pub(crate) fn do_read_s3_bucket_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.clone();
        Box::pin(async move {
            let bucket = match resource.get_attr("bucket") {
                Some(Value::String(s)) => s.clone(),
                _ => return Err(ProviderError::new("`bucket` is required")
                    .for_resource(resource.id.clone())),
            };

            // 1. HeadBucket — existence check; reuse Managed-side classifier
            self.s3_client.head_bucket().bucket(&bucket).send().await
                .map_err(|err| {
                    use aws_sdk_s3::error::SdkError;
                    let kind = match &err {
                        SdkError::ServiceError(svc) => classify_head_bucket_status(
                            svc.raw().status().as_u16(),
                            svc.err().is_not_found(),
                        ),
                        _ => HeadBucketErrorKind::Other,
                    };
                    match kind {
                        HeadBucketErrorKind::NotFound => ProviderError::new(format!(
                            "Bucket '{bucket}' not found"
                        )).for_resource(resource.id.clone()),
                        HeadBucketErrorKind::AccessDenied => ProviderError::new(format!(
                            "Access denied for bucket '{bucket}'. This may indicate \
                             insufficient IAM permissions or the bucket is owned by a \
                             different AWS account."
                        )).for_resource(resource.id.clone()),
                        HeadBucketErrorKind::Other => ProviderError::new(sdk_error_message(
                            "Failed to head bucket", &err,
                        )).for_resource(resource.id.clone()),
                    }
                })?;
            // Treat NotFound as the State-not-found case rather than an error?
            // In the Managed `read_s3_bucket` path we return State::not_found.
            // For a DataSource lookup the user explicitly asked to read this
            // bucket, so a missing bucket IS an error. Keep the NotFound→Err
            // mapping above.

            // 2. GetBucketLocation — region
            let region = self.s3_client.get_bucket_location().bucket(&bucket).send().await
                .map_err(|e| ProviderError::new(sdk_error_message(
                    "Failed to get bucket location", &e
                )).for_resource(resource.id.clone()))?
                .location_constraint()
                .map(|c| c.as_str().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "us-east-1".to_string());

            // 3. Computed
            let arn = format!("arn:aws:s3:::{}", bucket);
            let bucket_domain_name = format!("{}.s3.amazonaws.com", bucket);
            let bucket_regional_domain_name = format!("{}.s3.{}.amazonaws.com", bucket, region);
            let hosted_zone_id = s3_hosted_zone_id(&region)
                .map_err(|m| ProviderError::new(m).for_resource(resource.id.clone()))?;

            let mut attrs = HashMap::new();
            attrs.insert("bucket".into(), Value::String(bucket.clone()));
            attrs.insert("arn".into(), Value::String(arn));
            attrs.insert("region".into(), Value::String(region));
            attrs.insert("bucket_domain_name".into(), Value::String(bucket_domain_name));
            attrs.insert("bucket_regional_domain_name".into(), Value::String(bucket_regional_domain_name));
            attrs.insert("hosted_zone_id".into(), Value::String(hosted_zone_id.into()));

            Ok(State::existing(resource.id.clone(), attrs).with_identifier(&bucket))
        })
    }
}
```

**Verification**: `cargo test -p carina-provider-aws read_s3_bucket_data_source_requires_bucket_attribute`

---

### Task 7.3: Wire the new lookup into `DataSourceLookups`

**Goal**: Add the trait method to `data_source_lookups_impl.rs`.

**Files**: `carina-provider-aws/src/data_source_lookups_impl.rs`

**Test**: The Phase-6 type assertion is now incomplete — extend it to confirm the s3 method exists:
```rust
#[test]
fn aws_provider_implements_all_data_source_lookups() {
    fn assert_impl<T: crate::provider_generated::DataSourceLookups>() {}
    assert_impl::<crate::AwsProvider>();
}
```
(Same body, but lives now that all three trait methods are required. Compile failure would surface a missing method.)

**Implementation**: Add to `impl DataSourceLookups for AwsProvider` block:
```rust
fn read_s3_bucket_data_source(&self, resource: &Resource)
    -> BoxFuture<'_, ProviderResult<State>>
{
    self.do_read_s3_bucket_data_source(resource)
}
```

**Verification**:
- `cargo build -p carina-provider-aws`
- `cargo nextest run -p carina-provider-aws aws_provider_implements_all_data_source_lookups`

---

### Task 7.4: Workspace-wide green

**Goal**: All checks pass.

**Files**: none (verification only).

**Test**: Run the verify cycle.

**Implementation**: None.

**Verification**:
- `cargo check --workspace --all-targets`
- `cargo nextest run --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build -p carina-provider-aws --target wasm32-wasip2 --release`
- `cargo fmt --check`

---

## Phase 8: Acceptance test

### Task 8.1: Acceptance fixture skeleton

**Goal**: Create the directory layout. Pure file-creation; no execution yet.

**Files**:
- `acceptance-tests/s3_bucket_data_source/basic.crn.template` (new)
- `acceptance-tests/s3_bucket_data_source/run.sh` (new, executable)
- `acceptance-tests/s3_bucket_data_source/tests/.gitkeep` (or initial assertion script)

**Test**: A repository-level shell test (or simple file-exists check) — minimum: confirm `run.sh` is `chmod +x` and `basic.crn.template` parses as Carina DSL with the env vars unsubstituted (Carina parser may reject `${...}`; in that case verify only that `envsubst` produces valid output).

```sh
#!/usr/bin/env bash
# acceptance-tests/s3_bucket_data_source/tests/smoke_dryrun.sh
set -euo pipefail
NEW_BUCKET="dummy-new" PRE_BUCKET="dummy-pre" envsubst < basic.crn.template > /tmp/_dryrun.crn
grep -q 'aws.s3.Bucket' /tmp/_dryrun.crn
grep -q 'read aws.s3.Bucket' /tmp/_dryrun.crn
```

**Implementation**:

`basic.crn.template`:
```hcl
provider aws {
  region = aws.Region.ap_northeast_1
}

let new_bucket = aws.s3.Bucket {
  bucket = "${NEW_BUCKET}"
}

let existing = read aws.s3.Bucket {
  bucket = "${PRE_BUCKET}"
}
```

`run.sh` (executable):
```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PRE_BUCKET="carina-smoke-pre-$(uuidgen | tr -d - | head -c 8 | tr A-Z a-z)"
NEW_BUCKET="carina-smoke-new-$(uuidgen | tr -d - | head -c 8 | tr A-Z a-z)"
export PRE_BUCKET NEW_BUCKET

cleanup() {
  aws s3api delete-bucket --bucket "$PRE_BUCKET" --region ap-northeast-1 2>/dev/null || true
}
trap cleanup EXIT

aws s3api create-bucket --bucket "$PRE_BUCKET" --region ap-northeast-1 \
    --create-bucket-configuration LocationConstraint=ap-northeast-1

envsubst < basic.crn.template > basic.crn

carina apply -y .
carina destroy -y .
rm -f basic.crn carina.state.json
```

**Verification**: `bash acceptance-tests/s3_bucket_data_source/tests/smoke_dryrun.sh`

---

### Task 8.2: Plan-level assertion script

**Goal**: A verification step inside `run.sh` that confirms Carina's `plan` output mentions both a Create and a Read effect on `s3.Bucket`. This proves the dual-registration smoke test does what it claims.

**Files**: `acceptance-tests/s3_bucket_data_source/run.sh` (extend), `acceptance-tests/s3_bucket_data_source/tests/assert_plan.sh` (new).

**Test**: The full run.sh test goes against a real AWS account (manual) — the dry-run portion is `tests/smoke_dryrun.sh`. Add a new dry-run that shells out to a Carina binary and checks plan output structure:

```sh
#!/usr/bin/env bash
# tests/assert_plan.sh — invoked by run.sh between create-bucket and apply
set -euo pipefail

PLAN=$(carina plan . 2>&1 || true)

if ! grep -qE 'Create.*aws\.s3\.Bucket' <<<"$PLAN"; then
    echo "FAIL: expected Create effect for aws.s3.Bucket; got:"
    echo "$PLAN"
    exit 1
fi
if ! grep -qE 'Read.*aws\.s3\.Bucket' <<<"$PLAN"; then
    echo "FAIL: expected Read effect for aws.s3.Bucket; got:"
    echo "$PLAN"
    exit 1
fi
echo "OK: plan contains both Create and Read for aws.s3.Bucket"
```

**Implementation**: In `run.sh`, between `envsubst` and `carina apply`:
```bash
bash tests/assert_plan.sh
```

**Verification** (manual, AWS creds required):
```
cd acceptance-tests/s3_bucket_data_source && aws-vault exec <profile> -- bash run.sh
```

---

## Self-review

- ✅ Every requirement from the design doc maps to a task: `DataSourceOutput` (1.1, 1.2), `output_attributes` migration (2.1, 2.2, 3.x), file split (4.3 via codegen), `DataSourceLookups` trait (5.x, 6.x, 7.3), s3 lookup (7.1, 7.2), acceptance test (8.x).
- ✅ No placeholders ("similar to", "etc."): each task names its file, function, test code, and exact verify command.
- ✅ Type consistency: `DataSourceOutput` shape is fixed in 1.1 and consumed unchanged in every subsequent task.
- ✅ Order respects dependencies: type before usage (1→2), tests before generator rewrite (3.1 pins, 3.2 rewrites), trait declaration before impls (5→6), service migration before s3 implementation (6→7), production code before acceptance test (1-7→8).
- ✅ Each task is independently verifiable (one `cargo test` / `cargo build` / shell command).
- ⚠️ Phase-5/6 boundary intentionally lands the workspace red between 5.4 (provider_generated.rs regenerated) and 6.1 (first impl wired). This is the canonical TDD red-bar; tasks 6.1, 6.2, 7.3 each restore green incrementally.
- ⚠️ `for_tests()` test helper on `AwsProvider` may not exist; tasks that depend on it (6.2, 7.2) include a fallback note but should be cross-checked against current code before pulling each task.
