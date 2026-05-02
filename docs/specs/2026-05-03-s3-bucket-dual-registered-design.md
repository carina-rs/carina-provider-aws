# `aws.s3.Bucket` Dual-Registered: Design

<!-- derived-from ./2026-04-14-datasource-def-design.md -->

## Goal

Ship `aws.s3.Bucket` as the first resource type registered in **both** `SchemaKind::Managed` and `SchemaKind::DataSource`, validating end-to-end that the dual-registration support added by carina#2328 / aws#161 works through schema → DSL parse → validation → provider dispatch → AWS API.

This is the smoke test promised in carina#2330's "follow-up" section. Closes aws#163.

## Non-goals

- Splitting the bundled `aws.s3.Bucket` Managed schema into per-API resources (Terraform-style decomposition). Tracked separately in aws#164.
- Surfacing AWS API operations in generated docs. Tracked separately in aws#165.

## Chosen approach (summary)

| Decision | Choice | Rationale |
|---|---|---|
| DataSource attribute set | Terraform parity (minimal) | Matches user mental model from `aws_s3_bucket` data source; avoids 6× API call cost; clean separation between "identify a bucket" and "configure a bucket" |
| Schema declaration | Explicit `output_attributes` on `DataSourceDef` (case C) | One source of truth for what a data source returns; supports computed fields (`arn`, `bucket_domain_name`) that aren't 1:1 with API output |
| File layout | Two files: `bucket.rs` + `bucket_data_source.rs` (case Y) | Preserves codegen's "1 def → 1 file" model; physical separation makes review trivial |
| Dispatch | `DataSourceLookups` trait, codegen-emitted (case R) | Compile-time enforcement of "every DataSourceDef has an implementation"; scales as data sources multiply |
| Acceptance test | `run.sh` pre-creates the `read` target and cleans it up; `.crn` declares both a Managed bucket (Carina creates / destroys) and a `read` bucket (pre-created) so dual-registration is exercised in one apply | Single fixture validates SchemaRegistry kind-routing end-to-end (validation, planner, dispatch); mirrors real-world DataSource usage (read something Carina did not create) |
| `read_ops` field | Keep | Future IAM policy generation; aws#165 will surface in docs |
| Existing DataSources | Migrate to `output_attributes` in same PR | Avoid mixed `read_ops`-inferred / `output_attributes`-explicit modes |

## DataSource attribute set (Terraform parity)

`aws.s3.Bucket` (DataSource) returns:

| attribute | source | type |
|---|---|---|
| `bucket` | input echo | String |
| `arn` | computed: `format!("arn:aws:s3:::{}", bucket)` | aws.arn |
| `region` | API: `GetBucketLocation.LocationConstraint` (with `us-east-1` fallback for empty) | aws.Region |
| `bucket_domain_name` | computed: `format!("{}.s3.amazonaws.com", bucket)` | String |
| `bucket_regional_domain_name` | computed: `format!("{}.s3.{}.amazonaws.com", bucket, region)` | String |
| `hosted_zone_id` | computed: lookup table keyed by region | String |

Input attributes:

| attribute | required | identity | description |
|---|---|---|---|
| `bucket` | yes | yes | Bucket name; matches existing Managed `bucket` create-only field by name and type |

API operations used:
- `HeadBucket(Bucket: bucket)` — existence check (returns 404 → `not_found`)
- `GetBucketLocation(Bucket: bucket)` — region lookup

## `DataSourceDef` extension: `output_attributes`

The current `DataSourceDef` (introduced aws#136) infers schema attributes from `read_ops` field tuples. This design extends it to declare outputs explicitly, separating the schema contract from the API surface used to populate it.

### New struct shape

```rust
pub struct DataSourceDef {
    pub name: &'static str,
    pub service_namespace: &'static str,
    pub inputs: Vec<DataSourceInput>,
    /// What the schema exposes. Single source of truth for codegen.
    pub output_attributes: Vec<DataSourceOutput>,
    /// API operations the lookup uses. Documentation / IAM hint /
    /// future-IAM-policy-gen value; NOT consumed by schema codegen.
    pub read_ops: Vec<ReadOp>,
    pub type_overrides: Vec<(&'static str, &'static str)>,
    pub exclude_fields: Vec<&'static str>,
}

pub struct DataSourceOutput {
    pub name: &'static str,            // "arn"
    pub provider_name: Option<&'static str>, // None for computed; Some("Account") for API-derived
    pub description: &'static str,
    /// Type expression as a Rust string for codegen, e.g. "AttributeType::String"
    /// or "super::aws_account_id()". Required: the codegen does not infer it
    /// from Smithy when the field is computed.
    pub type_code: &'static str,
}
```

`type_overrides` and `exclude_fields` are kept for backwards compatibility with the current `read_ops`-driven inference flow during the migration window, but are removed once all existing DataSources are migrated to explicit `output_attributes`.

### Migration of existing DataSources

`sts.CallerIdentity` and `identitystore.User` move to `output_attributes`. Behaviour unchanged; the codegen path that walks `read_ops` to materialise schema attributes is deleted.

## File layout

```
carina-provider-aws/src/schemas/generated/s3/
├── bucket.rs              # s3_bucket_config()              (Managed; existing)
└── bucket_data_source.rs  # s3_bucket_data_source_config()  (DataSource; new)
```

`schemas/generated/s3/mod.rs` declares both modules.

`schemas/generated/mod.rs::configs()` registers both:

```rust
pub fn configs() -> Vec<AwsSchemaConfig> {
    vec![
        // ... existing entries ...
        s3::bucket::s3_bucket_config(),
        s3::bucket_data_source::s3_bucket_data_source_config(),
        // ...
    ]
}
```

`SchemaRegistry::insert("aws", schema)` reads `schema.kind` and routes to the correct sub-map; both end up under provider="aws", resource_type="s3.Bucket" but in different `SchemaKind` slots. No further changes needed in carina-core.

## Dispatch: `DataSourceLookups` trait

Codegen emits a trait declaring one method per `DataSourceDef`:

```rust
// In src/provider_generated.rs (codegen output)
pub trait DataSourceLookups {
    fn read_s3_bucket_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_identitystore_user_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_sts_caller_identity_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>>;
}
```

`AwsProvider` must `impl DataSourceLookups`. Each impl lives near its hand-written lookup logic in `services/<service>/<resource>.rs`. Compilation fails if a `DataSourceDef` has no corresponding trait method implementation.

The hand-written `read_data_source` in `provider.rs` becomes a generated `match` (in `provider_generated.rs`):

```rust
fn read_data_source(&self, resource: &Resource) -> BoxFuture<'_, ProviderResult<State>> {
    match resource.id.resource_type.as_str() {
        "s3.Bucket" => self.read_s3_bucket_data_source(resource),
        "identitystore.User" => self.read_identitystore_user_data_source(resource),
        "sts.CallerIdentity" => self.read_sts_caller_identity_data_source(resource),
        _ => Box::pin(async move {
            Err(ProviderError::new(format!(
                "aws provider does not implement read_data_source for '{}'",
                resource.id.resource_type
            )).for_resource(resource.id.clone()))
        }),
    }
}
```

The current `provider.rs::read_data_source` (the hand-written dispatcher with the "drop user inputs" safety rail) is deleted; the safety rail moves into the generated dispatcher's default case.

## Implementation: `read_s3_bucket_data_source`

In `services/s3/bucket.rs`:

```rust
impl DataSourceLookups for AwsProvider {
    fn read_s3_bucket_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.clone();
        Box::pin(async move {
            let Some(Value::String(bucket)) = resource.get_attr("bucket") else {
                return Err(ProviderError::new("`bucket` is required").for_resource(resource.id.clone()));
            };

            // 1. HeadBucket — existence check
            self.s3_client.head_bucket().bucket(bucket).send().await
                .map_err(|e| /* ... existing classify_head_bucket_status ... */)?;

            // 2. GetBucketLocation — region
            let region = self.s3_client.get_bucket_location().bucket(bucket).send().await
                .map(|r| r.location_constraint().map(|c| c.as_str().to_string())
                    .unwrap_or_else(|| "us-east-1".to_string()))?;

            // 3. Computed
            let arn = format!("arn:aws:s3:::{}", bucket);
            let bucket_domain_name = format!("{}.s3.amazonaws.com", bucket);
            let bucket_regional_domain_name = format!("{}.s3.{}.amazonaws.com", bucket, region);
            let hosted_zone_id = s3_hosted_zone_id(&region)?.to_string();

            let mut attrs = HashMap::new();
            attrs.insert("bucket".into(), Value::String(bucket.clone()));
            attrs.insert("arn".into(), Value::String(arn));
            attrs.insert("region".into(), Value::String(region));
            attrs.insert("bucket_domain_name".into(), Value::String(bucket_domain_name));
            attrs.insert("bucket_regional_domain_name".into(), Value::String(bucket_regional_domain_name));
            attrs.insert("hosted_zone_id".into(), Value::String(hosted_zone_id));

            Ok(State::existing(resource.id.clone(), attrs).with_identifier(bucket))
        })
    }
}
```

The `s3_hosted_zone_id(region)` lookup table is hand-written in the same file (≈30 entries from AWS doc). Future regions can be added incrementally; unknown region returns an error rather than guessing.

## Acceptance test

`acceptance-tests/s3_bucket_data_source/`:

```
basic.crn.template — fixture template; `.crn` rendered by run.sh
run.sh             — orchestrates the test
tests/             — assertion helpers (existence, attribute values)
```

The fixture exercises **both kinds of `s3.Bucket` registration in a single apply**, so the SchemaRegistry kind-routing path is hit end-to-end (validation, planner Create-vs-Read split, provider dispatch).

Bucket ownership split:

| Bucket | Created by | Destroyed by | Carina sees it as |
|---|---|---|---|
| `PRE_BUCKET` | `run.sh` (before apply) | `run.sh` (after destroy) | `read aws.s3.Bucket` (DataSource) |
| `NEW_BUCKET` | `carina apply` | `carina destroy` | `aws.s3.Bucket` (Managed) |

`run.sh` flow:

```bash
PRE_BUCKET="carina-smoke-pre-$(uuidgen | tr -d - | head -c 8 | tr A-Z a-z)"
NEW_BUCKET="carina-smoke-new-$(uuidgen | tr -d - | head -c 8 | tr A-Z a-z)"
export PRE_BUCKET NEW_BUCKET
trap 'aws s3api delete-bucket --bucket "$PRE_BUCKET" --region ap-northeast-1 || true' EXIT

aws s3api create-bucket --bucket "$PRE_BUCKET" --region ap-northeast-1 \
    --create-bucket-configuration LocationConstraint=ap-northeast-1

envsubst < basic.crn.template > basic.crn

carina apply -y .
# assertions: NEW_BUCKET created, PRE_BUCKET data source resolved with expected attrs
carina destroy -y .
```

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

The plan must contain exactly two effects, both for resource type `aws.s3.Bucket` but routed through different SchemaKind paths:

- one `Create` for `new_bucket` (Managed) → calls `Provider::create()` → `create_s3_bucket`
- one `Read` for `existing` (DataSource) → calls `Provider::read_data_source()` → `read_s3_bucket_data_source`

This is the smoking-gun assertion: the same `(provider, resource_type)` produces two distinct effects in one plan, with the right method dispatched on each. After apply, `existing` exposes `bucket / arn / region / bucket_domain_name / bucket_regional_domain_name / hosted_zone_id` populated from the pre-existing bucket; `new_bucket` is a fresh, empty bucket.

## Edge cases

- **HeadBucket returns 404**: data source `read` returns `State::not_found(id)`. Plan-time validator already passes a missing data source through; runtime resolution surfaces the absence.
- **HeadBucket returns 403** (access denied / cross-account): same handling as the Managed `read_s3_bucket` — wrap in a "Access denied for bucket … This may indicate insufficient IAM permissions or the bucket is owned by a different AWS account." error.
- **GetBucketLocation returns empty `LocationConstraint`**: AWS-documented quirk for `us-east-1`. Default to `"us-east-1"`.
- **Unknown region in `s3_hosted_zone_id` table**: error (rather than guess). The table seed covers all current AWS commercial regions; isopartition (GovCloud, China) regions are out of scope and returned as a typed error.
- **Same `bucket` name appearing in both Managed `let new_bucket = aws.s3.Bucket {bucket="X"}` and DataSource `read aws.s3.Bucket {bucket="X"}`**: Carina-core treats them as distinct resources (different `binding`); the planner creates one and reads the other. No collision.
- **Migration of `sts.CallerIdentity` / `identitystore.User` to `output_attributes`**: zero-input case (sts) and multi-input-with-conditional-required case (identitystore). Both must produce byte-identical schema output before/after the migration; covered by snapshot test in carina-codegen-aws.

## File-change summary

| File | Change |
|---|---|
| `carina-codegen-aws/src/resource_defs.rs` | Add `DataSourceOutput` struct. Add `output_attributes` field to `DataSourceDef`. Migrate `sts_data_sources()` and `identitystore_data_sources()` to use `output_attributes`. Add `s3_data_sources()` returning the new `s3.Bucket` DataSource. |
| `carina-codegen-aws/src/main.rs` | `generate_data_source` reads `output_attributes` instead of inferring from `read_ops`. Generate trait method declaration into `provider_generated.rs`. Generate `read_data_source` `match` into `provider_generated.rs`. Drop the now-unused `read_ops`-driven attribute inference. |
| `carina-provider-aws/src/schemas/generated/s3/bucket_data_source.rs` | NEW — codegen output. |
| `carina-provider-aws/src/schemas/generated/s3/mod.rs` | Add `pub mod bucket_data_source;` |
| `carina-provider-aws/src/schemas/generated/mod.rs::configs()` | Register `s3::bucket_data_source::s3_bucket_data_source_config()`. |
| `carina-provider-aws/src/schemas/generated/identitystore/user.rs` | Regenerated under new `output_attributes` flow (byte-equivalent output). |
| `carina-provider-aws/src/schemas/generated/sts/caller_identity.rs` | Regenerated (byte-equivalent output). |
| `carina-provider-aws/src/provider_generated.rs` | NEW codegen output: `DataSourceLookups` trait + `read_data_source` `match`. |
| `carina-provider-aws/src/provider.rs` | Drop the hand-written `read_data_source`; the trait + dispatcher come from codegen. Keep `Provider` impl for everything else. |
| `carina-provider-aws/src/services/s3/bucket.rs` | Add `impl DataSourceLookups for AwsProvider { fn read_s3_bucket_data_source ... }`. Add `s3_hosted_zone_id` lookup table. |
| `carina-provider-aws/src/services/sts/caller_identity.rs` | Convert existing `read_sts_caller_identity` into `impl DataSourceLookups for AwsProvider { fn read_sts_caller_identity_data_source ... }`. |
| `carina-provider-aws/src/services/identitystore/user.rs` | Convert similarly. |
| `acceptance-tests/s3_bucket_data_source/basic.crn.template` | NEW |
| `acceptance-tests/s3_bucket_data_source/run.sh` | NEW |
| `acceptance-tests/s3_bucket_data_source/tests/` | NEW assertion scaffolding (mirrors existing acceptance test layout) |

## Verification plan

- `cargo check --workspace` clean
- `cargo nextest run --workspace` — all existing tests pass; snapshot test for `sts.CallerIdentity` / `identitystore.User` schema output stays byte-identical
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- `cargo build -p carina-provider-aws --target wasm32-wasip2 --release` clean
- `acceptance-tests/s3_bucket_data_source/run.sh` succeeds against a real AWS account (manual run; CI requires AWS creds)

## Related

- carina#2328 — parent: `SchemaRegistry` introduction
- carina#2330 — the SchemaRegistry PR
- aws#161 / aws#162 — provider migration to `SchemaRegistry` (landed)
- aws#163 — this issue
- aws#164 — Terraform-style resource decomposition (separate, larger discussion)
- aws#165 — Surfacing AWS API operations in generated docs (separate)
- ADR: `carina-rs/carina:docs/specs/2026-05-02-resource-vs-data-source-design.md`
