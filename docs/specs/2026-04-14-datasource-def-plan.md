# Implementation Plan: DataSourceDef

## Task 1/4: Define DataSourceDef and migrate sts.caller_identity

**Files:**
- `carina-codegen-aws/src/resource_defs.rs`

**Changes:**
- Add `DataSourceDef` and `DataSourceInput` structs
- Add `sts_data_sources()` returning `sts.caller_identity` as a `DataSourceDef`
- Remove `sts.caller_identity` from `sts_resources()`
- Ensure existing `ReadOp` struct is reused

**Tests:**
- Compile check: struct definitions valid
- Existing codegen tests still pass after moving sts.caller_identity

## Task 2/4: Codegen schema and docs generation for DataSourceDef

**Files:**
- `carina-codegen-aws/src/main.rs`

**Changes:**
- Collect `DataSourceDef` entries alongside `ResourceDef` entries
- Extract shared helpers from existing codegen (type resolution, attribute generation)
- Generate `.rs` schema for data sources: `ResourceSchema::new(...).as_data_source()` with input + output attributes
- Generate `.md` docs for data sources
- Add data source entries to `cf_type_name()`
- Regenerate `sts/caller_identity.rs` schema via codegen

**Tests:**
- Regenerated `sts/caller_identity.rs` matches expected output
- Generated docs include data source attributes
- Existing managed resource codegen unaffected

## Task 3/4: Add identitystore.user as DataSourceDef

**Files:**
- `carina-codegen-aws/src/resource_defs.rs`
- `scripts/download-smithy-models.sh`
- `carina-codegen-aws/src/main.rs` (cf_type_name)

**Changes:**
- Add `identitystore_data_sources()` with user lookup inputs
- Add identitystore Smithy model download
- Add `identitystore.user` to `cf_type_name()`
- Regenerate schema and docs

**Tests:**
- Generated `identitystore/user.rs` schema matches expected attributes
- Generated docs show lookup inputs and output attributes

## Task 4/4: Remove hand-written identitystore/user.rs, verify end-to-end

**Files:**
- `carina-provider-aws/src/schemas/generated/identitystore/user.rs` (regenerated)
- Existing tests

**Changes:**
- Replace hand-written schema with codegen output
- Verify all tests pass
- Verify docs generation produces `identitystore/user.md`

**Tests:**
- `cargo test` all pass
- Codegen Check CI passes
- `generate-docs.sh` produces identitystore/user.md
