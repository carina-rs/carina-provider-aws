# Implementation Plan: schema_structure codegen + Route 53

## Task 1/4: Add `schema_structure` and `identity_overrides` to codegen

**Files:**
- `carina-codegen-aws/src/resource_defs.rs` — add fields to `ResourceDef`
- `carina-codegen-aws/src/main.rs` — resolve fields from `schema_structure`, add `is_identity` to `AttrInfo`, emit `.identity()`

**Test:** Existing codegen tests must pass. Add test for `schema_structure` field resolution.

**Acceptance:** `cargo test -p carina-codegen-aws` passes, `cargo clippy` clean.

## Task 2/4: Add Route 53 ResourceDef and generate schema

**Files:**
- `carina-codegen-aws/src/resource_defs.rs` — add `route53_resources()`
- `carina-codegen-aws/src/main.rs` — register `route53_resources()`
- `scripts/download-smithy-models.sh` — add route53 model
- `carina-provider-aws/src/schemas/generated/route53/` — generated output

**Test:** Run `generate-schemas-smithy.sh`, verify generated schema has correct attributes. Review codegen diff.

**Acceptance:** Generated `route53/record_set.rs` has `hosted_zone_id`, `name`, `type` (.identity()), `ttl`, `resource_records`, `alias_target`.

## Task 3/4: Replace hand-written schema with generated schema + update service impl

**Files:**
- `carina-provider-aws/src/schemas/generated/route53/record_set.rs` — replace hand-written with codegen output
- `carina-provider-aws/src/schemas/generated/route53/mod.rs` — update if needed
- `carina-provider-aws/src/schemas/generated/mod.rs` — update registration
- `carina-provider-aws/src/services/route53/record_set.rs` — adapt to generated field names if they differ

**Test:** `cargo build`, `cargo test`, `cargo clippy` all pass.

## Task 4/4: Generate docs and create PR

**Files:**
- `generated-docs/` if applicable
- Update existing PR #106

**Acceptance:** CI passes, PR ready for review.
