//! #700: an offline schema inventory and obligations created by identified inputs.
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

/// Candidates use service/property semantics, never a blanket *_id/*_name rule.
/// Cross-service aliases below are the reviewed semantic mappings from #700.
fn key(leaf: &Value) -> (String, String) {
    let resource = leaf["resource"].as_str().unwrap();
    let service = leaf["service"].as_str().unwrap();
    let path = leaf["path"].as_str().unwrap();
    let property = path.rsplit('.').next().unwrap().replace("[]", "");
    match (resource, property.as_str()) {
        ("ec2.FlowLog", "log_group_name") => return ("logs".into(), property),
        ("identitystore.User", "user_id") => return ("sso".into(), "principal_id".into()),
        ("organizations.Account", "id") => return ("aws".into(), "account_id".into()),
        ("sso.Assignment", "target_id") => return ("aws".into(), "account_id".into()),
        _ => {}
    }
    if property == "region" {
        return ("aws".into(), property);
    }
    if matches!(
        property.as_str(),
        "account_id" | "master_account_id" | "peer_owner_id" | "source_security_group_owner_id"
    ) {
        return ("aws".into(), "account_id".into());
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
        let name = if resource == "iam.Roles" {
            "role"
        } else {
            &name
        };
        format!("{name}_{kind}")
    } else {
        property
    };
    (service.into(), property)
}

fn gaps(leaves: &[Value], sinks: &[Value]) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut obligations = BTreeMap::<_, BTreeSet<_>>::new();
    for sink in sinks {
        let Some(identity) = sink["identity"].as_str() else {
            continue;
        };
        let sink_key = key(sink);
        for producer in leaves
            .iter()
            .filter(|leaf| leaf["anonymous_string"] == true)
        {
            if key(producer) == sink_key {
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
    let mut gaps = gaps(leaves, sinks);
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        let source = exception["source"].as_str().unwrap();
        let identity = exception["destination_identity"].as_str().unwrap();
        let pair = (source.to_string(), identity.to_string());
        assert!(
            seen.insert(pair.clone()),
            "duplicate allowlist entry {pair:?}"
        );
        for field in ["reason", "evidence", "issue", "review_when", "expected"] {
            assert!(
                !exception[field].as_str().unwrap_or_default().is_empty(),
                "{pair:?}: missing {field}"
            );
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
        "reason": "fixture", "evidence": "fixture", "issue": "#700", "review_when": "fixture", "expected": "fixture"});
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
    let expected = reviewed
        .iter()
        .map(|entry| {
            for field in ["reason", "evidence", "review_when"] {
                assert!(!entry[field].as_str().unwrap_or_default().is_empty());
            }
            json!({"identity": entry["identity"], "aws": entry["aws"], "awscc": entry["awscc"]})
                .to_string()
        })
        .collect();
    assert_eq!(
        mismatches, expected,
        "cross-owner differences require review"
    );
}
