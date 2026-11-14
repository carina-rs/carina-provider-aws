//! #700: an offline schema inventory and obligations created by identified inputs.
//! It flags only anonymous String producers; a producer typed with a different identity is out of scope.
use std::collections::{BTreeMap, BTreeSet};

use carina_core::schema::{RawShape, ValidatedConfigSchema, ValidatedProviderSchemaWitness};
use heck::ToSnakeCase;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{generated, schema_inventory as inventory};

fn raw_manifest() -> Value {
    serde_json::from_str(include_str!("../fixtures/awscc-identified-inputs.json")).unwrap()
}

/// The updater uses the same compact, recursively key-sorted UTF-8 JSON encoding.
fn checksum(value: &Value) -> String {
    let mut canonical = value.clone();
    canonical.sort_all_objects();
    // Rust serde_json and Python json.dumps(sort_keys, separators, ensure_ascii=False) agree only while the manifest holds no floats.
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).unwrap())
    )
}

fn manifest_content_hash(manifest: &Value) -> String {
    let mut content = manifest.clone();
    content["source"]
        .as_object_mut()
        .unwrap()
        .remove("content_sha256");
    checksum(&content)
}

fn verify_manifest_hashes(manifest: &Value) -> Result<(), String> {
    if manifest["format"] != 2 {
        return Err("refresh the manifest to integrity format 2".into());
    }
    if manifest["source"]["content_sha256"] != manifest_content_hash(manifest) {
        return Err("manifest content_sha256 mismatch".into());
    }
    Ok(())
}

fn manifest() -> Value {
    let mut manifest = raw_manifest();
    for kind in ["inputs", "outputs"] {
        let definitions = manifest["definitions"].clone();
        for leaf in manifest[kind].as_array_mut().unwrap() {
            leaf["definition"] = definitions[leaf["identity"].as_str().unwrap()].clone();
        }
    }
    manifest
}

fn allowlist() -> Vec<Value> {
    serde_json::from_str(include_str!("../fixtures/identity-producer-allowlist.json")).unwrap()
}

fn require_nonblank(entry: &Value, fields: &[&str]) -> Result<(), String> {
    for field in fields {
        if entry[*field].as_str().unwrap_or_default().trim().is_empty() {
            return Err(format!("missing nonblank string {field}: {entry}"));
        }
    }
    Ok(())
}

fn expected_identity_differences(reviewed: &[Value]) -> Result<BTreeSet<String>, String> {
    reviewed
        .iter()
        .map(|entry| {
            require_nonblank(entry, &["identity", "reason", "evidence", "review_when"])?;
            Ok(
                json!({"identity": entry["identity"], "aws": entry["aws"], "awscc": entry["awscc"]})
                    .to_string(),
            )
        })
        .collect()
}

fn reviewed_aliases(aliases: Vec<Value>) -> Result<Vec<Value>, String> {
    let mut seen = BTreeSet::new();
    for alias in &aliases {
        require_nonblank(
            alias,
            &[
                "resource",
                "property",
                "destination_service",
                "destination_property",
                "reason",
                "evidence",
                "review_when",
            ],
        )?;
        let selector = (
            alias["resource"].as_str().unwrap(),
            alias["property"].as_str().unwrap(),
        );
        if !seen.insert(selector) {
            return Err(format!("duplicate alias {selector:?}"));
        }
    }
    let aws = leaves();
    let manifest = raw_manifest();
    let inventory = aws
        .iter()
        .chain(manifest["inputs"].as_array().unwrap())
        .chain(manifest["outputs"].as_array().unwrap());
    for alias in &aliases {
        if !inventory.clone().any(|leaf| {
            (alias["resource"] == "*" || alias["resource"] == leaf["resource"])
                && alias["property"] == property_name(leaf)
        }) {
            return Err(format!(
                "stale alias: {}/{}",
                alias["resource"].as_str().unwrap(),
                alias["property"].as_str().unwrap()
            ));
        }
    }
    Ok(aliases)
}

fn aliases() -> Vec<Value> {
    reviewed_aliases(
        serde_json::from_str(include_str!("../fixtures/identity-producer-aliases.json")).unwrap(),
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

fn leaves() -> Vec<Value> {
    generated::configs()
        .iter()
        .flat_map(|config| inventory::schema_leaves("aws", &config.schema))
        .collect()
}

fn source(leaf: &Value) -> String {
    format!(
        "{}.{}/{}/{}",
        leaf["owner"].as_str().unwrap(),
        leaf["resource"].as_str().unwrap(),
        leaf["view"].as_str().unwrap(),
        leaf["path"].as_str().unwrap()
    )
}

fn property_name(leaf: &Value) -> String {
    leaf["path"]
        .as_str()
        .unwrap()
        .rsplit('.')
        .next()
        .unwrap()
        .replace("[]", "")
}

/// Candidates use service/property semantics, never a blanket *_id/*_name rule.
/// Aliases come from the reviewed fixture, with exact resources preferred over `*`.
fn key(leaf: &Value, aliases: &[Value]) -> (String, String) {
    let resource = leaf["resource"].as_str().unwrap();
    let service = leaf["service"].as_str().unwrap();
    let path = leaf["path"].as_str().unwrap();
    let property = property_name(leaf);
    if let Some(alias) = aliases
        .iter()
        .find(|alias| alias["resource"] == resource && alias["property"] == property)
        .or_else(|| {
            aliases
                .iter()
                .find(|alias| alias["resource"] == "*" && alias["property"] == property)
        })
    {
        return (
            alias["destination_service"].as_str().unwrap().into(),
            alias["destination_property"].as_str().unwrap().into(),
        );
    }
    // A policy's nested `id` is not its containing resource's ID. Keep map-key
    // ancestry too: Condition's inner variable keys are not operator names.
    let property = if !path.contains('.')
        && matches!(property.as_str(), "id" | "arn" | "name" | "arns" | "names")
    {
        let kind = match property.as_str() {
            "arns" => "arn",
            "names" => "name",
            other => other,
        };
        let name = resource.split('.').nth(1).unwrap().to_snake_case();
        format!("{name}_{kind}")
    } else {
        property
    };
    (service.into(), property)
}

fn gaps(
    leaves: &[Value],
    sinks: &[Value],
    aliases: &[Value],
) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut obligations = BTreeMap::<_, BTreeSet<_>>::new();
    for sink in sinks {
        let Some(identity) = sink["identity"].as_str() else {
            continue;
        };
        let sink_key = key(sink, aliases);
        for producer in leaves
            .iter()
            .filter(|leaf| leaf["anonymous_string"] == true)
        {
            if key(producer, aliases) == sink_key {
                obligations
                    .entry((source(producer), identity.to_string()))
                    .or_default()
                    .insert(source(sink));
            }
        }
    }
    obligations
}

fn check(leaves: &[Value], sinks: &[Value], exceptions: &[Value]) -> Result<(), String> {
    for exception in exceptions {
        require_nonblank(
            exception,
            &[
                "source",
                "destination_identity",
                "reason",
                "evidence",
                "issue",
                "review_when",
                "expected",
            ],
        )?;
        let pair = (
            exception["source"].as_str().unwrap(),
            exception["destination_identity"].as_str().unwrap(),
        );
        let expected = exception["expected"].as_str().unwrap();
        if !matches!(
            expected,
            "no_semantic_producer_match"
                | "temporary_lookup_compatibility"
                | "not_a_current_positive_resource_edge"
        ) {
            return Err(format!("{pair:?}: invalid expected {expected:?}"));
        }
    }
    let mut gaps = gaps(leaves, sinks, &aliases());
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        let source = exception["source"].as_str().unwrap();
        let identity = exception["destination_identity"].as_str().unwrap();
        let pair = (source.to_string(), identity.to_string());
        if !seen.insert(pair.clone()) {
            return Err(format!("duplicate allowlist entry {pair:?}"));
        }
        if gaps.remove(&pair).is_none() {
            errors.push(format!("stale allowlist: {source} -> {identity}"));
        }
    }
    errors.extend(gaps.into_iter().map(|((source, identity), sinks)| {
        format!(
            "anonymous String producer {source} -> {identity}; identified destinations: {sinks:?}"
        )
    }));
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn sinks(leaves: &[Value], manifest: &Value) -> Vec<Value> {
    leaves
        .iter()
        .filter(|leaf| leaf["writable"] == true && !leaf["identity"].is_null())
        .cloned()
        .chain(manifest["inputs"].as_array().unwrap().iter().cloned())
        .collect()
}

#[test]
fn pinned_manifest_is_complete_and_has_provenance() {
    verify_manifest_hashes(&raw_manifest()).unwrap();
    let manifest = manifest();
    assert_eq!(manifest["format"], 2);
    assert_eq!(
        manifest["source"]["commit"],
        "a1bed434e6307671467c3f89afddf4202feedf74"
    );
    assert_eq!(
        manifest["source"]["carina_revision"],
        "ea66ae7a157bd6e28f374f80286a29eb8d77a557"
    );
    let mut seen = BTreeSet::new();
    for leaf in manifest["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .chain(manifest["outputs"].as_array().unwrap())
    {
        assert_eq!(leaf["owner"], "awscc");
        assert!(leaf["identity"].is_string() && leaf["definition"].is_object());
        assert_eq!(
            leaf["service"],
            leaf["resource"]
                .as_str()
                .unwrap()
                .split('.')
                .next()
                .unwrap()
        );
        assert!(seen.insert(leaf.to_string()), "duplicate leaf {leaf}");
    }
    assert!(
        manifest["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|leaf| leaf["writable"] == true)
    );
    assert!(
        manifest["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|leaf| leaf["writable"] == false)
    );
    assert_eq!(
        seen.len(),
        1205,
        "refresh the pinned inventory deliberately when awscc changes"
    );
}

#[test]
fn manifest_hashes_detect_inventory_hand_edits() {
    let mut edited = raw_manifest();
    edited["inputs"][0]["path"] = json!("hand_edited");
    assert!(
        verify_manifest_hashes(&edited).is_err(),
        "an input hand edit must invalidate the manifest"
    );

    let mut edited = raw_manifest();
    edited["definitions"]["aws.logs.LogGroup.LogGroupName"]["length"] = json!([1, 511]);
    assert!(
        verify_manifest_hashes(&edited).is_err(),
        "a definition hand edit must invalidate the manifest"
    );
}

#[test]
fn aliases_require_review_metadata() {
    let original = aliases();
    for field in ["reason", "evidence", "review_when"] {
        for replacement in [None, Some(json!(null)), Some(json!("")), Some(json!(" \t"))] {
            let mut edited = original.clone();
            if let Some(value) = replacement {
                edited[0][field] = value;
            } else {
                edited[0].as_object_mut().unwrap().remove(field);
            }
            let error = reviewed_aliases(edited).unwrap_err();
            assert!(error.contains(field), "{error}");
        }
    }
}

#[test]
fn candidate_matching_uses_reviewed_alias_data() {
    for (resource, service, path, destination_service, destination_property, unaliased_property) in [
        (
            "ec2.FlowLog",
            "ec2",
            "log_group_name",
            "logs",
            "log_group_name",
            "log_group_name",
        ),
        ("iam.Roles", "iam", "arns[]", "iam", "role_arn", "roles_arn"),
        (
            "iam.Roles",
            "iam",
            "names[]",
            "iam",
            "role_name",
            "roles_name",
        ),
    ] {
        let leaf = json!({"resource": resource, "service": service, "path": path});
        let mut aliases = aliases();
        assert_eq!(
            key(&leaf, &aliases),
            (destination_service.into(), destination_property.into())
        );
        aliases.retain(|alias| alias["resource"] != resource);
        assert_eq!(
            key(&leaf, &aliases),
            (service.into(), unaliased_property.into())
        );
    }
}

#[test]
fn differences_require_review_metadata() {
    let original: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/awscc-identity-differences.json")).unwrap();
    for field in ["reason", "evidence", "review_when"] {
        for replacement in [
            Some(json!(" \t")),
            None,
            Some(json!(null)),
            Some(json!("")),
            Some(json!(42)),
        ] {
            let mut edited = original.clone();
            if let Some(value) = replacement {
                edited[0][field] = value;
            } else {
                edited[0].as_object_mut().unwrap().remove(field);
            }
            let error = expected_identity_differences(&edited).unwrap_err();
            assert!(error.contains(field), "{error}");
        }
    }
}

#[test]
fn duplicate_allowlist_entries_return_errors() {
    let leaves = leaves();
    let sinks = sinks(&leaves, &manifest());
    let mut exceptions = allowlist();
    exceptions.push(exceptions[0].clone());
    let error = check(&leaves, &sinks, &exceptions).unwrap_err();
    assert!(error.contains("duplicate allowlist entry"), "{error}");
}

#[test]
fn allowlist_requires_nonblank_review_metadata() {
    let leaves = leaves();
    let sinks = sinks(&leaves, &manifest());
    for field in ["reason", "evidence", "issue", "review_when", "expected"] {
        for replacement in [Some(json!(" \t")), None, Some(json!(null)), Some(json!(""))] {
            let mut edited = allowlist();
            if let Some(value) = replacement {
                edited[0][field] = value;
            } else {
                edited[0].as_object_mut().unwrap().remove(field);
            }
            let error = check(&leaves, &sinks, &edited).unwrap_err();
            assert!(error.contains(field), "{error}");
        }
    }
}

#[test]
fn allowlist_expected_categories_are_closed() {
    let leaves = leaves();
    let sinks = sinks(&leaves, &manifest());
    for expected in ["unreviewed_category", "no_semantic_producer_match "] {
        let mut edited = allowlist();
        edited[0]["expected"] = json!(expected);
        let error = check(&leaves, &sinks, &edited).unwrap_err();
        assert!(
            error.contains("expected") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn stale_alias_selectors_are_rejected() {
    for (field, value) in [
        ("resource", "ec2.DeletedResource"),
        ("property", "deleted_log_group_name"),
    ] {
        let mut edited = aliases();
        edited[0][field] = json!(value);
        let error = reviewed_aliases(edited).unwrap_err();
        assert!(
            error.contains("stale alias") && error.contains(value),
            "{error}"
        );
    }
}

#[test]
fn schema_inventory_producer_obligations() {
    let leaves = leaves();
    check(&leaves, &sinks(&leaves, &manifest()), &allowlist())
        .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn new_manifest_input_creates_an_obligation_and_stale_allowlist_fails() {
    let leaves = leaves();
    let mut sink = leaves
        .iter()
        .find(|leaf| source(leaf) == "aws.iam.Role/resource/path")
        .unwrap()
        .clone();
    sink["owner"] = json!("awscc");
    sink["identity"] = json!("aws.iam.Role.Path");
    let mut manifest = manifest();
    manifest["inputs"].as_array_mut().unwrap().push(sink);
    let error = check(&leaves, &sinks(&leaves, &manifest), &allowlist()).unwrap_err();
    assert!(
        error.contains("aws.iam.Role/resource/path")
            && error.contains("awscc.iam.Role/resource/path"),
        "{error}"
    );
    let exception = json!({"source": "deleted", "destination_identity": "deleted",
        "reason": "fixture", "evidence": "fixture", "issue": "carina-rs/carina-provider-aws#700", "review_when": "fixture", "expected": "no_semantic_producer_match"});
    assert!(
        check(&leaves, &[], &[exception])
            .unwrap_err()
            .contains("stale allowlist")
    );
}

#[test]
fn aggregate_identity_definitions_agree_including_helpers_and_echoes() {
    let configs = generated::configs();
    // Admit all actual resources together, plus catalog/helper declarations.
    // The native provider test additionally exercises provider_schema_witness()
    // with the real provider configuration and assignability edges.
    let mut types = BTreeMap::new();
    for row in carina_aws_types::AwsType::ALL {
        types.insert(row.rust_variant_name().to_string(), row.attribute_type());
    }
    for (name, helper) in [
        ("role_arn_helper", generated::iam::role::arn()),
        (
            "account_arn_helper",
            generated::organizations::account::arn(),
        ),
        (
            "organization_arn_helper",
            generated::organizations::organization::arn(),
        ),
        (
            "bucket_read_arn_helper",
            generated::s3::bucket_data_source::arn(),
        ),
        ("caller_arn_helper", generated::sts::caller_identity::arn()),
        ("policy_arn_helper", generated::iam::policy::arn()),
        ("key_arn_helper", generated::kms::key::arn()),
        ("key_id_helper", generated::kms::key::id()),
    ] {
        types.insert(name.into(), helper);
    }
    ValidatedProviderSchemaWitness::new(
        configs
            .into_iter()
            .map(|config| config.schema.validate().unwrap())
            .collect(),
        ValidatedConfigSchema::validate(types).unwrap(),
        Default::default(),
        vec![],
    )
    .expect("all schemas, shared definitions, helpers and data-source echoes must agree");

    let mut definitions = BTreeMap::new();
    for config in generated::configs() {
        let mut visit = |path: &str, _: bool, ty: &carina_core::schema::AttributeType| {
            if matches!(
                ty.raw_shape(),
                RawShape::String { .. } | RawShape::Reference { .. }
            ) && let Some((identity, definition)) = inventory::scalar_definition(ty)
            {
                let definition = inventory::portable_definition(&definition);
                if let Some((previous, first)) = definitions.insert(
                    identity.clone(),
                    (
                        definition.clone(),
                        format!("{}:{path}", config.resource_type_name),
                    ),
                ) {
                    assert_eq!(
                        previous, definition,
                        "{identity}: {first} vs {}:{path}",
                        config.resource_type_name
                    );
                }
            }
        };
        for attribute in config.schema.attributes().values() {
            inventory::walk(
                attribute.attr_type(),
                config.schema.defs(),
                attribute.name(),
                false,
                &mut BTreeSet::new(),
                &mut visit,
            );
        }
        for (name, definition) in config.schema.defs() {
            inventory::walk(
                definition,
                config.schema.defs(),
                &format!("@def/{name}"),
                false,
                &mut BTreeSet::new(),
                &mut visit,
            );
        }
    }
}

#[test]
fn cross_owner_identity_definitions_agree_with_pinned_awscc() {
    let manifest = manifest();
    let mut mismatches = BTreeSet::new();
    let mut comparisons = 0;
    let mut declarations = leaves();
    declarations.extend(carina_aws_types::AwsType::ALL.iter().filter_map(|row| {
        inventory::scalar_definition(&row.attribute_type())
            .map(|(identity, definition)| json!({"identity": identity, "definition": definition}))
    }));
    for leaf in declarations.into_iter().filter(|leaf| {
        matches!(
            leaf["definition"]["shape"].as_str(),
            Some("string" | "reference")
        )
    }) {
        for counterpart in manifest["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .chain(manifest["outputs"].as_array().unwrap())
            .filter(|other| other["identity"] == leaf["identity"])
        {
            comparisons += 1;
            if inventory::portable_definition(&leaf["definition"])
                != inventory::portable_definition(&counterpart["definition"])
            {
                mismatches.insert(
                    json!({"identity": leaf["identity"],
                    "aws": inventory::portable_definition(&leaf["definition"]),
                    "awscc": inventory::portable_definition(&counterpart["definition"])})
                    .to_string(),
                );
            }
        }
    }
    assert!(comparisons > 0);
    let reviewed: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/awscc-identity-differences.json")).unwrap();
    let expected =
        expected_identity_differences(&reviewed).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        mismatches, expected,
        "cross-owner differences require review"
    );
}
