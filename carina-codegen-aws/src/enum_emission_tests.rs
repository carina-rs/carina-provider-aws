//! Evaluate emitted schemas and compare every enum with docgen's derived types.
use super::*;
use std::collections::BTreeSet;

type Aliases = BTreeMap<String, Vec<(String, String)>>;

include!("../tests/support/enum_aliases.rs");

#[test]
fn every_generated_enum_has_the_docgen_aliases() {
    let models = carina_codegen_aws::smithy_model_dir();
    let mut resources = resource_defs::ec2_resources();
    resources.extend(resource_defs::s3_resources());
    resources.extend(resource_defs::sts_resources());
    resources.extend(resource_defs::organizations_resources());
    resources.extend(resource_defs::route53_resources());
    resources.extend(resource_defs::iam_resources());
    resources.extend(resource_defs::logs_resources());
    resources.extend(resource_defs::acm_resources());
    resources.extend(resource_defs::sqs_resources());
    let mut sources = resource_defs::sts_data_sources();
    sources.extend(resource_defs::identitystore_data_sources());
    sources.extend(resource_defs::s3_data_sources());
    sources.extend(resource_defs::iam_data_sources());
    let namespaces = resources
        .iter()
        .flat_map(|resource| {
            std::iter::once(resource.service_namespace).chain(
                resource
                    .update_ops
                    .iter()
                    .filter_map(|operation| operation.fields.cross_service_namespace()),
            )
        })
        .chain(sources.iter().map(|source| source.service_namespace))
        .collect::<BTreeSet<_>>();
    for namespace in namespaces {
        let service = namespace
            .strip_prefix("com.amazonaws.")
            .unwrap_or(namespace);
        let path = models.join(format!("{service}.json"));
        if !path.is_file() {
            eprintln!(
                "Skipping enum alias parity: Smithy model not cached: {}. Run scripts/download-smithy-models.sh to enable this test.",
                path.display()
            );
            return;
        }
    }
    let mut expected = Aliases::new();
    for resource in &resources {
        let model = load_model(&models, resource.service_namespace).unwrap();
        for attr in collect_resource_attributes(resource, &model).unwrap().attrs {
            aliases(
                &attr.attribute_type,
                &format!("{} [Resource].{}", resource.name, attr.snake_name),
                &mut expected,
            );
        }
    }
    for source in sources {
        let model = load_model(&models, source.service_namespace).unwrap();
        let mut add = |name: &str, provider: &str, declaration: &DataSourceType| {
            let value = data_source_type(&model, source.name, name, provider, declaration).unwrap();
            aliases(
                &value.attribute_type,
                &format!("{} [DataSource].{name}", source.name),
                &mut expected,
            );
        };
        for input in &source.inputs {
            add(
                input.name,
                input.provider_name,
                &DataSourceType {
                    representation: input.representation,
                    decoration: input.type_override.clone(),
                },
            );
        }
        match &source.shape {
            resource_defs::DataSourceShape::Single { output_attributes } => {
                for output in output_attributes {
                    if !source.inputs.iter().any(|input| input.name == output.name) {
                        add(
                            output.name,
                            output.provider_name.unwrap_or(""),
                            &output.type_code,
                        );
                    }
                }
            }
            resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => {
                for output in aggregated_outputs {
                    add(&format!("{}[]", output.name), "", &output.item_type);
                }
            }
        }
    }
    assert!(
        expected.len() >= 88,
        "must cover every current enum position"
    );
    let temp = test_support::build_tempdir();
    let generated = temp.path().join("schemas");
    run(ResolvedArgs {
        model_dir: models,
        output_dir: generated.clone(),
        resource: None,
        output_mode: OutputMode::Rust,
    })
    .unwrap();
    let source = temp.path().join("aliases.rs");
    let executable = temp.path().join("aliases");
    let module = generated.join("mod.rs");
    let expected = serde_json::to_string(&expected).unwrap();
    let walk = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/enum_aliases.rs");
    std::fs::write(&source,format!(r#"#![allow(dead_code,unused_imports)]
use carina_core::schema::{{AttributeType,RawShape}};
pub mod schemas {{pub mod config {{pub use carina_aws_types::*;
pub struct ReadBackRequirement;impl ReadBackRequirement{{pub fn new(_: &'static str,_:ReadBackPredicate)->Self{{Self}}}}
pub struct AwsSchemaConfig{{pub aws_type_name:&'static str,pub resource_type_name:&'static str,pub has_tags:bool,pub await_on_read_back:Vec<ReadBackRequirement>,pub schema:carina_core::schema::ResourceSchema}}
}} #[path={module:?}] pub mod generated;}}
include!({walk:?});
fn main(){{let mut actual=std::collections::BTreeMap::new();
for config in schemas::generated::configs(){{for attr in config.schema.attributes().values(){{aliases(attr.attr_type(),&format!("{{}} [{{:?}}].{{}}",config.schema.resource_type(),config.schema.kind(),attr.name()),&mut actual);}}}}
let expected:std::collections::BTreeMap<String,Vec<(String,String)>>=serde_json::from_str({expected:?}).unwrap();assert_eq!(actual,expected);
}}"#)).unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut compiler = std::process::Command::new("rustc");
    compiler
        .args(["--edition=2024", "-L"])
        .arg(format!("dependency={}", deps.display()));
    for name in [
        "carina_core",
        "carina_aws_types",
        "carina_provider_protocol",
        "serde_json",
    ] {
        compiler.arg("--extern").arg(format!(
            "{name}={}",
            test_support::linked_library(name).display()
        ));
    }
    let output = compiler
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
