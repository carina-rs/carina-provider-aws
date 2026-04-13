# schema_structure: codegen support for non-standard API patterns

## Goal

Enable the Smithy codegen to generate resource schemas for AWS services whose create API doesn't directly expose resource fields as top-level input parameters (e.g., Route 53 `ChangeResourceRecordSets` where fields are nested inside `ChangeBatch`).

## Problem

The codegen currently resolves writable fields from the create operation's input structure. This works for standard CRUD APIs (EC2 `CreateVpc` → input has `CidrBlock`, `InstanceTenancy`, etc.) but fails for batch/aggregate APIs where the resource fields are in a nested structure:

- Route 53: `ChangeResourceRecordSetsRequest` has `HostedZoneId` + `ChangeBatch`, actual record fields are in `ResourceRecordSet`
- Potential future cases: DynamoDB `BatchWriteItem`, CloudWatch `PutMetricData`, etc.

## Chosen Approach: `schema_structure` field

Add `schema_structure: Option<&'static str>` to `ResourceDef`. When set, the codegen derives writable fields from this named Smithy structure instead of the create operation's input.

### How it works

1. If `schema_structure` is `Some("ResourceRecordSet")`, the codegen:
   - Resolves `com.amazonaws.route53#ResourceRecordSet` from the Smithy model
   - Uses its members as the writable field source (replacing the create input)
   - Still respects `exclude_fields`, `type_overrides`, `create_only_overrides`, `identity_overrides`, `required_overrides`
   - `extra_writable` adds fields not in the schema structure (e.g., `HostedZoneId`)

2. If `schema_structure` is `None` (default), existing behavior unchanged — fields come from create input.

3. `read_structure` remains separate — it's used for read-back extraction methods. For many resources, `schema_structure` and `read_structure` will be the same Smithy structure.

### Changes

#### `carina-codegen-aws/src/resource_defs.rs`
- Add `schema_structure: Option<&'static str>` to `ResourceDef`
- Add `identity_overrides: Vec<&'static str>` to `ResourceDef`
- Add `route53_resources()` function

#### `carina-codegen-aws/src/main.rs`
- In `generate_resource()`: when `schema_structure` is set, resolve fields from it instead of `create_input`
- Add `is_identity` to `AttrInfo` and emit `.identity()` when set
- Register `route53_resources()` in the resource collection

#### `scripts/download-smithy-models.sh`
- Add Route 53 model download

## Route 53 ResourceDef

```rust
ResourceDef {
    name: "route53.record_set",
    service_namespace: "com.amazonaws.route53",
    schema_structure: Some("ResourceRecordSet"),
    create_op: "ChangeResourceRecordSets",
    read_structure: Some("ResourceRecordSet"),
    identifier: "Name",
    has_tags: false,
    required_overrides: vec!["Name", "Type"],
    create_only_overrides: vec!["Name"],
    identity_overrides: vec!["Type"],
    extra_writable: vec![
        ExtraField {
            name: "HostedZoneId",
            read_source: None,
            description: Some("The ID of the hosted zone."),
        },
    ],
    exclude_fields: vec![
        "SetIdentifier", "Weight", "Region", "Failover",
        "MultiValueAnswer", "GeoLocation", "GeoProximityLocation",
        "HealthCheckId", "TrafficPolicyInstanceId", "CidrRoutingConfig",
    ],
    ...
}
```

## Edge cases

- `extra_writable` fields with `schema_structure` should be marked `create_only` (same as current behavior)
- `exclude_fields` removes fields from the schema structure that aren't needed in the initial version (routing policies, health checks)
- `TTL` in Route 53 Smithy model is typed as `Long` — codegen should map to `Int`
