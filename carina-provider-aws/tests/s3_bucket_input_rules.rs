use std::collections::HashMap;

use carina_core::effect::Effect;
use carina_core::input_rules::{InputWriteContext, finalize_plan_for_test};
use carina_core::plan::{Plan, PlanErrorKind};
use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{
    ConcreteValue, Current, DataSource, ResolvedDataSource, ResolvedResource, Resource, Value,
};
use carina_core::schema::{InputRuleScope, SchemaKind, SchemaRegistry, ValidatedResourceSchema};

fn schemas() -> [ValidatedResourceSchema; 2] {
    let schema = carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .find(|schema| {
            schema.parts().resource_type == "s3.Bucket"
                && schema.as_schema().kind() == SchemaKind::Resource
        })
        .unwrap();
    let wire = protocol_to_resource_schema(&resource_schema_to_protocol(&schema).unwrap()).unwrap();
    [schema, wire]
}

fn registry(schema: ValidatedResourceSchema) -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.insert("aws", schema);
    registry
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn bucket(name: &str, registry: &SchemaRegistry) -> ResolvedResource {
    ResolvedResource::resolve_raw_fixture(
        Resource::with_provider("aws", "s3.Bucket", "probe", None)
            .with_attribute("bucket", string(name)),
        registry,
    )
    .unwrap()
}

fn create(name: &str, registry: &SchemaRegistry) -> Plan {
    let mut plan = Plan::new();
    plan.add(Effect::Create(bucket(name, registry)));
    plan
}

fn rejects_create(name: &str, code: &str) {
    for schema in schemas() {
        let registry = registry(schema);
        let rejected = finalize_plan_for_test(create(name, &registry))
            .check_inputs(&registry, &HashMap::new())
            .expect_err("S3 must reject the name before creating the bucket");
        assert!(
            rejected.errors().iter().any(|error| {
                matches!(&error.kind, PlanErrorKind::InputRule {
                    path, code: actual, context: InputWriteContext::Create, ..
                } if path == "bucket" && actual == code)
            }),
            "{name:?}: {rejected:?}"
        );
        let accepted = finalize_plan_for_test(create("carina-probe-ok", &registry))
            .check_inputs(&registry, &HashMap::new())
            .expect("valid control must pass every rule");
        assert!(accepted.pending_inputs().is_empty());
    }
}

macro_rules! rejected_name {
    ($test:ident, $name:expr, $code:literal) => {
        #[test]
        fn $test() {
            rejects_create($name, concat!("s3.bucket.", $code));
        }
    };
}

rejected_name!(too_short, "ab", "length");
rejected_name!(too_long, &"a".repeat(64), "length");
rejected_name!(uppercase, "My-Bucket", "allowed_chars");
rejected_name!(underscore, "my_bucket", "allowed_chars");
rejected_name!(first_character, "-bucket", "first_char");
rejected_name!(last_character, "bucket-", "last_char");
rejected_name!(adjacent_dots, "my..bucket", "no_adjacent_dots");
rejected_name!(ipv4, "192.168.1.1", "not_ipv4");
rejected_name!(punycode_prefix, "xn--bucket", "no_prefix_xn");
rejected_name!(sthree_prefix, "sthree-bucket", "no_prefix_sthree");
rejected_name!(demo_prefix, "amzn-s3-demo-bucket", "no_prefix_amzn_s3_demo");
rejected_name!(alias_suffix, "bucket-s3alias", "no_suffix_s3alias");
rejected_name!(object_lambda_suffix, "bucket--ol-s3", "no_suffix_ol_s3");
rejected_name!(mrap_suffix, "bucket.mrap", "no_suffix_mrap");
rejected_name!(directory_suffix, "bucket--x-s3", "no_suffix_x_s3");
rejected_name!(table_suffix, "bucket--table-s3", "no_suffix_table_s3");
rejected_name!(legacy_create, "Legacy_Bucket", "allowed_chars");

#[test]
fn valid_boundaries_and_account_regional_suffix_are_not_rejected() {
    for schema in schemas() {
        let registry = registry(schema);
        for name in ["abc", &"a".repeat(63), "a.b-c0", "carina-probe-ok-an"] {
            finalize_plan_for_test(create(name, &registry))
                .check_inputs(&registry, &HashMap::new())
                .expect("valid names and the namespace-dependent -an suffix must pass");
        }
    }
}

#[test]
fn legacy_import_and_no_change_plan_are_exempt() {
    for schema in schemas() {
        assert!(
            schema
                .as_schema()
                .input_rules()
                .iter()
                .all(|rule| rule.scope() == InputRuleScope::CreateOnly)
        );
        let registry = registry(schema);
        let resource = bucket("Legacy_Bucket", &registry);
        let mut import = Plan::new();
        import.add(Effect::Import {
            id: resource.resolved_id().clone(),
            identifier: string("Legacy_Bucket"),
        });
        finalize_plan_for_test(import)
            .check_inputs(&registry, &HashMap::new())
            .expect("import must accept a legacy bucket name");

        let state = carina_provider_aws::test_existing_state(
            resource.id.clone(),
            resource.attributes.clone().into_iter().collect(),
        )
        .with_identifier("Legacy_Bucket");
        let current = HashMap::from([(resource.id.clone(), Current::from_state(state))]);
        finalize_plan_for_test(Plan::new())
            .check_inputs(&registry, &current)
            .expect("a no-change plan must accept an existing legacy bucket");
    }
}

#[test]
fn legacy_data_source_read_is_exempt() {
    let mut registry = registry(schemas().into_iter().next().unwrap());
    let schema = carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .find(|schema| {
            schema.parts().resource_type == "s3.Bucket"
                && schema.as_schema().kind() == SchemaKind::DataSource
        })
        .unwrap();
    registry.insert("aws", schema);
    let resource = ResolvedDataSource::collect_resolved([DataSource::with_provider(
        "aws",
        "s3.Bucket",
        "legacy",
        None,
    )
    .with_attribute("bucket", string("Legacy_Bucket"))])
    .unwrap()
    .remove(0);
    let mut plan = Plan::new();
    plan.add(Effect::Read { resource });
    finalize_plan_for_test(plan)
        .check_inputs(&registry, &HashMap::new())
        .expect("reading a legacy bucket must not apply creation rules");
}

#[test]
fn directory_fixture_defers_creation_rules_until_plan() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        std::sync::Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let registry = snapshot.schema_registry();
    let directory = carina_core::paths::ResolvedPath::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/s3-bucket-input-rules"),
    )
    .unwrap();
    let parsed = carina_core::config_loader::parse_directory(&directory, context).unwrap();
    assert_eq!(parsed.providers.len(), 1);
    assert_eq!(parsed.resources.len(), 1);
    carina_core::validation::validate_resources(
        parsed.parsed(),
        registry,
        &std::collections::HashSet::from(["aws".into()]),
        context,
        &mut Vec::new(),
    )
    .unwrap();
    // Authored CreateOnly previews warn before state is known; the finalized
    // create plan below still rejects the same invalid bucket name.
    let report = carina_core::input_rules::check_authored_inputs(
        parsed.parsed().authored_inputs().unwrap(),
        registry,
    );
    assert!(report.preparation_errors().is_empty());
    assert!(!report.findings().is_empty());
    assert!(report.findings().iter().all(
        |finding| finding.severity() == carina_core::input_rules::InputFindingSeverity::Warning
    ));
    let Value::Concrete(ConcreteValue::String(name)) = &parsed.resources[0].attributes["bucket"]
    else {
        panic!("fixture must provide a literal bucket name");
    };
    let rejected = finalize_plan_for_test(create(name.expose_plaintext(), registry))
        .check_inputs(registry, &HashMap::new())
        .expect_err("the directory's new bucket must fail at plan");
    assert!(rejected.errors().iter().any(|error| {
        matches!(&error.kind, PlanErrorKind::InputRule { path, code, .. }
            if path == "bucket" && code == "s3.bucket.allowed_chars")
    }));
}

#[test]
fn example_and_acceptance_create_names_pass_creation_rules() {
    let registry = registry(schemas().into_iter().next().unwrap());
    let bucket_name =
        regex::Regex::new(r#"(?s)\baws\.s3\.Bucket\s*\{\s*bucket\s*=\s*['"]([^'"]+)['"]"#).unwrap();
    let variable = regex::Regex::new(r"\$\{([A-Z_]+)\}").unwrap();
    let assignment = regex::Regex::new(r#"(?m)^([A-Z_]+)="([^"]+)"$"#).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for directory in ["examples", "acceptance-tests"] {
        let mut pending = vec![root.join(directory)];
        let mut checked = 0;
        while let Some(path) = pending.pop() {
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "crn") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for capture in bucket_name.captures_iter(&source) {
                if source[..capture.get(0).unwrap().start()]
                    .trim_end()
                    .ends_with("read")
                {
                    continue;
                }
                let mut name = capture[1].to_string();
                if variable.is_match(&name) {
                    // Acceptance runners interpolate names from these assignments;
                    // exercise their actual prefixes with a representative date suffix.
                    let runner =
                        std::fs::read_to_string(path.parent().unwrap().join("tests/basic.sh"))
                            .unwrap();
                    let variables: HashMap<_, _> = assignment
                        .captures_iter(&runner)
                        .map(|capture| (capture[1].to_string(), capture[2].to_string()))
                        .collect();
                    name = variable
                        .replace_all(&name, |capture: &regex::Captures<'_>| {
                            variables[&capture[1]].replace("${SUFFIX}", "202611080244")
                        })
                        .into_owned();
                }
                finalize_plan_for_test(create(&name, &registry))
                    .check_inputs(&registry, &HashMap::new())
                    .unwrap_or_else(|error| panic!("{}: {name}: {error:?}", path.display()));
                checked += 1;
            }
        }
        assert!(checked > 0, "must exercise {directory} bucket names");
    }
}
