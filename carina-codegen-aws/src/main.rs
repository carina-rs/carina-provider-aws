//! Smithy-based Code Generator for Carina AWS Provider
//!
//! Generates Rust schema code from AWS Smithy JSON AST models,
//! producing output identical to the CloudFormation-based codegen.
//!
//! Usage:
//!   smithy-codegen --model-dir <path> --output-dir <path> [--resource <name>]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use carina_smithy::{Shape, ShapeKind, SmithyModel};
use clap::Parser;
use heck::{ToSnakeCase, ToUpperCamelCase};

use carina_aws_types::dsl_enum_value;
use carina_codegen_aws::resource_defs::{self, ReplacementClass, ResourceDef};
use carina_docgen::{Heading, PageBuilder};

const PLAIN_STRING_TYPE_CODE: &str = "AttributeType::string()";

#[derive(Parser, Debug)]
#[command(name = "smithy-codegen")]
#[command(about = "Generate Carina AWS provider schema code from Smithy models")]
struct Args {
    /// Directory containing Smithy model JSON files
    #[arg(long)]
    model_dir: PathBuf,

    /// Output directory for generated Rust files
    #[arg(long)]
    output_dir: PathBuf,

    /// Generate only the specified resource (e.g., "ec2.Vpc")
    #[arg(long)]
    resource: Option<String>,

    /// Output format: rust (default) or markdown (for documentation)
    #[arg(long, default_value = "rust")]
    format: String,

    /// Workspace Clippy configuration generated with provider resend safety.
    #[arg(long)]
    clippy_config: Option<PathBuf>,

    /// Directory containing generated-doc examples (`<service>_<page>/main.crn`).
    #[arg(long)]
    examples_dir: Option<PathBuf>,
}

/// Unified type override for resource-scoped property overrides.
/// Allows overriding string type, enum values, integer range, or integer enum
/// for a specific (resource_type, property_name) pair.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum TypeOverride {
    /// Override to a specific string type (e.g., "super::super::iam::role::arn()")
    StringType(&'static str),
    /// Override to an enum with specific values
    Enum(Vec<&'static str>),
    /// Override to an integer range (min, max)
    IntRange(i64, i64),
    /// Override to an integer enum with specific allowed values
    IntEnum(Vec<i64>),
}

/// Information about a detected enum type
#[derive(Debug, Clone)]
struct EnumInfo {
    /// Type name in PascalCase (e.g., "InstanceTenancy")
    type_name: String,
    /// Valid enum values (e.g., ["default", "dedicated", "host"])
    values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArnEmitChoice {
    PerKind(&'static ArnValidation),
    ServicePrefix(&'static str),
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArnValidation {
    service: &'static str,
    resource: &'static str,
    regex: &'static str,
}

static ARN_VALIDATIONS: &[ArnValidation] = &[
    ArnValidation {
        service: "iam",
        resource: "Role",
        regex: "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$",
    },
    ArnValidation {
        service: "iam",
        resource: "Policy",
        regex: "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$",
    },
    ArnValidation {
        service: "iam",
        resource: "OidcProvider",
        regex: "^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:oidc-provider/.+$",
    },
    ArnValidation {
        service: "kms",
        resource: "Key",
        regex: "^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$",
    },
    ArnValidation {
        service: "s3",
        resource: "Bucket",
        regex: "^arn:(aws|aws-cn|aws-us-gov):s3:::.+$",
    },
    ArnValidation {
        service: "cloudfront",
        resource: "Distribution",
        regex: "^arn:(aws|aws-cn|aws-us-gov):cloudfront::[^:]*:distribution/.+$",
    },
    ArnValidation {
        service: "ecs",
        resource: "Cluster",
        regex: "^arn:(aws|aws-cn|aws-us-gov):ecs:[^:]*:[^:]*:cluster/.+$",
    },
    ArnValidation {
        service: "dynamodb",
        resource: "Table",
        regex: "^arn:(aws|aws-cn|aws-us-gov):dynamodb:[^:]*:[^:]*:table/.+$",
    },
    ArnValidation {
        service: "logs",
        resource: "LogGroup",
        regex: "^arn:(aws|aws-cn|aws-us-gov):logs:[^:]*:[^:]*:log-group:.+$",
    },
];

static KNOWN_SERVICES: &[&str] = &[
    "cloudfront",
    "dynamodb",
    "ec2",
    "ecs",
    "iam",
    "kms",
    "logs",
    "organizations",
    "s3",
    "sts",
    "wafv2",
];

fn arn_emit_choice(service: &str, resource: &str) -> ArnEmitChoice {
    if let Some(entry) = ARN_VALIDATIONS
        .iter()
        .find(|v| v.service == service && v.resource == resource)
    {
        ArnEmitChoice::PerKind(entry)
    } else if let Some(&known) = KNOWN_SERVICES.iter().find(|&&known| known == service) {
        ArnEmitChoice::ServicePrefix(known)
    } else {
        ArnEmitChoice::Generic
    }
}

fn resource_identity_parts(name: &str) -> Option<(&str, &str)> {
    name.split_once('.')
}

fn shared_arn_constructor(service: &str, resource: &str) -> Option<&'static str> {
    match (service, resource) {
        ("iam", "Role") => Some("iam_role_arn"),
        ("iam", "Policy") => Some("iam_policy_arn"),
        ("iam", "OidcProvider") => Some("iam_oidc_provider_arn"),
        ("kms", "Key") => Some("kms_key_arn"),
        ("organizations", "Account") => Some("organizations_account_arn"),
        ("organizations", "Organization") => Some("organizations_organization_arn"),
        ("s3", "Bucket") => Some("s3_bucket_arn"),
        ("sts", "CallerIdentity") => Some("sts_caller_identity_arn"),
        _ => None,
    }
}

fn emit_arn_helper(service: &str, resource: &str, choice: ArnEmitChoice) -> String {
    if let Some(constructor) = shared_arn_constructor(service, resource) {
        return format!("pub fn arn() -> AttributeType {{\n    super::{constructor}()\n}}\n\n");
    }

    if matches!(choice, ArnEmitChoice::Generic) {
        return "pub fn arn() -> AttributeType {\n    super::arn()\n}\n\n".to_string();
    }

    let regex_expr = match choice {
        ArnEmitChoice::PerKind(entry) => format!("Some({:?}.to_string())", entry.regex),
        ArnEmitChoice::ServicePrefix(service) => {
            format!(
                "Some(\"^arn:(aws|aws-cn|aws-us-gov):{}:.*$\".to_string())",
                service
            )
        }
        ArnEmitChoice::Generic => unreachable!("handled above"),
    };
    format!(
        r#"pub fn arn() -> AttributeType {{
    AttributeType::refined_string(
        Some(super::provider_type("{service}", "{resource}", "Arn")),
        {regex_expr},
        None,
        None,
    )
}}

"#
    )
}

/// Build the `dsl_aliases: ...` Rust source code for a `StringEnum`'s
/// alias table, following naming-conventions design D7.
///
/// Per #247 / D3 / D7, every enum value gets a snake_case DSL spelling and
/// the canonical AWS spelling is preserved for the underlying SDK call.
/// This helper emits a `Vec<(String, String)>` literal mapping each
/// canonical (API) value to its DSL spelling, layered with any explicit
/// aliases configured via `ResourceDef::enum_aliases`.
///
/// Data form (rather than a `fn` closure) is required so the alias map
/// survives the WASM-component boundary — see carina#2832 / aws#247.
///
/// Returns Rust source that evaluates to `vec![("api","dsl"), ...]` or
/// `vec![]` when no value needs translation.
fn build_dsl_aliases_code(
    values: &[String],
    explicit_aliases: Option<&Vec<(&str, &str)>>,
) -> String {
    // Collect (canonical, dsl_form) pairs. Explicit aliases take precedence
    // over the auto-derived `dsl_enum_value` form.
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut seen_canonical: HashSet<String> = HashSet::new();

    if let Some(aliases) = explicit_aliases {
        for (canonical, alias) in aliases {
            if seen_canonical.insert(canonical.to_string()) {
                pairs.push((canonical.to_string(), alias.to_string()));
            }
        }
    }
    for value in values {
        if seen_canonical.insert(value.clone()) {
            // Emit every value, including identity rows where the DSL
            // spelling equals the API spelling. The exhaustive table
            // is what makes the carina-core strict-DSL validator (see
            // `carina-rs/carina#2980`) treat the whole enum uniformly.
            let dsl_form = dsl_enum_value(value);
            pairs.push((value.clone(), dsl_form));
        }
    }

    if pairs.is_empty() {
        return "vec![]".to_string();
    }

    let entries: Vec<String> = pairs
        .iter()
        .map(|(canonical, dsl)| format!("(\"{}\".to_string(), \"{}\".to_string())", canonical, dsl))
        .collect();
    format!("vec![{}]", entries.join(", "))
}

/// Information about an attribute to generate
#[derive(Debug, Clone)]
struct AttrInfo {
    /// Snake_case attribute name (e.g., "cidr_block")
    snake_name: String,
    /// Smithy provider name (usually PascalCase, but sometimes lowerCamelCase)
    provider_name: String,
    /// Rust code for the attribute type
    type_code: String,
    /// Whether the field is required
    is_required: bool,
    /// Whether the field is create-only
    is_create_only: bool,
    /// Whether the field is read-only
    is_read_only: bool,
    /// Whether core must preserve the desired value because AWS never returns
    /// it from read APIs.
    is_write_only: bool,
    /// Whether the generated read model requires a canonical value or permits
    /// an absent AWS value.
    read_presence: resource_defs::ReadPresence,
    /// Reason attached to write-only/read overrides for generated comments.
    read_reason: Option<String>,
    /// Whether the underlying Smithy member target is a plain string shape.
    /// This is captured before rendered type overrides are applied.
    is_plain_string: bool,
    /// Whether the field contributes to anonymous resource identity hashing
    is_identity: bool,
    /// Whether the AWS API populates this field asynchronously after Create
    /// (carina#3034). Validate-time check rejects chained references to
    /// such a field unless the user has declared a `wait` block.
    is_deferred_populate: bool,
    /// Description from Smithy docs
    description: Option<String>,
    /// Enum info if this attribute is an enum
    enum_info: Option<EnumInfo>,
}

/// Select the one typestate constructor represented by a source declaration.
fn mode_constructor(required: bool, read_only: bool) -> &'static str {
    match (required, read_only) {
        (_, true) => "provider_populated",
        (true, false) => "required",
        (false, false) => "optional",
    }
}

fn is_plain_smithy_string(model: &SmithyModel, target: &str) -> bool {
    match model.get_shape(target) {
        Some(Shape::String(shape)) => !shape.traits.contains_key("smithy.api#enum"),
        None => target == "smithy.api#String",
        _ => false,
    }
}

fn emit_replacement_builder(
    code: &mut String,
    resource_type_name: &str,
    replacement: ReplacementClass,
    attrs: &[AttrInfo],
) -> Result<()> {
    match replacement {
        ReplacementClass::UniqueName(provider_name) => {
            let provider_name_snake = provider_name.to_snake_case();
            let attr = attrs
                .iter()
                .find(|attr| attr.provider_name.to_snake_case() == provider_name_snake);
            let attr = if attrs.iter().any(|attr| attr.is_identity) {
                Err("on a resource with a composite identity")
            } else {
                match attr {
                    None => Err("missing"),
                    Some(attr) if attr.is_read_only => Err("read-only"),
                    Some(attr)
                        if !attr.is_plain_string || attr.type_code != PLAIN_STRING_TYPE_CODE =>
                    {
                        Err("not a plain string")
                    }
                    Some(attr) => Ok(attr),
                }
            };
            let attr = match attr {
                Ok(attr) => attr,
                Err(reason) => anyhow::bail!(
                    "{} replacement unique-name attribute {} is {}",
                    resource_type_name,
                    provider_name,
                    reason
                ),
            };
            code.push_str(&format!(
                "\x20       .with_unique_name_attribute(\"{}\")\n",
                attr.snake_name
            ));
        }
        ReplacementClass::Coexisting => {
            code.push_str("\x20       .with_coexisting_replacement()\n");
        }
        ReplacementClass::Conflicting => {
            code.push_str("\x20       .with_conflicting_replacement()\n");
        }
    }

    Ok(())
}

/// Integer range constraint (supports one-sided ranges)
#[derive(Debug, Clone, Copy)]
struct IntRange {
    min: Option<i64>,
    max: Option<i64>,
}

/// Convert a PascalCase token (e.g., `"SecurityGroupIngress"`) to snake_case
/// (`"security_group_ingress"`) for use in Rust module names and identifiers.
fn pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Convert a DSL resource name to a Rust module name.
/// e.g., "ec2.Vpc" -> "ec2_vpc", "ec2.SecurityGroupIngress" -> "ec2_security_group_ingress".
/// The DSL name uses PascalCase for the final segment per the naming-conventions
/// rule (design D2); Rust modules stay snake_case.
fn module_name(name: &str) -> String {
    let (service, resource) = split_service_resource(name);
    format!("{}_{}", service, pascal_to_snake(resource))
}

/// Split a DSL resource name into (service, resource).
/// e.g., "ec2.Vpc" -> ("ec2", "Vpc"), "s3.Bucket" -> ("s3", "Bucket").
/// The final segment is PascalCase per design D2; callers that need a
/// snake_case form should apply `pascal_to_snake` to the second element.
fn split_service_resource(name: &str) -> (&str, &str) {
    name.split_once('.').expect("DSL name must contain '.'")
}

/// Split a DSL resource name and snake_case the final segment.
/// Returns (service, snake_resource) suitable for Rust module paths,
/// file names, and identifier generation.
fn split_service_resource_snake(name: &str) -> (&str, String) {
    let (service, resource) = split_service_resource(name);
    (service, pascal_to_snake(resource))
}

/// Build a markdown filename whose suffix unambiguously identifies its kind.
fn markdown_file_name(resource: &str, data_source: bool) -> String {
    let suffix = if data_source { "_data_source" } else { "" };
    format!("{}{suffix}.md", pascal_to_snake(resource))
}

fn load_markdown_example(
    examples_dir: Option<&Path>,
    service: &str,
    page_stem: &str,
) -> Result<Option<String>> {
    let Some(examples_dir) = examples_dir else {
        return Ok(None);
    };
    let path = examples_dir
        .join(format!("{service}_{page_stem}"))
        .join("main.crn");
    match std::fs::read_to_string(&path) {
        Ok(source) => Ok(Some(preprocess_markdown_example(&source))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

fn validate_examples_dir(examples_dir: Option<&Path>) -> Result<()> {
    let Some(examples_dir) = examples_dir else {
        return Ok(());
    };
    if !examples_dir.is_dir() {
        anyhow::bail!(
            "--examples-dir must be an existing directory: {}",
            examples_dir.display()
        );
    }
    Ok(())
}

/// Escape Rust reserved keywords with the raw identifier prefix `r#`.
///
/// AWS SDK method names are snake_case and may collide with Rust keywords
/// (e.g., `type` → `r#type`). This is used when generating method calls
/// on SDK types in provider_generated.rs.
///
/// Note: `crate`, `self`, `Self`, and `super` are excluded because they
/// cannot be used as raw identifiers.
fn escape_rust_keyword(name: &str) -> String {
    // Strict keywords that can be used as raw identifiers (r#keyword).
    // Source: https://doc.rust-lang.org/reference/keywords.html
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "static", "struct", "trait", "type", "unsafe", "use", "where", "while",
    ];
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    validate_examples_dir(args.examples_dir.as_deref())?;

    std::fs::create_dir_all(&args.output_dir)?;

    // Collect all resource definitions
    let mut all_resources = resource_defs::ec2_resources();
    all_resources.extend(resource_defs::s3_resources());
    all_resources.extend(resource_defs::sts_resources());
    all_resources.extend(resource_defs::organizations_resources());
    all_resources.extend(resource_defs::route53_resources());
    all_resources.extend(resource_defs::iam_resources());
    all_resources.extend(resource_defs::logs_resources());
    all_resources.extend(resource_defs::acm_resources());
    all_resources.extend(resource_defs::sqs_resources());

    // Collect all data source definitions
    let mut all_data_sources = resource_defs::sts_data_sources();
    all_data_sources.extend(resource_defs::identitystore_data_sources());
    all_data_sources.extend(resource_defs::s3_data_sources());
    all_data_sources.extend(resource_defs::iam_data_sources());

    // Filter to requested resource if specified
    let resources: Vec<&ResourceDef> = if let Some(ref name) = args.resource {
        all_resources
            .iter()
            .filter(|r| r.name == name.as_str())
            .collect()
    } else {
        all_resources.iter().collect()
    };

    let data_sources: Vec<&resource_defs::DataSourceDef> = if let Some(ref name) = args.resource {
        all_data_sources
            .iter()
            .filter(|d| d.name == name.as_str())
            .collect()
    } else {
        all_data_sources.iter().collect()
    };

    if resources.is_empty() && data_sources.is_empty() {
        if let Some(ref name) = args.resource {
            anyhow::bail!("Unknown resource: {}", name);
        }
        anyhow::bail!("No resource definitions found");
    }

    // Load Smithy models (keyed by service namespace)
    let mut models: HashMap<&str, SmithyModel> = HashMap::new();
    for res in &resources {
        if models.contains_key(res.service_namespace) {
            continue;
        }
        let model = load_model(&args.model_dir, res.service_namespace)?;
        models.insert(res.service_namespace, model);
    }
    for ds in &data_sources {
        if models.contains_key(ds.service_namespace) {
            continue;
        }
        let model = load_model(&args.model_dir, ds.service_namespace)?;
        models.insert(ds.service_namespace, model);
    }

    match args.format.as_str() {
        "rust" => {
            // Generate each resource into service/resource directory structure
            let mut generated_modules: Vec<GeneratedModule> = Vec::new();
            let managed_names: std::collections::HashSet<&str> =
                resources.iter().map(|r| r.name).collect();
            for res in &resources {
                let model = models.get(res.service_namespace).unwrap();
                let code = generate_resource(res, model)?;

                let (service, resource) = split_service_resource(res.name);
                let resource_snake = pascal_to_snake(resource);
                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;

                let output_path = service_dir.join(format!("{}.rs", resource_snake));
                std::fs::write(&output_path, &code)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                eprintln!("Generated: {}", output_path.display());
                generated_modules.push(GeneratedModule {
                    service: service.to_string(),
                    file_stem: resource_snake.clone(),
                    config_fn: format!("{}_config", module_name(res.name)),
                });
            }

            // Generate data source schemas
            for ds in &data_sources {
                let model = models.get(ds.service_namespace).unwrap();
                let dual_registered = managed_names.contains(ds.name);
                let code = generate_data_source(ds, model, dual_registered)?;

                let (service, resource) = split_service_resource(ds.name);
                let resource_snake = pascal_to_snake(resource);
                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;

                let suffix = if dual_registered { "_data_source" } else { "" };
                let file_stem = format!("{}{}", resource_snake, suffix);
                let output_path = service_dir.join(format!("{}.rs", file_stem));
                std::fs::write(&output_path, &code)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                eprintln!("Generated: {}", output_path.display());
                generated_modules.push(GeneratedModule {
                    service: service.to_string(),
                    file_stem,
                    config_fn: format!("{}{}_config", module_name(ds.name), suffix),
                });
            }

            generate_virtual_helper_modules(&args.output_dir, &mut generated_modules)?;

            // Generate per-service mod.rs files
            generate_service_mod_files(&args.output_dir, &generated_modules)?;

            // Generate top-level mod.rs (also picks up orphaned legacy modules)
            let mod_rs = generate_mod_rs(&generated_modules, &args.output_dir);
            let mod_path = args.output_dir.join("mod.rs");
            std::fs::write(&mod_path, &mod_rs)
                .with_context(|| format!("Failed to write {}", mod_path.display()))?;
            eprintln!("Generated: {}", mod_path.display());
        }
        "provider" => {
            let manual_methods = scan_manual_methods(&args.output_dir.join("services"));
            let code =
                generate_provider_code(&all_resources, &all_data_sources, &models, &manual_methods);
            let output_path = args.output_dir.join("provider_generated.rs");
            std::fs::write(&output_path, &code)
                .with_context(|| format!("Failed to write {}", output_path.display()))?;
            eprintln!("Generated: {}", output_path.display());

            let read_models =
                generate_read_models_code(&all_resources, &all_data_sources, &models)?;
            let read_models_path = args.output_dir.join("read_models_generated.rs");
            std::fs::write(&read_models_path, read_models)
                .with_context(|| format!("Failed to write {}", read_models_path.display()))?;
            eprintln!("Generated: {}", read_models_path.display());

            let resend_safety = carina_codegen_aws::resend_safety::generate_provider_module(
                &models,
                carina_codegen_aws::resend_safety::OVERRIDES,
            )?;
            let resend_safety_path = args.output_dir.join("resend_safety_generated.rs");
            std::fs::write(&resend_safety_path, resend_safety)
                .with_context(|| format!("Failed to write {}", resend_safety_path.display()))?;
            eprintln!("Generated: {}", resend_safety_path.display());

            if let Some(clippy_config_path) = &args.clippy_config {
                let clippy_config = carina_codegen_aws::resend_safety::generate_clippy_config(
                    carina_codegen_aws::resend_safety::OVERRIDES,
                );
                std::fs::write(clippy_config_path, clippy_config)
                    .with_context(|| format!("Failed to write {}", clippy_config_path.display()))?;
                eprintln!("Generated: {}", clippy_config_path.display());
            }
        }
        "markdown" | "md" => {
            for res in &resources {
                let model = models.get(res.service_namespace).unwrap();
                let (service, resource) = split_service_resource(res.name);
                let file_name = markdown_file_name(resource, false);
                let example = load_markdown_example(
                    args.examples_dir.as_deref(),
                    service,
                    file_name.trim_end_matches(".md"),
                )?;
                let md = generate_markdown_resource(res, model, example.as_deref())
                    .with_context(|| format!("generating docs for aws.{}", res.name))?;

                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;
                let output_path = service_dir.join(file_name);
                std::fs::write(&output_path, &md)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                eprintln!("Generated: {}", output_path.display());
            }
            for ds in &data_sources {
                let model = models.get(ds.service_namespace).unwrap();
                let (service, resource) = split_service_resource(ds.name);
                let file_name = markdown_file_name(resource, true);
                let example = load_markdown_example(
                    args.examples_dir.as_deref(),
                    service,
                    file_name.trim_end_matches(".md"),
                )?;
                let md = generate_markdown_data_source(ds, model, example.as_deref())
                    .with_context(|| format!("generating docs for aws.{}", ds.name))?;

                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;
                let output_path = service_dir.join(file_name);
                std::fs::write(&output_path, &md)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                eprintln!("Generated: {}", output_path.display());
            }
        }
        other => anyhow::bail!("Unknown format: {}. Use 'rust' or 'markdown'.", other),
    }

    Ok(())
}

/// Load a Smithy model from a JSON file in the model directory.
fn load_model(model_dir: &Path, namespace: &str) -> Result<SmithyModel> {
    // Map namespace to file name: "com.amazonaws.ec2" -> "ec2.Json"
    let service_name = namespace
        .strip_prefix("com.amazonaws.")
        .unwrap_or(namespace);
    let model_path = model_dir.join(format!("{}.json", service_name));

    let json = std::fs::read_to_string(&model_path)
        .with_context(|| format!("Failed to read model: {}", model_path.display()))?;
    let model = carina_smithy::parse(&json)
        .with_context(|| format!("Failed to parse model: {}", model_path.display()))?;

    Ok(model)
}

struct CollectedResourceAttributes {
    attrs: Vec<AttrInfo>,
    all_enums: BTreeMap<String, EnumInfo>,
    description: Option<String>,
}

/// Collect the exact top-level schema attributes before rendering either the
/// schema or its typed read model. Sharing this pass is the guard against the
/// two generated contracts drifting apart.
fn collect_resource_attributes_unvalidated(
    res: &ResourceDef,
    model: &SmithyModel,
) -> Result<CollectedResourceAttributes> {
    let ns = res.service_namespace;
    let namespace = format!("aws.{}", res.name);

    // Build exclude set
    let exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();

    // Build type override map
    let type_overrides: HashMap<&str, &str> = res.type_overrides.iter().copied().collect();

    // Build create-only override set
    let create_only_overrides: HashSet<&str> = res.create_only_overrides.iter().copied().collect();

    // Build required override set
    let required_overrides: HashSet<&str> = res.required_overrides.iter().copied().collect();

    // Build read-only override set
    let read_only_overrides: HashSet<&str> = res.read_only_overrides.iter().copied().collect();

    // Build identity override set
    let identity_overrides: HashSet<&str> = res.identity_overrides.iter().copied().collect();

    // Build deferred-populate override set (carina#3034). Names are
    // PascalCase, matched against `name` (Smithy member name) before
    // it is snake_cased for the schema.
    let deferred_populate_overrides: HashSet<&str> =
        res.deferred_populate_overrides.iter().copied().collect();

    // Build extra read-only set
    let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();

    // Build read-shape override set (carina-provider-aws#296). Names
    // here force the codegen to use the read structure's member_ref
    // for the attribute's type, even if the create input has a
    // narrower shape under the same name. The attribute is also
    // forced read-only — the rich response shape cannot be
    // round-tripped back into the create input.
    let read_shape_overrides: HashSet<&str> = res.read_shape_overrides.iter().copied().collect();

    // Build deferred-populate StructField override set (carina#3034).
    // Each tuple is (attr_pascal_name, member_pascal_name); used
    // when only specific inner fields of a struct/list-of-struct
    // attribute populate asynchronously, not the whole attribute.
    let deferred_populate_struct_fields: HashSet<(&str, &str)> = res
        .deferred_populate_struct_field_overrides
        .iter()
        .copied()
        .collect();

    // Build per-(parent_struct, member) type override map (aws#302).
    // Distinguishes struct fields by parent struct shape so the same
    // member name can resolve to different types in different contexts.
    let struct_field_type_overrides: HashMap<(&str, &str), &str> = res
        .struct_field_type_overrides
        .iter()
        .map(|(p, m, t)| ((*p, *m), *t))
        .collect();

    // Build enum alias map: attr_snake_name -> [(canonical, alias)]
    let mut enum_alias_map: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for (attr, alias, canonical) in &res.enum_aliases {
        enum_alias_map
            .entry(attr)
            .or_default()
            .push((canonical, alias));
    }

    // Build to_dsl override map
    let to_dsl_overrides: HashMap<&str, &str> = res.to_dsl_overrides.iter().copied().collect();

    // Data sources have no create_op — skip create input resolution
    let is_data_source = res.create_op.is_empty();

    // Resolve create input fields (skip for data sources)
    let create_input = if !is_data_source {
        let create_op_id = format!("{}#{}", ns, res.create_op);
        Some(
            model
                .operation_input(&create_op_id)
                .with_context(|| format!("Cannot find create input for {}", create_op_id))?,
        )
    } else {
        None
    };

    // Resolve schema structure (for resources with non-standard create APIs)
    let schema_structure = if let Some(schema_struct_name) = res.schema_structure {
        let schema_structure_id = format!("{}#{}", ns, schema_struct_name);
        Some(
            model
                .get_structure(&schema_structure_id)
                .with_context(|| format!("Cannot find schema structure {}", schema_structure_id))?,
        )
    } else {
        None
    };

    // Resolve read structure fields (if present)
    let read_structure = if let Some(read_struct_name) = res.read_structure {
        let read_structure_id = format!("{}#{}", ns, read_struct_name);
        Some(
            model
                .get_structure(&read_structure_id)
                .with_context(|| format!("Cannot find read structure {}", read_structure_id))?,
        )
    } else {
        None
    };

    // Resolve update input fields and their structures
    let mut updatable_fields: HashSet<String> = HashSet::new();
    let mut update_inputs: Vec<&carina_smithy::StructureShape> = Vec::new();
    for update_op in &res.update_ops {
        for field in update_op.fields.field_names() {
            updatable_fields.insert(field.to_string());
        }
        let update_op_id = format!("{}#{}", ns, update_op.operation);
        if let Some(update_input) = model.operation_input(&update_op_id) {
            update_inputs.push(update_input);
        }
    }

    // Collectors for enums and ranged ints (populated during type resolution)
    let mut all_enums: BTreeMap<String, EnumInfo> = BTreeMap::new();
    let mut all_ranged_ints: BTreeMap<String, IntRange> = BTreeMap::new();

    // Collect writable fields: from schema_structure if set, otherwise from create input
    let mut writable_fields: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    if let Some(schema_struct) = &schema_structure {
        // Use schema_structure members as writable fields.
        // Unlike create input, don't skip the identifier — it's a user-set attribute.
        for (name, member_ref) in &schema_struct.members {
            if exclude.contains(name.as_str()) {
                continue;
            }
            if name.eq_ignore_ascii_case("tags") {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    } else if let Some(create_input) = &create_input {
        for (name, member_ref) in &create_input.members {
            if exclude.contains(name.as_str()) {
                continue;
            }
            if name.eq_ignore_ascii_case("tags") {
                continue; // handled separately
            }
            // Skip names whose schema type should come from the read
            // structure instead — they will be inserted into
            // `read_only_fields` below from the read_structure's
            // member_ref. carina-provider-aws#296.
            if read_shape_overrides.contains(name.as_str()) {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    }

    // For read_ops resources: resolve fields from operation outputs and add them
    // as writable fields (if they match an update op) or read-only.
    let mut read_op_read_only: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    for read_op in &res.read_ops {
        let op_id = format!("{}#{}", ns, read_op.operation);
        let output = model
            .operation_output(&op_id)
            .with_context(|| format!("Cannot find output for {}", op_id))?;
        for (field_name, rename) in &read_op.fields {
            let effective_name = rename.unwrap_or(field_name);
            if let Some(member_ref) = output.members.get(*field_name) {
                if updatable_fields.contains(effective_name)
                    && !writable_fields.contains_key(effective_name)
                {
                    writable_fields.insert(effective_name.to_string(), member_ref);
                } else if !writable_fields.contains_key(effective_name) {
                    read_op_read_only.insert(effective_name.to_string(), member_ref);
                }
            }
        }
    }

    // Add updatable-only fields from read structure and update op inputs
    if let Some(read_struct) = read_structure {
        // (e.g., EnableDnsHostnames for VPC is in ModifyVpcAttributeRequest but not in Vpc struct)
        for (name, member_ref) in &read_struct.members {
            if exclude.contains(name.as_str())
                || name.eq_ignore_ascii_case("tags")
                || name == res.identifier
            {
                continue;
            }
            if writable_fields.contains_key(name) {
                continue;
            }
            if updatable_fields.contains(name.as_str()) {
                writable_fields.insert(name.clone(), member_ref);
            }
        }
    }
    // Also check update operation inputs for fields not found in create input or read structure
    for update_input in &update_inputs {
        for (name, member_ref) in &update_input.members {
            if exclude.contains(name.as_str())
                || name.eq_ignore_ascii_case("tags")
                || name == res.identifier
            {
                continue;
            }
            if writable_fields.contains_key(name) {
                continue;
            }
            if updatable_fields.contains(name.as_str()) {
                writable_fields.insert(name.clone(), member_ref);
            }
        }
    }

    // Add extra_attributes fields sourced from the read structure
    for extra in &res.extra_attributes {
        if writable_fields.contains_key(extra.name) {
            continue;
        }
        if let Some(source_field) = extra.read_source
            && let Some(read_struct) = read_structure
            && let Some(member_ref) = read_struct.members.get(source_field)
        {
            writable_fields.insert(extra.name.to_string(), member_ref);
        }
        // Synthetic fields (read_source = None) are handled after main attribute generation
    }

    // Collect read-only fields from read structure
    let mut read_only_fields: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    if let Some(read_struct) = read_structure {
        for (name, member_ref) in &read_struct.members {
            if exclude.contains(name.as_str()) {
                continue;
            }
            if name.eq_ignore_ascii_case("tags") {
                continue;
            }
            // Skip fields already in writable set
            if writable_fields.contains_key(name) {
                continue;
            }
            // Include the identifier and extra read-only fields
            if name == res.identifier || extra_read_only.contains(name.as_str()) {
                read_only_fields.insert(name.clone(), member_ref);
            }
        }
    }
    // Add read-only fields from read_ops
    for (name, member_ref) in read_op_read_only {
        if !writable_fields.contains_key(&name) && !read_only_fields.contains_key(&name) {
            read_only_fields.insert(name, member_ref);
        }
    }

    // Build extra_attributes description override map
    let extra_attribute_descs: HashMap<&str, Option<&str>> = res
        .extra_attributes
        .iter()
        .map(|e| (e.name, e.description))
        .collect();
    // Build attribute list
    let mut attrs: Vec<AttrInfo> = Vec::new();

    // Process writable fields
    for (name, member_ref) in &writable_fields {
        let snake_name = name.to_snake_case();
        // `extra_attributes` fields with `read_source = None` are synthetic:
        // the codegen has no Smithy member to ground them in. They default
        // to create-only — UNLESS they appear in `update_ops` (typically as
        // `FieldLayout::InsideStruct` members, e.g. PutPublicAccessBlock's
        // `PublicAccessBlockConfiguration` sub-fields), in which case they
        // are updatable just like any other writable field.
        let is_synthetic_extra_attribute = res
            .extra_attributes
            .iter()
            .any(|e| e.name == name.as_str() && e.read_source.is_none());
        // A synthetic declaration is the authoritative schema declaration
        // even when the create request happens to expose a same-named Smithy
        // member. Preserve the synthetic requiredness override instead of
        // emitting the request member and synthetic field twice.
        let is_required = if is_synthetic_extra_attribute {
            required_overrides.contains(name.as_str())
        } else {
            SmithyModel::is_required(member_ref) || required_overrides.contains(name.as_str())
        } && !read_only_overrides.contains(name.as_str());
        let is_read_only = read_only_overrides.contains(name.as_str());
        let is_create_only = if is_read_only {
            false
        } else if is_synthetic_extra_attribute {
            !updatable_fields.contains(name.as_str())
                || create_only_overrides.contains(name.as_str())
        } else if schema_structure.is_some() {
            // For schema_structure resources, only explicit overrides are create-only.
            // The default is writable since the update operation is hand-coded.
            create_only_overrides.contains(name.as_str())
        } else {
            create_only_overrides.contains(name.as_str())
                || !updatable_fields.contains(name.as_str())
        };
        // Use ExtraField description override if available, otherwise Smithy docs
        let description = if let Some(Some(desc)) = extra_attribute_descs.get(name.as_str()) {
            Some(desc.to_string())
        } else {
            SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string())
        };

        let (type_code, enum_info) = resolve_type(
            &mut TypeResolutionContext {
                model,
                namespace: &namespace,
                struct_path: Vec::new(),
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
                current_root_attr: Some(name.as_str()),
                deferred_populate_struct_fields: &deferred_populate_struct_fields,
                struct_field_type_overrides: &struct_field_type_overrides,
            },
            &member_ref.target,
            name,
        );

        let is_plain_string = is_plain_smithy_string(model, &member_ref.target);
        let is_identity = identity_overrides.contains(name.as_str());
        let is_deferred_populate = deferred_populate_overrides.contains(name.as_str());

        attrs.push(AttrInfo {
            snake_name,
            provider_name: name.clone(),
            type_code,
            is_required,
            is_create_only,
            is_read_only,
            is_write_only: false,
            read_presence: if is_required {
                resource_defs::ReadPresence::Required
            } else {
                resource_defs::ReadPresence::Optional
            },
            read_reason: None,
            is_plain_string,
            is_identity,
            is_deferred_populate,
            description,
            enum_info,
        });
    }

    // Process synthetic extra_attributes fields (no read_source).
    // A synthetic field defaults to create-only, but is treated as updatable
    // when it appears in `update_ops` (typically as a `FieldLayout::InsideStruct`
    // member of a wrapper struct in the API request shape). When the field is
    // listed in `read_only_overrides`, it is emitted as read-only instead —
    // mirrors the behaviour for Smithy-grounded fields.
    for extra in &res.extra_attributes {
        if extra.read_source.is_some() {
            continue; // Already handled via writable_fields
        }
        if attrs.iter().any(|attr| attr.provider_name == extra.name) {
            continue; // Same-named Smithy member was emitted with synthetic semantics above.
        }
        let snake_name = extra.name.to_snake_case();
        let type_code = if let Some(&override_type) = type_overrides.get(extra.name) {
            override_type.to_string()
        } else if let Some(inferred) = infer_string_type(extra.name) {
            inferred
        } else {
            PLAIN_STRING_TYPE_CODE.to_string()
        };
        let is_read_only = read_only_overrides.contains(extra.name);
        let is_create_only = !is_read_only
            && (!updatable_fields.contains(extra.name)
                || create_only_overrides.contains(extra.name));
        let is_required = required_overrides.contains(extra.name);
        attrs.push(AttrInfo {
            snake_name,
            provider_name: extra.name.to_string(),
            type_code,
            is_required,
            is_create_only,
            is_read_only,
            is_write_only: false,
            read_presence: if is_required {
                resource_defs::ReadPresence::Required
            } else {
                resource_defs::ReadPresence::Optional
            },
            read_reason: None,
            is_plain_string: false,
            is_identity: identity_overrides.contains(extra.name),
            is_deferred_populate: deferred_populate_overrides.contains(extra.name),
            description: extra.description.map(|s| s.to_string()),
            enum_info: None,
        });
    }

    // Process read-only fields
    for (name, member_ref) in &read_only_fields {
        let snake_name = name.to_snake_case();
        let description = SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string());

        let (type_code, enum_info) = resolve_type(
            &mut TypeResolutionContext {
                model,
                namespace: &namespace,
                struct_path: Vec::new(),
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
                current_root_attr: Some(name.as_str()),
                deferred_populate_struct_fields: &deferred_populate_struct_fields,
                struct_field_type_overrides: &struct_field_type_overrides,
            },
            &member_ref.target,
            name,
        );

        let is_plain_string = is_plain_smithy_string(model, &member_ref.target);
        attrs.push(AttrInfo {
            snake_name,
            provider_name: name.clone(),
            type_code,
            is_required: false,
            is_create_only: false,
            is_read_only: true,
            is_write_only: false,
            read_presence: resource_defs::ReadPresence::Optional,
            read_reason: None,
            is_plain_string,
            is_identity: false,
            is_deferred_populate: deferred_populate_overrides.contains(name.as_str()),
            description,
            enum_info,
        });
    }

    // Also register top-level attribute enums (enum_info is set but may not have
    // been registered if the attribute was detected via known_enum_overrides in
    // resolve_type before the collector existed)
    for attr in &attrs {
        if let Some(ref ei) = attr.enum_info {
            all_enums
                .entry(attr.provider_name.clone())
                .or_insert_with(|| ei.clone());
        }
    }

    let description = if let Some(read_struct) = read_structure {
        SmithyModel::documentation(&read_struct.traits).map(str::to_string)
    } else {
        create_input
            .as_ref()
            .and_then(|input| SmithyModel::documentation(&input.traits))
            .map(str::to_string)
    };

    Ok(CollectedResourceAttributes {
        attrs,
        all_enums,
        description,
    })
}

fn collect_resource_attributes(
    res: &ResourceDef,
    model: &SmithyModel,
) -> Result<CollectedResourceAttributes> {
    let mut collected = collect_resource_attributes_unvalidated(res, model)?;
    let dispositions = res.read_dispositions();
    apply_read_contract(res, model, &mut collected.attrs, &dispositions)?;
    collected.all_enums.retain(|provider_name, _| {
        collected
            .attrs
            .iter()
            .any(|attr| attr.provider_name == *provider_name)
    });
    Ok(collected)
}

#[cfg(test)]
fn validate_read_contract(
    res: &ResourceDef,
    model: &SmithyModel,
    dispositions: &[resource_defs::ReadDisposition],
) -> Result<()> {
    let mut collected = collect_resource_attributes_unvalidated(res, model)?;
    apply_read_contract(res, model, &mut collected.attrs, dispositions)
}

fn has_inferred_read_source(res: &ResourceDef, model: &SmithyModel, attr: &AttrInfo) -> bool {
    let attr_snake = attr.provider_name.to_snake_case();
    if res
        .derived_attributes
        .iter()
        .any(|derived| derived.attr.to_snake_case() == attr_snake)
    {
        return true;
    }

    if res.read_ops.iter().any(|operation| {
        operation
            .fields
            .iter()
            .any(|(field, rename)| rename.unwrap_or(field).to_snake_case() == attr_snake)
    }) {
        return true;
    }

    let read_member = res
        .extra_attributes
        .iter()
        .find(|extra| extra.name.to_snake_case() == attr_snake)
        .and_then(|extra| extra.read_source)
        .unwrap_or(attr.provider_name.as_str());
    let Some(read_structure) = res.read_structure else {
        return false;
    };
    let read_structure_id = format!("{}#{read_structure}", res.service_namespace);
    model
        .get_structure(&read_structure_id)
        .is_some_and(|structure| {
            structure
                .members
                .keys()
                .any(|name| name.to_snake_case() == read_member.to_snake_case())
        })
}

fn apply_read_contract(
    res: &ResourceDef,
    model: &SmithyModel,
    attrs: &mut Vec<AttrInfo>,
    dispositions: &[resource_defs::ReadDisposition],
) -> Result<()> {
    use resource_defs::ReadDisposition;

    let mut seen = HashSet::new();
    for disposition in dispositions {
        let field = disposition.field();
        let normalized = field.to_snake_case();
        if !seen.insert(normalized.clone()) {
            anyhow::bail!(
                "{} has duplicate/conflicting read dispositions for {}",
                res.name,
                field
            );
        }

        let Some(attr) = attrs
            .iter_mut()
            .find(|attr| attr.provider_name.to_snake_case() == normalized)
        else {
            anyhow::bail!(
                "{} read disposition names unknown schema attribute {}",
                res.name,
                field
            );
        };

        match disposition {
            ReadDisposition::Read { presence, .. } => {
                attr.read_presence = *presence;
            }
            ReadDisposition::WriteOnly { reason, .. } => {
                if reason.trim().is_empty() {
                    anyhow::bail!(
                        "{} write-only attribute {} requires a non-empty reason",
                        res.name,
                        field
                    );
                }
                if attr.is_read_only || !attr.is_create_only {
                    anyhow::bail!(
                        "{} write-only attribute {} must be writable and create-only",
                        res.name,
                        field
                    );
                }
                attr.is_write_only = true;
                attr.read_reason = Some((*reason).to_string());
            }
            ReadDisposition::OmitSchema { reason, .. } => {
                if reason.trim().is_empty() {
                    anyhow::bail!(
                        "{} schema omission for {} requires a non-empty reason",
                        res.name,
                        field
                    );
                }
                attr.read_reason = Some((*reason).to_string());
            }
        }
    }

    let omitted: HashSet<String> = dispositions
        .iter()
        .filter_map(|disposition| match disposition {
            ReadDisposition::OmitSchema { field, .. } => Some(field.to_snake_case()),
            _ => None,
        })
        .collect();
    attrs.retain(|attr| !omitted.contains(&attr.provider_name.to_snake_case()));

    let mut unclassified = Vec::new();
    for attr in attrs.iter() {
        let explicitly_readable = dispositions.iter().any(|disposition| {
            matches!(disposition, ReadDisposition::Read { .. })
                && disposition.field().to_snake_case() == attr.provider_name.to_snake_case()
        });
        if !attr.is_write_only
            && !explicitly_readable
            && !has_inferred_read_source(res, model, attr)
        {
            unclassified.push(attr.provider_name.as_str());
        }
    }

    if !unclassified.is_empty() {
        anyhow::bail!(
            "{} schema attributes are unclassified: {}; add read dispositions or classify them as write-only/omitted with reasons",
            res.name,
            unclassified.join(", ")
        );
    }

    Ok(())
}

/// Generate Rust schema code for a single resource.
fn generate_resource(res: &ResourceDef, model: &SmithyModel) -> Result<String> {
    let CollectedResourceAttributes {
        attrs,
        all_enums,
        description,
    } = collect_resource_attributes(res, model)?;
    let ns = res.service_namespace;
    let is_data_source = res.create_op.is_empty();
    let namespace = format!("aws.{}", res.name);
    let mut enum_alias_map: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for (attr, alias, canonical) in &res.enum_aliases {
        enum_alias_map
            .entry(attr)
            .or_default()
            .push((canonical, alias));
    }
    let to_dsl_overrides: HashMap<&str, &str> = res.to_dsl_overrides.iter().copied().collect();

    // Determine needed imports
    let code_str = attrs
        .iter()
        .map(|a| a.type_code.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let needs_types = code_str.contains("types::");
    let needs_tags_type = res.has_tags;
    let needs_struct_field = code_str.contains("StructField::");

    // Build code
    let mut code = String::new();
    let mod_name = module_name(res.name);

    // Header
    let resource_short = res
        .name
        .strip_prefix("ec2.")
        .or_else(|| res.name.strip_prefix("s3."))
        .or_else(|| res.name.strip_prefix("sts."))
        .unwrap_or(res.name);
    let mut schema_imports = vec!["AttributeSchema", "ResourceSchema"];
    let needs_attribute_type = attrs
        .iter()
        .any(|a| a.type_code.contains("AttributeType::"));
    if needs_attribute_type {
        schema_imports.insert(1, "AttributeType");
    }
    if needs_struct_field {
        schema_imports.push("StructField");
    }
    if needs_types {
        schema_imports.push("types");
    }
    let arn_identity = resource_identity_parts(res.name)
        .filter(|_| attrs.iter().any(|a| a.snake_name == "arn"))
        .map(|(service, resource)| (service, resource, arn_emit_choice(service, resource)));
    let arn_helper =
        arn_identity.map(|(service, resource, choice)| emit_arn_helper(service, resource, choice));
    let has_arn_helper = arn_helper.is_some();
    if has_arn_helper && !schema_imports.contains(&"AttributeType") {
        schema_imports.insert(1, "AttributeType");
    }
    let schema_imports_str = schema_imports.join(", ");
    code.push_str(&format!(
        "//! {} schema definition for AWS Cloud Control\n\
         //!\n\
         //! Auto-generated from Smithy model: {}\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with smithy-codegen\n\n\
         use super::AwsSchemaConfig;\n",
        resource_short, ns
    ));

    if needs_tags_type {
        code.push_str("use super::tags_type;\n");
        code.push_str("use carina_provider_protocol::types::ValidatorType;\n");
    }
    code.push_str(&format!(
        "use carina_core::schema::{{{}}};\n\n",
        schema_imports_str
    ));

    // Generate enum constants.
    //
    // The constant lists only canonical AWS wire values. DSL spellings live
    // in schema `dsl_aliases`; mixing aliases into this list makes the host
    // treat them as already API-canonical.
    for (prop_name, enum_info) in &all_enums {
        let const_name = format!("VALID_{}", prop_name.to_snake_case().to_uppercase());
        let values_str = enum_info
            .values
            .iter()
            .map(|v| format!("\"{}\"", v))
            .collect::<Vec<_>>()
            .join(", ");
        code.push_str(&format!(
            "const {}: &[&str] = &[{}];\n\n",
            const_name, values_str
        ));
    }

    if let Some(helper) = &arn_helper {
        code.push_str(helper);
    }

    // Generate config function
    code.push_str(&format!(
        "/// Returns the schema config for {} (Smithy: {})\n\
         pub fn {}_config() -> AwsSchemaConfig {{\n\
         \x20   AwsSchemaConfig {{\n\
         \x20       aws_type_name: \"{}\",\n\
         \x20       resource_type_name: \"{}\",\n\
         \x20       has_tags: {},\n\
         \x20       schema: ResourceSchema::new(\"{}\")\n",
        res.name,
        ns,
        mod_name,
        cf_type_name(res.name),
        res.name,
        res.has_tags,
        res.name,
    ));

    if let Some(description) = description {
        let escaped = escape_description(&description);
        let truncated = truncate_str(&escaped, 200);
        code.push_str(&format!(
            "\x20       .with_description(\"{}\")\n",
            truncated
        ));
    }

    // Mark data sources
    if is_data_source {
        code.push_str("\x20       .as_data_source()\n");
    }

    emit_replacement_builder(&mut code, res.name, res.replacement, &attrs)?;

    // Generate attributes
    for attr in &attrs {
        if let Some(reason) = &attr.read_reason {
            code.push_str(&format!(
                "\x20       // Read contract: {}\n",
                reason.replace('\n', " ")
            ));
        }
        let type_code = if let Some(ref ei) = attr.enum_info {
            // Use shared schema enum type for constrained strings.
            //
            // Per naming-conventions design D7, every enum value gets a
            // snake_case DSL alias. `build_dsl_aliases_code` derives the
            // per-value mapping from `dsl_enum_value()` and layers in
            // explicit aliases from `ResourceDef::enum_aliases`. Explicit
            // `to_dsl_overrides` wins over both.
            let dsl_aliases_code =
                if let Some(override_code) = to_dsl_overrides.get(attr.snake_name.as_str()) {
                    override_code.to_string()
                } else {
                    let snake = attr.provider_name.to_snake_case();
                    let explicit = enum_alias_map.get(snake.as_str());
                    build_dsl_aliases_code(&ei.values, explicit)
                };
            let values_str = ei
                .values
                .iter()
                .map(|v| format!("\"{}\".to_string()", v))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "AttributeType::enum_(\n\
                 \x20   carina_core::schema::enum_identity(\"{}\", Some(\"{}\")),\n\
                 \x20   Some(vec![{}]),\n\
                 \x20   {},\n\
                 \x20   None,\n\
                 \x20   None,\n\
                 )",
                ei.type_name, namespace, values_str, dsl_aliases_code
            )
        } else {
            attr.type_code.clone()
        };

        let type_code = if has_arn_helper && attr.snake_name == "arn" {
            "self::arn()".to_string()
        } else {
            type_code
        };

        let constructor = mode_constructor(attr.is_required, attr.is_read_only);
        let mut attr_code = format!(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::{}(\"{}\", {})",
            constructor, attr.snake_name, type_code
        );

        if attr.is_create_only {
            attr_code.push_str("\n\x20               .create_only()");
        }
        if attr.is_write_only {
            attr_code.push_str("\n\x20               .write_only()");
        }
        if attr.is_identity {
            attr_code.push_str("\n\x20               .identity()");
        }
        if attr.is_deferred_populate {
            attr_code.push_str("\n\x20               .deferred_populate()");
        }

        if let Some(ref desc) = attr.description {
            let escaped = escape_description(desc);
            let truncated = truncate_str(&escaped, 150);
            let suffix = if attr.is_read_only {
                " (read-only)"
            } else {
                ""
            };
            attr_code.push_str(&format!(
                "\n\x20               .with_description(\"{}{}\")",
                truncated, suffix
            ));
        } else if attr.is_read_only {
            attr_code.push_str("\n\x20               .with_description(\" (read-only)\")");
        }

        attr_code.push_str(&format!(
            "\n\x20               .with_provider_name(\"{}\")",
            attr.provider_name
        ));
        attr_code.push_str("\n\x20               .build()");

        attr_code.push_str(",\n\x20       )\n");
        code.push_str(&attr_code);
    }

    // Tags attribute
    if res.has_tags {
        code.push_str(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::optional(\"tags\", tags_type())\n\
             \x20               .with_description(\"The tags for the resource.\")\n\
             \x20               .with_provider_name(\"Tags\")\n\
             \x20               .build(),\n\
             \x20       )\n",
        );
    }

    // Tags validator
    if res.has_tags {
        code.push_str(
            "\x20       .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])\n",
        );
    }

    // Close schema and config
    code.push_str("\x20   }\n}\n");

    // Generate enum_valid_values()
    code.push_str(
        "\n/// Returns the resource type name and all enum valid values for this module\n\
         pub fn enum_valid_values() -> (&'static str, &'static [(&'static str, &'static [&'static str])]) {\n"
    );
    if all_enums.is_empty() {
        code.push_str(&format!("    (\"{}\", &[])\n", res.name));
    } else {
        let entries: Vec<String> = all_enums
            .keys()
            .map(|prop_name| {
                let attr_name = prop_name.to_snake_case();
                let const_name = format!("VALID_{}", attr_name.to_uppercase());
                format!("        (\"{}\", {}),", attr_name, const_name)
            })
            .collect();
        code.push_str(&format!(
            "    (\"{}\", &[\n{}\n    ])\n",
            res.name,
            entries.join("\n")
        ));
    }
    code.push_str("}\n");

    Ok(code)
}

/// Shared context for Smithy-to-Carina type resolution.
///
/// Groups the Smithy model, configuration overrides, and mutable collectors
/// that are passed to both `resolve_type` and `generate_struct_type`.
struct TypeResolutionContext<'a> {
    model: &'a SmithyModel,
    /// Base resource namespace, e.g. `aws.acm.Certificate`.
    namespace: &'a str,
    /// Containing Smithy struct type names between the resource and the
    /// field currently being resolved.
    struct_path: Vec<String>,
    type_overrides: &'a HashMap<&'a str, &'a str>,
    enum_alias_map: &'a HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    to_dsl_overrides: &'a HashMap<&'a str, &'a str>,
    all_enums: &'a mut BTreeMap<String, EnumInfo>,
    all_ranged_ints: &'a mut BTreeMap<String, IntRange>,
    /// Top-level attribute being resolved — set by callers in the
    /// writable / read-only loops, used by `generate_struct_type` to
    /// decide whether a nested StructField needs `.deferred_populate()`.
    /// `None` for callers (e.g. inside enum collection passes) that
    /// don't have an attribute context.
    current_root_attr: Option<&'a str>,
    /// `(attr_pascal_name, member_pascal_name)` pairs whose nested
    /// StructField should be marked `.deferred_populate()`. Carries
    /// `ResourceDef.deferred_populate_struct_field_overrides` —
    /// `generate_struct_type` checks `(current_root_attr, member)`
    /// against this set when emitting each field. carina#3034.
    deferred_populate_struct_fields: &'a HashSet<(&'a str, &'a str)>,
    /// `(parent_struct, member) -> type_code` overrides for struct
    /// fields, keyed by the immediate parent struct shape name (not the
    /// top-level attribute). Lets a member like `HostedZoneId` resolve
    /// to one type inside `AliasTarget` and another at the top level.
    /// Carries `ResourceDef.struct_field_type_overrides` (aws#302).
    struct_field_type_overrides: &'a HashMap<(&'a str, &'a str), &'a str>,
}

fn enum_namespace(ctx: &TypeResolutionContext<'_>) -> String {
    if ctx.struct_path.is_empty() {
        ctx.namespace.to_string()
    } else {
        format!("{}.{}", ctx.namespace, ctx.struct_path.join("."))
    }
}

/// Resolve a Smithy type to a Carina type code string.
/// Returns (type_code, Option<EnumInfo>).
/// Also populates collectors for enums and ranged ints discovered during resolution.
fn resolve_type(
    ctx: &mut TypeResolutionContext<'_>,
    target: &str,
    field_name: &str,
) -> (String, Option<EnumInfo>) {
    // Check type overrides first
    if let Some(&override_type) = ctx.type_overrides.get(field_name) {
        return (override_type.to_string(), None);
    }

    // Check known enum overrides
    if let Some(values) = known_enum_overrides().get(field_name) {
        let type_name = field_name.to_string();
        let enum_info = EnumInfo {
            type_name,
            values: values.iter().map(|s| s.to_string()).collect(),
        };
        ctx.all_enums
            .entry(field_name.to_string())
            .or_insert_with(|| enum_info.clone());
        return ("/* enum */".to_string(), Some(enum_info));
    }

    let kind = ctx.model.shape_kind(target);

    match kind {
        Some(ShapeKind::String) => {
            // Check name-based type inference (handles CIDR, IP, AZ, ARN, resource IDs, etc.)
            if let Some(inferred) = infer_string_type(field_name) {
                return (inferred, None);
            }

            (PLAIN_STRING_TYPE_CODE.to_string(), None)
        }
        Some(ShapeKind::Boolean) => ("AttributeType::bool()".to_string(), None),
        Some(ShapeKind::Integer) | Some(ShapeKind::Long) => {
            // Check for range traits on the target shape
            let range = get_int_range(ctx.model, target, field_name);
            if let Some(r) = range {
                ctx.all_ranged_ints
                    .entry(field_name.to_string())
                    .or_insert(r);
                let range_expr = match (r.min, r.max) {
                    (Some(min), Some(max)) => {
                        format!("Some((Some({}), Some({})))", min, max)
                    }
                    (Some(min), None) => format!("Some((Some({}), None))", min),
                    (None, Some(max)) => format!("Some((None, Some({})))", max),
                    (None, None) => "None".to_string(),
                };
                (
                    format!(
                        "AttributeType::refined_int(\n\
                         \x20               None,\n\
                         \x20               {},\n\
                         \x20           )",
                        range_expr
                    ),
                    None,
                )
            } else {
                ("AttributeType::int()".to_string(), None)
            }
        }
        Some(ShapeKind::Float) | Some(ShapeKind::Double) => {
            ("AttributeType::float()".to_string(), None)
        }
        Some(ShapeKind::Enum) => {
            // Get enum values from Smithy model
            if let Some(values) = ctx.model.enum_values(target) {
                // Prefer the Smithy shape name (PascalCase, e.g. "LogGroupClass")
                // over the field name (which can be camelCase, e.g. "logGroupClass")
                // so the generated DSL identifier reads naturally.
                let type_name =
                    pascalize_enum_type_name(SmithyModel::shape_name(target), field_name);
                let string_values: Vec<String> = values.into_iter().map(|(_, v)| v).collect();
                let enum_info = EnumInfo {
                    type_name,
                    values: string_values,
                };
                ctx.all_enums
                    .entry(field_name.to_string())
                    .or_insert_with(|| enum_info.clone());
                return ("/* enum */".to_string(), Some(enum_info));
            }
            (PLAIN_STRING_TYPE_CODE.to_string(), None)
        }
        Some(ShapeKind::IntEnum) => ("AttributeType::int()".to_string(), None),
        Some(ShapeKind::List) => {
            // Get list member type
            if let Some(carina_smithy::Shape::List(list_shape)) = ctx.model.get_shape(target) {
                let (item_type, _) = resolve_type(ctx, &list_shape.member.target, field_name);
                (format!("AttributeType::list({})", item_type), None)
            } else {
                (
                    format!("AttributeType::list({PLAIN_STRING_TYPE_CODE})"),
                    None,
                )
            }
        }
        Some(ShapeKind::Map) => (
            format!("AttributeType::map({PLAIN_STRING_TYPE_CODE})"),
            None,
        ),
        Some(ShapeKind::Structure) => {
            // Check if it's a TagList-like structure
            let shape_name = SmithyModel::shape_name(target);
            if shape_name == "TagList" || shape_name == "Tag" {
                return ("tags_type()".to_string(), None);
            }

            // Unwrap EC2 AttributeBooleanValue wrapper → plain Bool
            if shape_name == "AttributeBooleanValue" {
                return ("AttributeType::bool()".to_string(), None);
            }

            // Generate struct type for nested structures
            if let Some(structure) = ctx.model.get_structure(target) {
                let struct_code = generate_struct_type(ctx, shape_name, structure);
                return (struct_code, None);
            }
            (PLAIN_STRING_TYPE_CODE.to_string(), None)
        }
        _ => {
            // Fallback: try name-based heuristics
            if let Some(inferred) = infer_string_type(field_name) {
                (inferred, None)
            } else {
                (PLAIN_STRING_TYPE_CODE.to_string(), None)
            }
        }
    }
}

/// Generate Rust code for an AttributeType::Struct.
fn generate_struct_type(
    ctx: &mut TypeResolutionContext<'_>,
    struct_name: &str,
    structure: &carina_smithy::StructureShape,
) -> String {
    ctx.struct_path.push(struct_name.to_string());
    let mut fields: Vec<String> = Vec::new();
    for (field_name, member_ref) in &structure.members {
        let snake_name = field_name.to_snake_case();
        let is_required = SmithyModel::is_required(member_ref);

        // Check `(parent_struct, member)` type override first — distinguishes
        // a member name shared across structs (e.g. `HostedZoneId` in
        // `AliasTarget` vs `RecordSet`). Falls through to the default
        // resolve_type path when no per-parent override is registered.
        let override_type = ctx
            .struct_field_type_overrides
            .get(&(struct_name, field_name.as_str()))
            .copied();

        let (field_type, enum_info) = if let Some(t) = override_type {
            (t.to_string(), None)
        } else {
            resolve_type(ctx, &member_ref.target, field_name)
        };

        // If enum detected, use shared schema enum type.
        let field_type = if let Some(ei) = enum_info {
            let namespace = enum_namespace(ctx);
            // Same D7 rule as the top-level enum attribute path: derive a
            // snake_case DSL alias for every API value, layer in explicit
            // aliases, and let `to_dsl_overrides` take precedence over both.
            let dsl_aliases_code =
                if let Some(override_code) = ctx.to_dsl_overrides.get(snake_name.as_str()) {
                    override_code.to_string()
                } else {
                    let explicit = ctx.enum_alias_map.get(snake_name.as_str());
                    build_dsl_aliases_code(&ei.values, explicit)
                };
            let values_str = ei
                .values
                .iter()
                .map(|v| format!("\"{}\".to_string()", v))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "AttributeType::enum_(\n\
                 \x20   carina_core::schema::enum_identity(\"{}\", Some(\"{}\")),\n\
                 \x20   Some(vec![{}]),\n\
                 \x20   {},\n\
                 \x20   None,\n\
                 \x20   None,\n\
                 )",
                ei.type_name, namespace, values_str, dsl_aliases_code
            )
        } else {
            field_type
        };

        let constructor = mode_constructor(is_required, false);
        let mut field_code = format!(
            "StructField::{}(\"{}\", {})",
            constructor, snake_name, field_type
        );
        if let Some(root_attr) = ctx.current_root_attr
            && ctx
                .deferred_populate_struct_fields
                .contains(&(root_attr, field_name.as_str()))
        {
            field_code.push_str(".deferred_populate()");
        }
        if let Some(desc) = SmithyModel::documentation(&member_ref.traits) {
            let escaped = escape_description(desc);
            let truncated = truncate_str(&escaped, 150);
            field_code.push_str(&format!(".with_description(\"{}\")", truncated));
        }
        field_code.push_str(&format!(".with_provider_name(\"{}\")", field_name));
        field_code.push_str(".build()");
        fields.push(field_code);
    }

    let fields_str = fields.join(",\n                    ");
    ctx.struct_path.pop();
    format!(
        "AttributeType::struct_(\n\
         \x20                   \"{}\".to_string(),\n\
         \x20                   vec![\n\
         \x20                   {}\n\
         \x20                   ],\n\
         \x20               )",
        struct_name, fields_str
    )
}

/// Get integer range for a field from Smithy traits or known overrides.
fn get_int_range(model: &SmithyModel, target: &str, field_name: &str) -> Option<IntRange> {
    // Check Smithy range trait on the target shape
    if let Some(shape) = model.get_shape(target) {
        let traits = match shape {
            carina_smithy::Shape::Integer(t) => &t.traits,
            carina_smithy::Shape::Long(t) => &t.traits,
            _ => {
                // Check known overrides for the field name
                return known_int_range_overrides()
                    .get(field_name)
                    .map(|&(min, max)| IntRange {
                        min: Some(min),
                        max: Some(max),
                    });
            }
        };
        if let Some(range_val) = traits.get("smithy.api#range") {
            let min = range_val.get("min").and_then(|v| v.as_i64());
            let max = range_val.get("max").and_then(|v| v.as_i64());
            if min.is_some() || max.is_some() {
                return Some(IntRange { min, max });
            }
        }
    }

    // Check known overrides
    known_int_range_overrides()
        .get(field_name)
        .map(|&(min, max)| IntRange {
            min: Some(min),
            max: Some(max),
        })
}

/// Generate per-service mod.rs files that declare resource modules.
/// Tracks one generated schema module (Managed resource or DataSource).
#[derive(Debug, Clone)]
struct GeneratedModule {
    /// Service segment (e.g. "s3").
    service: String,
    /// File stem inside the service directory: "bucket" for the Managed
    /// resource, "bucket_data_source" for the DataSource sibling when both
    /// register under the same DSL name.
    file_stem: String,
    /// `<file_stem>::<config_fn>()` is the entry point for `configs()`.
    config_fn: String,
}

fn generate_virtual_helper_modules(
    output_dir: &std::path::Path,
    modules: &mut Vec<GeneratedModule>,
) -> Result<()> {
    let iam_dir = output_dir.join("iam");
    std::fs::create_dir_all(&iam_dir)
        .with_context(|| format!("Failed to create {}", iam_dir.display()))?;
    let iam_policy = format!(
        "//! Auto-generated helper schema module for AWS IAM Policy identifiers\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh\n\n\
         use carina_core::schema::AttributeType;\n\n{}",
        emit_arn_helper("iam", "Policy", arn_emit_choice("iam", "Policy"))
    );
    let iam_policy_path = iam_dir.join("policy.rs");
    std::fs::write(&iam_policy_path, iam_policy)
        .with_context(|| format!("Failed to write {}", iam_policy_path.display()))?;

    let kms_dir = output_dir.join("kms");
    std::fs::create_dir_all(&kms_dir)
        .with_context(|| format!("Failed to create {}", kms_dir.display()))?;
    let kms_key = format!(
        "//! Auto-generated helper schema module for AWS KMS Key identifiers\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh\n\n\
         use carina_core::schema::AttributeType;\n\n{}\
         pub fn id() -> AttributeType {{\n\
         \x20   super::kms_key_id()\n\
         }}\n",
        emit_arn_helper("kms", "Key", arn_emit_choice("kms", "Key"))
    );
    let kms_key_path = kms_dir.join("key.rs");
    std::fs::write(&kms_key_path, kms_key)
        .with_context(|| format!("Failed to write {}", kms_key_path.display()))?;

    modules.push(GeneratedModule {
        service: "iam".to_string(),
        file_stem: "policy".to_string(),
        config_fn: String::new(),
    });
    modules.push(GeneratedModule {
        service: "kms".to_string(),
        file_stem: "key".to_string(),
        config_fn: String::new(),
    });

    Ok(())
}

fn generate_service_mod_files(
    output_dir: &std::path::Path,
    modules: &[GeneratedModule],
) -> Result<()> {
    // Group modules by service. File stems are already snake-cased.
    let mut services: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for m in modules {
        services
            .entry(m.service.as_str())
            .or_default()
            .push(m.file_stem.as_str());
    }

    for (service, file_stems) in &services {
        let mut code = String::new();
        code.push_str(
            "//! Auto-generated — DO NOT EDIT MANUALLY\n\
             //!\n\
             //! Regenerate with:\n\
             //!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh\n\n\
             // Re-export parent types so resource modules can use `super::` to access them.\n\
             pub use super::*;\n\n",
        );

        let mut sorted: Vec<&&str> = file_stems.iter().collect();
        sorted.sort();
        sorted.dedup();
        for stem in sorted {
            code.push_str(&format!("pub mod {};\n", stem));
        }

        let mod_path = output_dir.join(service).join("mod.rs");
        std::fs::write(&mod_path, &code)
            .with_context(|| format!("Failed to write {}", mod_path.display()))?;
        eprintln!("Generated: {}", mod_path.display());
    }

    Ok(())
}

/// Scan `output_dir` for orphaned service/resource modules — files that exist on disk
/// but are not registered in `resource_defs.rs`. These are typically legacy schemas
/// generated by an earlier codegen pipeline. Returns DSL names using the
/// naming-conventions spelling (PascalCase final segment, e.g. `"iam.Role"`).
fn scan_orphaned_modules(
    output_dir: &std::path::Path,
    known_modules: &[GeneratedModule],
) -> Vec<String> {
    let mut orphaned = Vec::new();
    // The on-disk filename is snake_case (e.g. `role.rs`); compare against
    // the file stems we just generated.
    let known_snake: std::collections::HashSet<String> = known_modules
        .iter()
        .map(|m| format!("{}.{}", m.service, m.file_stem))
        .collect();

    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return orphaned;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(service) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Scan resources (.rs files except mod.rs) in this service directory
        let Ok(service_entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for service_entry in service_entries.flatten() {
            let rpath = service_entry.path();
            if rpath.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Some(file_stem) = rpath.file_stem().and_then(|n| n.to_str()) else {
                continue;
            };
            if file_stem == "mod" {
                continue;
            }
            let snake_name = format!("{}.{}", service, file_stem);
            if !known_snake.contains(&snake_name) {
                // Emit in new-spelling DSL form so callers can compose output
                // paths with `split_service_resource`.
                let resource_pascal: String = {
                    let mut out = String::with_capacity(file_stem.len());
                    let mut upper = true;
                    for c in file_stem.chars() {
                        if c == '_' {
                            upper = true;
                        } else if upper {
                            out.push(c.to_ascii_uppercase());
                            upper = false;
                        } else {
                            out.push(c);
                        }
                    }
                    out
                };
                orphaned.push(format!("{}.{}", service, resource_pascal));
            }
        }
    }
    orphaned.sort();
    orphaned
}

/// Generate mod.rs that includes all generated modules.
fn generate_mod_rs(modules: &[GeneratedModule], output_dir: &std::path::Path) -> String {
    let mut code = String::new();

    code.push_str(
        "//! Auto-generated AWS provider resource schemas\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh\n\n\
         // Re-export all types and validators from config so that\n\
         // generated schema files can use `super::` to access them.\n\
         pub use super::config::*;\n\n",
    );

    // Scan for orphaned modules (files on disk not in resource_defs.rs) so we
    // preserve legacy schemas across codegen runs.
    let orphaned = scan_orphaned_modules(output_dir, modules);

    // Build a unified entry list. Sort key is (service, file_stem) so the
    // emitted text is stable across runs.
    #[derive(Debug, Clone)]
    struct Entry {
        service: String,
        file_stem: String,
        config_fn: String,
    }
    let mut entries: Vec<Entry> = modules
        .iter()
        .map(|m| Entry {
            service: m.service.clone(),
            file_stem: m.file_stem.clone(),
            config_fn: m.config_fn.clone(),
        })
        .collect();
    for orphan in orphaned {
        // Orphan format is `"<service>.<PascalResource>"` (legacy spelling).
        let (service, resource) = split_service_resource(&orphan);
        let resource_snake = pascal_to_snake(resource);
        entries.push(Entry {
            service: service.to_string(),
            file_stem: resource_snake.clone(),
            config_fn: format!("{}_{}_config", service, resource_snake),
        });
    }
    entries.sort_by(|a, b| {
        a.service
            .cmp(&b.service)
            .then(a.file_stem.cmp(&b.file_stem))
    });

    // Collect unique services (sorted)
    let mut services: Vec<&str> = entries.iter().map(|e| e.service.as_str()).collect();
    services.dedup();

    // Service module declarations
    for service in &services {
        code.push_str(&format!("pub mod {};\n", service));
    }

    // configs() function
    code.push_str(
        "\n/// Returns all generated schema configs\n\
         pub fn configs() -> Vec<AwsSchemaConfig> {\n\
         \x20   vec![\n",
    );
    for e in &entries {
        if e.config_fn.is_empty() {
            continue;
        }
        code.push_str(&format!(
            "\x20       {}::{}::{}(),\n",
            e.service, e.file_stem, e.config_fn
        ));
    }
    code.push_str(
        "\x20   ]\n\
         }\n\n",
    );

    // get_enum_valid_values() — DataSource modules also expose enum_valid_values()
    // (codegen always emits a stub), so include them.
    code.push_str(
        "/// Get valid enum values for a given resource type and attribute name.\n\
         /// Used during read-back to normalize AWS-returned values to canonical DSL form.\n\
         ///\n\
         /// Auto-generated from schema enum constants.\n\
         #[allow(clippy::type_complexity)]\n\
         pub fn get_enum_valid_values(resource_type: &str, attr_name: &str) -> Option<&'static [&'static str]> {\n\
         \x20   let modules: &[(&str, &[(&str, &[&str])])] = &[\n",
    );
    for e in &entries {
        if e.config_fn.is_empty() {
            continue;
        }
        code.push_str(&format!(
            "\x20       {}::{}::enum_valid_values(),\n",
            e.service, e.file_stem
        ));
    }
    code.push_str(
        "\x20   ];\n\
         \x20   for (rt, attrs) in modules {\n\
         \x20       if *rt == resource_type {\n\
         \x20           for (attr, values) in *attrs {\n\
         \x20               if *attr == attr_name {\n\
         \x20                   return Some(values);\n\
         \x20               }\n\
         \x20           }\n\
         \x20           return None;\n\
         \x20       }\n\
         \x20   }\n\
         \x20   None\n\
         }\n\n",
    );

    code
}

// ── Provider boilerplate generation ──

/// Scan `services_dir/**/*.rs` for existing `fn <name>` method definitions.
///
/// Returns a set of method names (e.g. `"delete_ec2_vpc"`) that already have
/// manual implementations. The codegen uses this to skip generating duplicates
/// when a resource has both a `simple_delete`/`noop_update` flag and a hand-written
/// service file. Catches both `async fn` and plain `fn`. Returns an empty set
/// if the directory does not exist.
fn scan_manual_methods(services_dir: &std::path::Path) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut methods = HashSet::new();
    if !services_dir.exists() {
        return methods;
    }
    fn visit(dir: &std::path::Path, out: &mut HashSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in contents.lines() {
                // Find the `fn ` token. Accepts `async fn`, `pub fn`, `pub(crate) fn`, etc.
                let Some(after_fn) = line.split(" fn ").nth(1) else {
                    continue;
                };
                let Some((name, _)) = after_fn.split_once(['(', '<']) else {
                    continue;
                };
                out.insert(name.trim().to_string());
            }
        }
    }
    visit(services_dir, &mut methods);
    methods
}

/// Generate the provider_generated.rs file from ResourceDef metadata and Smithy models.
/// Uses Smithy models to resolve types for read/write helper generation.
/// Render the `Value::*` wrapping for a single child member of a
/// nested struct (used by `DerivedSource::FromStruct` emission).
/// Returns `None` for shapes the emitter cannot handle.
///
/// `value_var` is the local name (`"v"` in current call sites) bound
/// to the child accessor result via `if let Some(v) = ...`.
fn struct_child_value_expr(
    model: &SmithyModel,
    child_ref: &carina_smithy::ShapeRef,
    value_var: &str,
) -> Option<String> {
    let kind = model.shape_kind(&child_ref.target)?;
    let expr = match kind {
        ShapeKind::String => format!(
            "Value::Concrete(ConcreteValue::String({}.to_string().into()))",
            value_var
        ),
        ShapeKind::Boolean => {
            format!("Value::Concrete(ConcreteValue::Bool({}))", value_var)
        }
        // SDK getters return `i32` for Integer, `i64` for Long. Cast only
        // when widening — clippy::unnecessary_cast catches the no-op
        // `i64 as i64` shape.
        ShapeKind::Integer => format!("Value::Concrete(ConcreteValue::Int({} as i64))", value_var),
        ShapeKind::Long => format!("Value::Concrete(ConcreteValue::Int({}))", value_var),
        ShapeKind::Enum => format!(
            "Value::Concrete(ConcreteValue::String({}.as_str().to_string().into()))",
            value_var
        ),
        ShapeKind::List => {
            let element_kind = match model.get_shape(&child_ref.target) {
                Some(carina_smithy::Shape::List(list_shape)) => {
                    model.shape_kind(&list_shape.member.target)
                }
                _ => None,
            };
            if !matches!(element_kind, Some(ShapeKind::String)) {
                return None;
            }
            format!(
                "Value::Concrete(ConcreteValue::List({}.iter().map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into()))).collect()))",
                value_var
            )
        }
        _ => return None,
    };
    Some(expr)
}

#[derive(Debug)]
struct DirectReadField<'a> {
    attr_name: String,
    accessor_name: String,
    member_ref: &'a carina_smithy::ShapeRef,
}

fn direct_read_field_is_supported(
    model: &SmithyModel,
    member_ref: &carina_smithy::ShapeRef,
) -> bool {
    match model.shape_kind(&member_ref.target) {
        Some(
            ShapeKind::Enum
            | ShapeKind::Boolean
            | ShapeKind::Integer
            | ShapeKind::Long
            | ShapeKind::String,
        ) => true,
        Some(ShapeKind::List) => match model.get_shape(&member_ref.target) {
            Some(carina_smithy::Shape::List(list_shape)) => matches!(
                model.shape_kind(&list_shape.member.target),
                Some(ShapeKind::String)
            ),
            _ => false,
        },
        _ => false,
    }
}

fn direct_read_fields<'a>(
    res: &ResourceDef,
    model: &'a SmithyModel,
    read_struct: &'a carina_smithy::StructureShape,
) -> Vec<DirectReadField<'a>> {
    let exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();
    let omitted: HashSet<&str> = res
        .read_dispositions()
        .iter()
        .filter_map(|disposition| match disposition {
            resource_defs::ReadDisposition::OmitSchema { field, .. } => Some(*field),
            _ => None,
        })
        .collect();
    let create_input = if !res.create_op.is_empty() {
        let create_op_id = format!("{}#{}", res.service_namespace, res.create_op);
        model.operation_input(&create_op_id)
    } else {
        None
    };
    let schema_structure = res
        .schema_structure
        .and_then(|name| model.get_structure(&format!("{}#{name}", res.service_namespace)));
    let updatable_fields: HashSet<&str> = res
        .update_ops
        .iter()
        .flat_map(|operation| operation.fields.field_names().iter())
        .copied()
        .collect();
    let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();
    let mut fields = Vec::new();

    for (member_name, member_ref) in &read_struct.members {
        if exclude.contains(member_name.as_str())
            || (omitted.contains(member_name.as_str())
                && !is_read_model_alias_source(res.name, &member_name.to_snake_case()))
            || member_name.eq_ignore_ascii_case("tags")
        {
            continue;
        }
        let is_schema_attr = member_name == res.identifier
            || extra_read_only.contains(member_name.as_str())
            || updatable_fields.contains(member_name.as_str())
            || schema_structure.is_some_and(|schema| schema.members.contains_key(member_name))
            || create_input.is_some_and(|input| input.members.contains_key(member_name));
        if !is_schema_attr || !direct_read_field_is_supported(model, member_ref) {
            continue;
        }
        let snake_name = member_name.to_snake_case();
        fields.push(DirectReadField {
            attr_name: snake_name.clone(),
            accessor_name: snake_name,
            member_ref,
        });
    }

    for extra in &res.extra_attributes {
        if let Some(read_source) = extra.read_source
            && let Some(member_ref) = read_struct.members.get(read_source)
            && direct_read_field_is_supported(model, member_ref)
        {
            let attr_name = extra.name.to_snake_case();
            let accessor_name = read_source.to_snake_case();
            if !fields.iter().any(|field| field.attr_name == attr_name) {
                fields.push(DirectReadField {
                    attr_name,
                    accessor_name,
                    member_ref,
                });
            }
        }
    }

    fields.sort_by(|left, right| left.attr_name.cmp(&right.attr_name));
    fields
}

fn extracted_read_field_names(
    res: &ResourceDef,
    direct_fields: &[DirectReadField<'_>],
    readable: &[&AttrInfo],
) -> Vec<String> {
    let readable_names: HashSet<&str> = readable
        .iter()
        .map(|attribute| attribute.snake_name.as_str())
        .collect();
    let mut names: Vec<String> = direct_fields
        .iter()
        .map(|field| field.attr_name.clone())
        .chain(
            res.derived_attributes
                .iter()
                .filter(|derived| readable_names.contains(derived.attr.to_snake_case().as_str()))
                .map(|derived| derived.attr.to_snake_case()),
        )
        .collect();
    names.sort();
    names.dedup();
    names
}

fn read_fields_type_name(resource_name: &str) -> String {
    let (service, resource) = split_service_resource(resource_name);
    format!("{}{}ReadFields", service.to_upper_camel_case(), resource)
}

fn read_model_supplement_type_name(resource_name: &str) -> String {
    let (service, resource) = split_service_resource(resource_name);
    format!(
        "{}{}ReadModelSupplement",
        service.to_upper_camel_case(),
        resource
    )
}

/// Return the extracted SDK field that supplies a differently named canonical
/// read-model field. Every other extracted field maps by snake_case name.
fn read_model_field_alias(resource_name: &str, model_field: &str) -> Option<&'static str> {
    match (resource_name, model_field) {
        ("ec2.Eip", "address") => Some("public_ip"),
        ("organizations.Account", "account_name") => Some("name"),
        _ => None,
    }
}

fn is_read_model_alias_source(resource_name: &str, source: &str) -> bool {
    matches!(
        (resource_name, source),
        ("ec2.Eip", "public_ip") | ("organizations.Account", "name")
    )
}

fn read_model_field_sources(
    res: &ResourceDef,
    extracted_field_names: &[String],
    readable: &[&AttrInfo],
) -> HashMap<String, String> {
    let extracted: HashSet<&str> = extracted_field_names.iter().map(String::as_str).collect();
    let mut sources = HashMap::new();
    for attr in readable {
        let source =
            read_model_field_alias(res.name, &attr.snake_name).unwrap_or(attr.snake_name.as_str());
        if extracted.contains(source) {
            sources.insert(attr.snake_name.clone(), source.to_string());
        }
    }

    for extracted_name in extracted_field_names {
        if !sources.values().any(|source| source == extracted_name) {
            panic!(
                "codegen: typed read field {}.{} is not consumed by the canonical read model; add an explicit alias or classification",
                res.name, extracted_name
            );
        }
    }
    sources
}

fn generate_provider_code(
    all_resources: &[ResourceDef],
    all_data_sources: &[resource_defs::DataSourceDef],
    models: &HashMap<&str, SmithyModel>,
    manual_methods: &std::collections::HashSet<String>,
) -> String {
    if let Some(resource) = all_resources
        .iter()
        .find(|resource| !resource.read_ops.is_empty())
    {
        panic!(
            "codegen: legacy map-based read_ops are unsupported for {}; add the response fields to its typed read extractor",
            resource.name
        );
    }

    let mut code = String::new();

    // Header
    code.push_str(
        "//! Auto-generated provider boilerplate\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./carina-provider-aws/scripts/generate-provider.sh\n\n\
         use indexmap::IndexMap;\n\
         use carina_core::provider::{\n\
         \x20   AppliedIdentifier, BoxFuture, ProviderError, ProviderResult, ReadOutcome,\n\
         };\n\
         use carina_core::resource::{\n\
         \x20   ConcreteValue, DataSource, Resource, ResourceId, State, Value,\n\
         };\n\
         use crate::AwsProvider;\n\
         use crate::error_helpers::api_error_with_meta;\n\n",
    );

    // Generate methods on AwsProvider
    code.push_str("// ===== Generated Methods on AwsProvider =====\n\n");

    for res in all_resources
        .iter()
        .filter(|resource| resource.read_structure.is_some())
    {
        let resource_name = module_name(res.name);
        if manual_methods.contains(&format!("extract_{}_attributes", resource_name)) {
            continue;
        }
        let Some(model) = models.get(res.service_namespace) else {
            continue;
        };
        let read_struct_name = res.read_structure.expect("filtered above");
        let read_struct_id = format!("{}#{}", res.service_namespace, read_struct_name);
        let Some(read_struct) = model.get_structure(&read_struct_id) else {
            continue;
        };
        let direct_fields = direct_read_fields(res, model, read_struct);
        let fields_type = read_fields_type_name(res.name);
        let model_type = read_model_type_name(res.name);
        let supplement_type = read_model_supplement_type_name(res.name);
        let collected = collect_resource_attributes(res, model)
            .unwrap_or_else(|error| panic!("codegen: {} read model: {error}", res.name));
        let readable: Vec<&AttrInfo> = collected
            .attrs
            .iter()
            .filter(|attr| !attr.is_write_only)
            .collect();
        let field_names = extracted_read_field_names(res, &direct_fields, &readable);
        let model_sources = read_model_field_sources(res, &field_names, &readable);
        let supplemental: Vec<&&AttrInfo> = readable
            .iter()
            .filter(|attr| !model_sources.contains_key(&attr.snake_name))
            .collect();

        code.push_str(&format!(
            "/// Typed values extracted from the AWS read shape for `{}`.\n\
             #[allow(dead_code)]\n\
             pub(crate) struct {} {{\n",
            res.name, fields_type
        ));
        for field in &field_names {
            let field = escape_rust_keyword(field);
            code.push_str(&format!("    {field}: Option<Value>,\n"));
        }
        code.push_str("    applied_identifier: Option<String>,\n");
        code.push_str("}\n\n");

        code.push_str(&format!(
            "#[allow(dead_code, clippy::wrong_self_convention)]\nimpl {fields_type} {{\n"
        ));
        for field_name in &field_names {
            let field = escape_rust_keyword(field_name);
            let map_method = escape_rust_keyword(&format!("map_{field_name}"));
            code.push_str(&format!(
                "    pub(crate) fn {field}(&self) -> Option<&Value> {{\n\
                         self.{field}.as_ref()\n\
                     }}\n\n\
                     pub(crate) fn {map_method}(\n\
                         mut self,\n\
                         map: impl FnOnce(Option<Value>) -> Option<Value>,\n\
                     ) -> Self {{\n\
                         self.{field} = map(self.{field});\n\
                         self\n\
                     }}\n\n"
            ));
        }
        code.push_str(
            "    pub(crate) fn applied_identifier(&self) -> &Option<String> {\n\
                     &self.applied_identifier\n\
                 }\n\n",
        );
        code.push_str(&format!(
            "    pub(crate) fn into_model(\n\
                     self,\n\
                     resource_id: &ResourceId,\n\
                     supplement: crate::read_models_generated::{supplement_type},\n\
                 ) -> ProviderResult<(Option<String>, crate::read_models_generated::{model_type})> {{\n"
        ));
        code.push_str("        let Self {\n");
        for field_name in &field_names {
            let field = escape_rust_keyword(field_name);
            code.push_str(&format!("            {field},\n"));
        }
        code.push_str("            applied_identifier,\n        } = self;\n");
        code.push_str(&format!(
            "        let crate::read_models_generated::{supplement_type} {{\n"
        ));
        for attr in &supplemental {
            let field = escape_rust_keyword(&attr.snake_name);
            code.push_str(&format!("            {field},\n"));
        }
        if res.has_tags {
            code.push_str("            tags,\n");
        }
        code.push_str("        } = supplement;\n");
        code.push_str("        let _ = resource_id;\n");
        code.push_str(&format!(
            "        let model = crate::read_models_generated::{model_type} {{\n"
        ));
        for attr in &readable {
            let field = escape_rust_keyword(&attr.snake_name);
            if let Some(source) = model_sources.get(&attr.snake_name) {
                let source = escape_rust_keyword(source);
                match attr.read_presence {
                    resource_defs::ReadPresence::Required => code.push_str(&format!(
                        "            {field}: crate::read_model::require_read_value(resource_id, {source}.clone(), \"{}\")?,\n",
                        attr.snake_name
                    )),
                    resource_defs::ReadPresence::Optional => code.push_str(&format!(
                        "            {field}: {source}.clone(),\n"
                    )),
                }
            } else {
                code.push_str(&format!("            {field},\n"));
            }
        }
        if res.has_tags {
            code.push_str("            tags,\n");
        }
        code.push_str(
            "        };\n\
                     Ok((applied_identifier, model))\n\
                 }\n\
             }\n\n",
        );
    }

    // Some typed SDK projections are unused when a managed read needs a
    // different response shape or additional side calls. No-op update helpers
    // can likewise be unused when the service calls its read directly. Emit
    // these uniformly and gate dead-code here instead of pruning case-by-case.
    code.push_str("#[allow(dead_code)]\nimpl AwsProvider {\n");

    // Simple delete methods
    for res in all_resources.iter().filter(|r| r.simple_delete) {
        let (service, resource) = split_service_resource_snake(res.name);
        let method_name = format!("delete_{}_{}", service, resource);
        if manual_methods.contains(&method_name) {
            continue;
        }
        let client_field = client_field_name(res.service_namespace);
        let sdk_method = res.delete_op.to_snake_case();
        let id_setter = res.identifier.to_snake_case();

        // Human-readable resource name for error message. The final segment
        // is PascalCase under the naming-conventions rule; snake_case it and
        // convert underscores to spaces for display ("SecurityGroup" -> "security group").
        let display_name =
            pascal_to_snake(res.name.split('.').next_back().unwrap_or(res.name)).replace('_', " ");

        code.push_str(&format!(
            "\x20   /// Delete {} (generated)\n\
             \x20   pub(crate) async fn {}(\n\
             \x20       &self,\n\
             \x20       id: ResourceId,\n\
             \x20       identifier: &AppliedIdentifier,\n\
             \x20   ) -> ProviderResult<()> {{\n\
             \x20       self.{}.{}().{}(identifier.as_str()).send().await.map_err(|e| {{\n\
             \x20           api_error_with_meta(\"Failed to delete {}\", e)\n\
             \x20               .for_resource(id.clone())\n\
             \x20       }})?;\n\
             \x20       Ok(())\n\
             \x20   }}\n\n",
            res.name, method_name, client_field, sdk_method, id_setter, display_name,
        ));
    }

    // No-op update methods (with optional tag handling)
    for res in all_resources.iter().filter(|r| r.noop_update) {
        let method_name = format!("update_{}", module_name(res.name));
        if manual_methods.contains(&method_name) {
            continue;
        }
        let read_method = format!("read_{}", module_name(res.name));

        if res.has_tags {
            // Tag-enabled noop update: apply tags then read back
            code.push_str(&format!(
                "\x20   /// Update {}: apply tag changes and read back (generated)\n\
                 \x20   pub(crate) async fn {}(\n\
                 \x20       &self,\n\
                 \x20       id: ResourceId,\n\
                 \x20       identifier: &AppliedIdentifier,\n\
                 \x20       from: &State,\n\
                 \x20       to: Resource,\n\
                 \x20   ) -> ProviderResult<ProviderResult<ReadOutcome>> {{\n\
                 \x20       self.apply_ec2_tags(&id, identifier.as_str(), &to.resolved_attributes(), Some(&from.attributes))\n\
                 \x20           .await?;\n\
                 \x20       Ok(self.{}(&id, identifier).await)\n\
                 \x20   }}\n\n",
                res.name, method_name, read_method,
            ));
        } else {
            code.push_str(&format!(
                "\x20   /// Update {} (no-op, just read back current state) (generated)\n\
                 \x20   pub(crate) async fn {}(\n\
                 \x20       &self,\n\
                 \x20       id: ResourceId,\n\
                 \x20       identifier: &AppliedIdentifier,\n\
                 \x20       _to: Resource,\n\
                 \x20   ) -> ProviderResult<ProviderResult<ReadOutcome>> {{\n\
                 \x20       Ok(self.{}(&id, identifier).await)\n\
                 \x20   }}\n\n",
                res.name, method_name, read_method,
            ));
        }
    }

    // Write helpers for update_ops with InsideStruct layout
    for res in all_resources.iter().filter(|r| {
        r.update_ops
            .iter()
            .any(|op| matches!(op.fields, resource_defs::FieldLayout::InsideStruct { .. }))
    }) {
        let model = match models.get(res.service_namespace) {
            Some(m) => m,
            None => continue,
        };
        let ns = res.service_namespace;
        let client_field = client_field_name(ns);
        let id_setter = res.identifier.to_snake_case();
        let resource_name = module_name(res.name);
        let sdk_crate_name = sdk_crate_name(ns);

        // Build reverse rename map from read_ops: effective_name -> original_smithy_name
        let mut reverse_rename: HashMap<&str, &str> = HashMap::new();
        for read_op in &res.read_ops {
            for (field_name, rename) in &read_op.fields {
                if let Some(renamed) = rename {
                    reverse_rename.insert(renamed, field_name);
                }
            }
        }

        for update_op in &res.update_ops {
            let resource_defs::FieldLayout::InsideStruct {
                name: struct_name,
                fields: update_fields,
            } = &update_op.fields
            else {
                continue;
            };
            let suffix = op_suffix(update_op.operation, res.identifier);
            let method_name = format!("write_{}_{}", resource_name, suffix);
            if manual_methods.contains(&method_name) {
                continue;
            }
            // Skip generating the write function if any of the struct fields
            // has a `type_overrides` entry: the override changes the DSL Value
            // type (e.g. Bool, Json) but the generated builder body assumes
            // `Value::String`. The provider-side write must be hand-written
            // in services/<service>/<resource>.rs.
            if update_fields
                .iter()
                .any(|f| res.type_overrides.iter().any(|(k, _)| *k == *f))
            {
                continue;
            }
            let sdk_method = update_op.operation.to_snake_case();
            let struct_setter = struct_name.to_snake_case();

            // Resolve the nested struct from the Put input
            let op_id = format!("{}#{}", ns, update_op.operation);
            let input = match model.operation_input(&op_id) {
                Some(i) => i,
                None => continue,
            };
            let struct_ref = match input.members.get(*struct_name) {
                Some(r) => r,
                None => continue,
            };
            let nested_struct = match model.get_structure(&struct_ref.target) {
                Some(s) => s,
                None => continue,
            };
            let struct_type_name = SmithyModel::shape_name(&struct_ref.target);

            // Collect field info and use types
            struct FieldInfo {
                attr_snake: String,
                builder_setter: String,
                enum_type_name: Option<String>,
            }
            let mut fields = Vec::new();
            let mut use_types: Vec<String> = vec![struct_type_name.to_string()];

            for effective_name in update_fields {
                let original_name = reverse_rename
                    .get(*effective_name)
                    .copied()
                    .unwrap_or(effective_name);
                let attr_snake = effective_name.to_snake_case();
                let builder_setter = escape_rust_keyword(&original_name.to_snake_case());

                // Look up field in nested struct to resolve enum type
                let enum_type_name =
                    if let Some(member_ref) = nested_struct.members.get(original_name) {
                        if matches!(model.shape_kind(&member_ref.target), Some(ShapeKind::Enum)) {
                            let type_name = SmithyModel::shape_name(&member_ref.target).to_string();
                            use_types.push(type_name.clone());
                            Some(type_name)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                fields.push(FieldInfo {
                    attr_snake,
                    builder_setter,
                    enum_type_name,
                });
            }

            let op_desc = sdk_method.replace('_', " ");
            let use_list = use_types.join(", ");

            // Method signature
            code.push_str(&format!(
                "\x20   /// Write {} {} (generated)\n",
                res.name, update_op.operation
            ));
            code.push_str(&format!("\x20   pub(crate) async fn {}(\n", method_name));
            code.push_str("\x20       &self,\n");
            code.push_str("\x20       id: &ResourceId,\n");
            code.push_str("\x20       identifier: &str,\n");
            code.push_str("\x20       attributes: &std::collections::HashMap<String, Value>,\n");
            code.push_str("\x20   ) -> ProviderResult<()> {\n");
            code.push_str(&format!(
                "\x20       use {}::types::{{{}}};\n",
                sdk_crate_name, use_list
            ));

            // Build nested struct
            code.push_str(&format!(
                "\x20       let mut builder = {}::builder();\n",
                struct_type_name
            ));
            code.push_str("\x20       let mut has_changes = false;\n");

            for field in &fields {
                if let Some(ref enum_type) = field.enum_type_name {
                    code.push_str(&format!(
                        "\x20       if let Some(value) = attributes.get(\"{}\") {{\n",
                        field.attr_snake
                    ));
                    code.push_str("\x20           let normalized = match value {\n");
                    code.push_str(
                        "\x20               Value::Concrete(ConcreteValue::String(val)) => val.expose_plaintext().to_string(),\n",
                    );
                    code.push_str(
                        "\x20               Value::Concrete(ConcreteValue::CanonicalEnum(val)) => val.api_value().to_string(),\n",
                    );
                    code.push_str("\x20               _ => continue,\n");
                    code.push_str("\x20           };\n");
                    code.push_str(&format!(
                        "\x20           builder = builder.{}({}::from(normalized));\n",
                        field.builder_setter, enum_type
                    ));
                } else {
                    code.push_str(&format!(
                        "\x20       if let Some(Value::Concrete(ConcreteValue::String(val))) = attributes.get(\"{}\") {{\n",
                        field.attr_snake
                    ));
                    code.push_str(&format!(
                        "\x20           builder = builder.{}(val.expose_plaintext());\n",
                        field.builder_setter
                    ));
                }
                code.push_str("\x20           has_changes = true;\n");
                code.push_str("\x20       }\n");
            }

            // Send API call
            code.push_str("\x20       if has_changes {\n");
            code.push_str("\x20           let config = builder.build();\n");
            code.push_str(&format!(
                "\x20           self.{}.{}().{}(identifier).{}(config).send().await.map_err(|e| {{\n",
                client_field, sdk_method, id_setter, struct_setter
            ));
            code.push_str(&format!(
                "\x20               api_error_with_meta(\"Failed to {}\", e)\n",
                op_desc,
            ));
            code.push_str("\x20                   .for_resource(id.clone())\n");
            code.push_str("\x20           })?;\n");
            code.push_str("\x20       }\n");
            code.push_str("\x20       Ok(())\n");
            code.push_str("\x20   }\n\n");
        }
    }

    // Read attribute extraction methods for resources with read_structure
    for res in all_resources.iter().filter(|r| r.read_structure.is_some()) {
        let model = match models.get(res.service_namespace) {
            Some(m) => m,
            None => continue,
        };
        let ns = res.service_namespace;
        let read_struct_name = res.read_structure.unwrap();
        let read_struct_id = format!("{}#{}", ns, read_struct_name);
        let read_struct = match model.get_structure(&read_struct_id) {
            Some(s) => s,
            None => continue,
        };

        let resource_name = module_name(res.name);
        if manual_methods.contains(&format!("extract_{}_attributes", resource_name)) {
            continue;
        }
        let sdk_crate = sdk_crate_name(ns);
        let direct_fields = direct_read_fields(res, model, read_struct);
        let collected = collect_resource_attributes(res, model)
            .unwrap_or_else(|error| panic!("codegen: {} read extractor: {error}", res.name));
        let readable: Vec<&AttrInfo> = collected
            .attrs
            .iter()
            .filter(|attr| !attr.is_write_only)
            .collect();
        let extracted_field_names = extracted_read_field_names(res, &direct_fields, &readable);
        let fields_type = read_fields_type_name(res.name);

        // Generate method
        code.push_str(&format!(
            "\x20   /// Extract {} attributes from SDK response type (generated)\n",
            res.name
        ));
        code.push_str(&format!(
            "\x20   pub(crate) fn extract_{}_attributes(\n",
            resource_name
        ));
        code.push_str(&format!(
            "\x20       obj: &{}::types::{},\n",
            sdk_crate, read_struct_name
        ));
        code.push_str(&format!("\x20   ) -> {fields_type} {{\n"));

        for DirectReadField {
            attr_name,
            accessor_name,
            member_ref,
        } in &direct_fields
        {
            let kind = model.shape_kind(&member_ref.target);
            let accessor = escape_rust_keyword(accessor_name);
            let field = escape_rust_keyword(attr_name);
            // Smithy `@required` members generate accessors that return the
            // value directly (`-> &str`, `-> bool`, etc.) rather than as
            // `Option`. Branch the emission so required strings hit
            // `let v = obj.<f>(); if !v.is_empty()` and required scalars
            // skip the `if let` entirely. Optional members keep the
            // existing `if let Some(v)` shape.
            let required = SmithyModel::is_required(member_ref);

            match kind {
                Some(ShapeKind::Enum) => {
                    if required {
                        code.push_str(&format!(
                            "\x20       let {field} = Some(Value::Concrete(ConcreteValue::String(obj.{accessor}().as_str().to_string().into())));\n"
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       let {field} = obj.{accessor}().map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));\n"
                        ));
                    }
                }
                Some(ShapeKind::Boolean) => {
                    if required {
                        code.push_str(&format!(
                            "\x20       let {field} = Some(Value::Concrete(ConcreteValue::Bool(obj.{accessor}())));\n"
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       let {field} = obj.{accessor}().map(|v| Value::Concrete(ConcreteValue::Bool(v)));\n"
                        ));
                    }
                }
                Some(int_kind @ (ShapeKind::Integer | ShapeKind::Long)) => {
                    // SDK getters return `i32` for Integer, `i64` for
                    // Long. Cast only when widening — clippy's
                    // `unnecessary_cast` catches the no-op `i64 as i64`
                    // shape that would otherwise be emitted for Long.
                    let cast = if matches!(int_kind, ShapeKind::Integer) {
                        " as i64"
                    } else {
                        ""
                    };
                    if required {
                        code.push_str(&format!(
                            "\x20       let {field} = Some(Value::Concrete(ConcreteValue::Int(obj.{accessor}(){cast})));\n"
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       let {field} = obj.{accessor}().map(|v| Value::Concrete(ConcreteValue::Int(v{cast})));\n"
                        ));
                    }
                }
                Some(ShapeKind::String) => {
                    if required {
                        // Required strings: skip empty-string sentinels the
                        // SDK uses for "missing on the wire," matching the
                        // pre-existing hand edits in provider_generated.rs.
                        code.push_str(&format!(
                            "\x20       let v = obj.{accessor}();\n\
                             \x20       let {field} = (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));\n"
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       let {field} = obj.{accessor}().map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));\n"
                        ));
                    }
                }
                Some(carina_smithy::ShapeKind::List) => {
                    // Flatten List<String> response fields directly into
                    // `Value::List(String)`. Other list element kinds
                    // (struct, list-of-list, etc.) still need declarative
                    // help from `derived_attributes` and stay skipped here.
                    let element_kind = match model.get_shape(&member_ref.target) {
                        Some(carina_smithy::Shape::List(list_shape)) => {
                            model.shape_kind(&list_shape.member.target)
                        }
                        _ => None,
                    };
                    if matches!(element_kind, Some(carina_smithy::ShapeKind::String)) {
                        code.push_str(&format!(
                            "\x20       let ids = obj.{accessor}();\n\
                             \x20       let {field} = (!ids.is_empty()).then(|| {{\n\
                             \x20           let list: Vec<Value> = ids.iter().map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into()))).collect();\n\
                             \x20           Value::Concrete(ConcreteValue::List(list))\n\
                             \x20       }});\n"
                        ));
                    }
                    // Non-string-element lists need a declarative
                    // projection (DerivedSource::ListAll) — handled below.
                }
                _ => unreachable!("unsupported direct read fields were filtered above"),
            }
        }

        // Emit derived attributes (DerivedSource projections). These are
        // attributes whose value comes from a non-trivial walk of the read
        // structure (e.g. `nat_gateway_addresses[0].allocation_id`); the
        // regular fields_to_extract loop above can only handle direct member
        // accessors.
        for derived in &res.derived_attributes {
            match &derived.source {
                resource_defs::DerivedSource::ListFirst {
                    list_member,
                    child_member,
                } => {
                    // Locate the list member on the read structure, follow
                    // its target shape to a List, then to the list element
                    // (a Structure), then look up the child member to
                    // determine the right unwrap pattern.
                    let Some(list_ref) = read_struct.members.get(*list_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: list member '{}' not found on read structure",
                            res.name, list_member
                        );
                        continue;
                    };
                    let element_struct_id = match model.get_shape(&list_ref.target) {
                        Some(carina_smithy::Shape::List(list_shape)) => {
                            list_shape.member.target.as_str()
                        }
                        _ => {
                            eprintln!(
                                "warning: derived_attributes for {}: '{}' is not a list shape",
                                res.name, list_member
                            );
                            continue;
                        }
                    };
                    let Some(element_struct) = model.get_structure(element_struct_id) else {
                        eprintln!(
                            "warning: derived_attributes for {}: list element '{}' is not a structure",
                            res.name, element_struct_id
                        );
                        continue;
                    };
                    let Some(child_ref) = element_struct.members.get(*child_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: child member '{}' not found on '{}'",
                            res.name, child_member, element_struct_id
                        );
                        continue;
                    };

                    let attr_snake = derived.attr.to_snake_case();
                    let list_snake = escape_rust_keyword(&list_member.to_snake_case());
                    let child_snake = escape_rust_keyword(&child_member.to_snake_case());
                    let child_kind = model.shape_kind(&child_ref.target);
                    // Today's two call sites (NatGateway, EgressOnlyInternetGateway)
                    // both project an optional String off the list element. Other
                    // shape kinds and required-child variants will appear when
                    // future ResourceDefs use ListFirst with a different child
                    // type — surface them loudly so we extend the emitter
                    // instead of silently emitting wrong code.
                    if !matches!(child_kind, Some(carina_smithy::ShapeKind::String)) {
                        eprintln!(
                            "warning: derived_attributes for {}: ListFirst child '{}' is not a String; skipping (extend the emitter when this shape ships)",
                            res.name, child_member
                        );
                        continue;
                    }
                    if SmithyModel::is_required(child_ref) {
                        eprintln!(
                            "warning: derived_attributes for {}: ListFirst child '{}' is required; skipping (extend the emitter when this shape ships)",
                            res.name, child_member
                        );
                        continue;
                    }

                    code.push_str(&format!(
                        "\x20       let {attr_snake} = obj.{list_snake}().first()\n\
                         \x20           .and_then(|item| item.{child_snake}())\n\
                         \x20           .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));\n"
                    ));
                }
                resource_defs::DerivedSource::ListAll {
                    list_member,
                    child_member,
                } => {
                    // Same shape walk as ListFirst: read structure → list
                    // shape → list element struct → child member.
                    let Some(list_ref) = read_struct.members.get(*list_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: list member '{}' not found on read structure",
                            res.name, list_member
                        );
                        continue;
                    };
                    let element_struct_id = match model.get_shape(&list_ref.target) {
                        Some(carina_smithy::Shape::List(list_shape)) => {
                            list_shape.member.target.as_str()
                        }
                        _ => {
                            eprintln!(
                                "warning: derived_attributes for {}: '{}' is not a list shape",
                                res.name, list_member
                            );
                            continue;
                        }
                    };
                    let Some(element_struct) = model.get_structure(element_struct_id) else {
                        eprintln!(
                            "warning: derived_attributes for {}: list element '{}' is not a structure",
                            res.name, element_struct_id
                        );
                        continue;
                    };
                    let Some(child_ref) = element_struct.members.get(*child_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: child member '{}' not found on '{}'",
                            res.name, child_member, element_struct_id
                        );
                        continue;
                    };
                    let attr_snake = derived.attr.to_snake_case();
                    let list_snake = escape_rust_keyword(&list_member.to_snake_case());
                    let child_snake = escape_rust_keyword(&child_member.to_snake_case());
                    let child_kind = model.shape_kind(&child_ref.target);
                    if !matches!(child_kind, Some(carina_smithy::ShapeKind::String)) {
                        eprintln!(
                            "warning: derived_attributes for {}: ListAll child '{}' is not a String; skipping (extend the emitter when this shape ships)",
                            res.name, child_member
                        );
                        continue;
                    }
                    if SmithyModel::is_required(child_ref) {
                        eprintln!(
                            "warning: derived_attributes for {}: ListAll child '{}' is required; skipping (extend the emitter when this shape ships)",
                            res.name, child_member
                        );
                        continue;
                    }
                    code.push_str(&format!(
                        "\x20       let list: Vec<Value> = obj.{list_snake}()\n\
                         \x20           .iter()\n\
                         \x20           .filter_map(|item| item.{child_snake}().map(|id| Value::Concrete(ConcreteValue::String(id.to_string().into()))))\n\
                         \x20           .collect();\n\
                         \x20       let {attr_snake} = (!list.is_empty()).then(|| Value::Concrete(ConcreteValue::List(list)));\n"
                    ));
                }
                resource_defs::DerivedSource::FromStruct {
                    struct_member,
                    children,
                } => {
                    if children.is_empty() {
                        panic!(
                            "codegen: FromStruct for {}'s derived attribute '{}' must list at least one child member",
                            res.name, derived.attr
                        );
                    }
                    if !schema_declares_attribute(res, model, derived.attr) {
                        panic!(
                            "codegen: FromStruct's `attr` '{}' is not declared in the resource's schema for {}",
                            derived.attr, res.name
                        );
                    }
                    let schema_attr_kind = schema_attribute_shape_kind(res, model, derived.attr)
                        .expect("schema_declares_attribute already checked schema presence");
                    let attr_snake = derived.attr.to_snake_case();

                    let Some(struct_ref) = read_struct.members.get(*struct_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: struct member '{}' not found on read structure",
                            res.name, struct_member
                        );
                        continue;
                    };
                    let Some(inner_struct) = model.get_structure(&struct_ref.target) else {
                        eprintln!(
                            "warning: derived_attributes for {}: '{}' is not a structure",
                            res.name, struct_member
                        );
                        continue;
                    };
                    let struct_snake = escape_rust_keyword(&struct_member.to_snake_case());
                    let name_says_nested_map = attr_snake == struct_member.to_snake_case();
                    let schema_says_nested_map = matches!(schema_attr_kind, ShapeKind::Structure);
                    if name_says_nested_map != schema_says_nested_map {
                        panic!(
                            "codegen: FromStruct for {}'s derived attribute '{}': name-equality heuristic and schema-shape lookup disagree on layout (name says {}, schema says {}). Misclassification rejected at codegen post-#448; the schema-shape signal is authoritative — fix the resource_defs.rs entry so both signals agree.",
                            res.name, derived.attr, name_says_nested_map, schema_says_nested_map
                        );
                    }
                    let is_nested_map_layout = schema_says_nested_map;
                    if !is_nested_map_layout && children.len() != 1 {
                        panic!(
                            "codegen: FromStruct for {}'s derived attribute '{}' emits a top-level leaf attribute '{}' but lists more than one child; declare separate DerivedAttribute entries for each rename target",
                            res.name, derived.attr, attr_snake
                        );
                    }

                    if is_nested_map_layout {
                        code.push_str(&format!(
                            "\x20       let {attr_snake} = obj.{struct_snake}().and_then(|dns_opts| {{\n\
                             \x20           let mut fields = IndexMap::new();\n"
                        ));
                        for child_name in *children {
                            let Some(child_ref) = inner_struct.members.get(*child_name) else {
                                eprintln!(
                                    "warning: derived_attributes for {}: child member '{}' not found on '{}'",
                                    res.name, child_name, struct_ref.target
                                );
                                continue;
                            };
                            let child_snake = escape_rust_keyword(&child_name.to_snake_case());
                            let child_kind = model.shape_kind(&child_ref.target);
                            let Some(insert_expr) = struct_child_value_expr(model, child_ref, "v")
                            else {
                                eprintln!(
                                    "warning: derived_attributes for {}: FromStruct child '{}' has unsupported shape; skipping",
                                    res.name, child_name
                                );
                                continue;
                            };
                            if matches!(child_kind, Some(ShapeKind::List)) {
                                code.push_str(&format!(
                                    "\x20           {{\n\
                                     \x20               let v = dns_opts.{}();\n\
                                     \x20               if !v.is_empty() {{\n\
                                     \x20                   fields.insert(\"{}\".to_string(), {});\n\
                                     \x20               }}\n\
                                     \x20           }}\n",
                                    child_snake, child_snake, insert_expr
                                ));
                            } else {
                                code.push_str(&format!(
                                    "\x20           if let Some(v) = dns_opts.{}() {{\n\
                                     \x20               fields.insert(\"{}\".to_string(), {});\n\
                                     \x20           }}\n",
                                    child_snake, child_snake, insert_expr
                                ));
                            }
                        }
                        code.push_str(
                            "\x20           (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))\n\
                             \x20       });\n",
                        );
                    } else {
                        let child_name = children[0];
                        let Some(child_ref) = inner_struct.members.get(child_name) else {
                            eprintln!(
                                "warning: derived_attributes for {}: child member '{}' not found on '{}'",
                                res.name, child_name, struct_ref.target
                            );
                            continue;
                        };
                        let child_snake = escape_rust_keyword(&child_name.to_snake_case());
                        let Some(insert_expr) = struct_child_value_expr(model, child_ref, "v")
                        else {
                            eprintln!(
                                "warning: derived_attributes for {}: FromStruct child '{}' has unsupported shape; skipping",
                                res.name, child_name
                            );
                            continue;
                        };
                        code.push_str(&format!(
                            "\x20       let {attr_snake} = obj.{struct_snake}()\n\
                             \x20           .and_then(|opts| opts.{child_snake}())\n\
                             \x20           .map(|v| {insert_expr});\n"
                        ));
                    }
                }
            }
        }

        // Preserve the response identifier for create paths that need to turn
        // the just-created SDK object into an AppliedIdentifier.
        if !res.identifier.is_empty() && read_struct.members.contains_key(res.identifier) {
            let id_snake = escape_rust_keyword(&res.identifier.to_snake_case());
            let id_member = read_struct.members.get(res.identifier).unwrap();
            let id_required = SmithyModel::is_required(id_member);
            if id_required {
                code.push_str(&format!(
                    "\x20       let applied_identifier = Some(obj.{}().to_string());\n",
                    id_snake
                ));
            } else {
                code.push_str(&format!(
                    "\x20       let applied_identifier = obj.{}().map(String::from);\n",
                    id_snake
                ));
            }
        } else {
            code.push_str("\x20       let applied_identifier = None;\n");
        }

        code.push_str(&format!("\x20       {fields_type} {{\n"));
        for field in &extracted_field_names {
            let field = escape_rust_keyword(field);
            code.push_str(&format!("\x20           {field},\n"));
        }
        code.push_str("\x20           applied_identifier,\n");
        code.push_str("\x20       }\n");

        code.push_str("\x20   }\n\n");
    }

    code.push_str("}\n\n");

    // ===== DataSourceLookups trait =====
    code.push_str("// ===== Generated DataSourceLookups Trait =====\n\n");
    code.push_str(
        "/// One method per `DataSourceDef`. AwsProvider must implement all of\n\
         /// them; the codegen-emitted dispatcher below routes by\n\
         /// `resource.id.resource_type`.\n",
    );
    code.push_str("pub trait DataSourceLookups {\n");
    for ds in all_data_sources {
        let module = module_name(ds.name);
        code.push_str(&format!(
            "\x20   fn read_{}_data_source(\n\
             \x20       &self,\n\
             \x20       resource: &DataSource<impl Clone + Send + Sync + 'static>,\n\
             \x20   ) -> BoxFuture<'_, ProviderResult<State>>;\n\n",
            module,
        ));
    }
    code.push_str("}\n\n");

    // ===== read_data_source dispatcher =====
    code.push_str("// ===== Generated read_data_source dispatcher =====\n\n");
    code.push_str(
        "/// Routes `Provider::read_data_source` calls to the matching\n\
         /// `DataSourceLookups` trait method. The default arm refuses\n\
         /// to drop user-supplied inputs silently.\n",
    );
    code.push_str(
        "pub(crate) fn dispatch_read_data_source<'a>(\n\
         \x20   provider: &'a AwsProvider,\n\
         \x20   resource: &'a DataSource<impl Clone + Send + Sync + 'static>,\n\
         ) -> BoxFuture<'a, ProviderResult<State>> {\n\
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
         \x20               Err(ProviderError::internal(format!(\n\
         \x20                   \"aws provider does not implement read_data_source for '{}'\",\n\
         \x20                   resource_type\n\
         \x20               )).for_resource(id))\n\
         \x20           })\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    );

    code
}

fn read_model_type_name(resource_name: &str) -> String {
    let (service, resource) = split_service_resource(resource_name);
    format!("{}{}ReadModel", service.to_upper_camel_case(), resource)
}

fn data_source_read_model_type_name(resource_name: &str) -> String {
    let (service, resource) = split_service_resource(resource_name);
    format!(
        "{}{}DataSourceReadModel",
        service.to_upper_camel_case(),
        resource
    )
}

/// Generate the exhaustive managed-resource read models and their sole
/// Value-map conversion seam.
fn generate_read_models_code(
    all_resources: &[ResourceDef],
    all_data_sources: &[resource_defs::DataSourceDef],
    models: &HashMap<&str, SmithyModel>,
) -> Result<String> {
    let mut code = String::from(
        "//! Auto-generated managed-resource and data-source read models.\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./scripts/generate-provider.sh\n\n\
         use std::collections::HashMap;\n\n\
         use std::sync::OnceLock;\n\n\
         use carina_core::provider::ProviderResult;\n\
         use carina_core::resource::Value;\n\n\
         use carina_core::schema::ValidatedResourceSchema;\n\n\
         use crate::read_model::{\n\
         \x20   DataSourceReadModel, ManagedReadModel, cached_validated_read_schema, sealed,\n\
         \x20   validate_read_attributes,\n\
         };\n\n",
    );

    for res in all_resources {
        let model = models
            .get(res.service_namespace)
            .with_context(|| format!("missing Smithy model for {}", res.name))?;
        let collected = collect_resource_attributes(res, model)?;
        let readable: Vec<&AttrInfo> = collected
            .attrs
            .iter()
            .filter(|attr| !attr.is_write_only)
            .collect();
        let model_name = read_model_type_name(res.name);
        let (service, resource) = split_service_resource_snake(res.name);
        let config_fn = format!("{}_config", module_name(res.name));
        let model_supplement = res.read_structure.and_then(|read_structure| {
            let read_struct_id = format!("{}#{read_structure}", res.service_namespace);
            let read_struct = model.get_structure(&read_struct_id)?;
            let direct_fields = direct_read_fields(res, model, read_struct);
            let extracted = extracted_read_field_names(res, &direct_fields, &readable);
            let sources = read_model_field_sources(res, &extracted, &readable);
            let supplemental = readable
                .iter()
                .filter(|attr| !sources.contains_key(&attr.snake_name))
                .copied()
                .collect::<Vec<_>>();
            Some((read_model_supplement_type_name(res.name), supplemental))
        });

        code.push_str(&format!(
            "/// Exhaustive canonical read state for `{}`.\n\
             pub(crate) struct {} {{\n",
            res.name, model_name
        ));
        for attr in &readable {
            let field = escape_rust_keyword(&attr.snake_name);
            let field_type = match attr.read_presence {
                resource_defs::ReadPresence::Required => "Value",
                resource_defs::ReadPresence::Optional => "Option<Value>",
            };
            code.push_str(&format!("    pub(crate) {field}: {field_type},\n"));
        }
        if res.has_tags {
            code.push_str("    pub(crate) tags: Value,\n");
        }
        code.push_str("}\n\n");

        if let Some((supplement_name, supplemental)) = &model_supplement {
            code.push_str(&format!(
                "/// Canonical fields for `{}` supplied by identifiers, side reads, or complex SDK projections.\n\
                 pub(crate) struct {supplement_name} {{\n",
                res.name
            ));
            for attr in supplemental {
                let field = escape_rust_keyword(&attr.snake_name);
                let field_type = match attr.read_presence {
                    resource_defs::ReadPresence::Required => "Value",
                    resource_defs::ReadPresence::Optional => "Option<Value>",
                };
                code.push_str(&format!("    pub(crate) {field}: {field_type},\n"));
            }
            if res.has_tags {
                code.push_str("    pub(crate) tags: Value,\n");
            }
            code.push_str("}\n\n");
        }

        code.push_str(&format!("impl sealed::Sealed for {model_name} {{}}\n\n"));
        code.push_str(&format!(
            "impl ManagedReadModel for {model_name} {{\n\
             \x20   fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {{\n"
        ));
        code.push_str("        let Self {\n");
        for attr in &readable {
            let field = escape_rust_keyword(&attr.snake_name);
            code.push_str(&format!("            {field},\n"));
        }
        if res.has_tags {
            code.push_str("            tags,\n");
        }
        code.push_str("        } = self;\n");
        code.push_str(&format!(
            "        let mut attributes = HashMap::with_capacity({});\n",
            readable.len() + usize::from(res.has_tags)
        ));
        for attr in &readable {
            let field = escape_rust_keyword(&attr.snake_name);
            match attr.read_presence {
                resource_defs::ReadPresence::Required => code.push_str(&format!(
                    "        attributes.insert(\"{}\".to_string(), {field});\n",
                    attr.snake_name
                )),
                resource_defs::ReadPresence::Optional => code.push_str(&format!(
                    "        if let Some(value) = {field} {{\n\
                     \x20           attributes.insert(\"{}\".to_string(), value);\n\
                     \x20       }}\n",
                    attr.snake_name
                )),
            }
        }
        if res.has_tags {
            code.push_str("        attributes.insert(\"tags\".to_string(), tags);\n");
        }
        code.push_str(&format!(
            "        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> = OnceLock::new();\n\
             \x20       let schema = cached_validated_read_schema(\n\
             \x20           \"{}\",\n\
             \x20           &VALIDATED_SCHEMA,\n\
             \x20           || crate::schemas::generated::{service}::{resource}::{config_fn}().schema,\n\
             \x20       )?;\n\
             \x20       validate_read_attributes(\n\
             \x20           \"{}\",\n\
             \x20           schema,\n\
             \x20           &mut attributes,\n\
             \x20       )?;\n\
             \x20       Ok(attributes)\n\
             \x20   }}\n\
             }}\n\n",
            res.name, res.name
        ));
    }

    let managed_names = all_resources
        .iter()
        .map(|resource| resource.name)
        .collect::<HashSet<_>>();
    for data_source in all_data_sources {
        let mut fields = Vec::<(&str, bool)>::new();
        for input in &data_source.inputs {
            fields.push((input.name, input.required));
        }
        match &data_source.shape {
            resource_defs::DataSourceShape::Single { output_attributes } => {
                for output in output_attributes {
                    if !fields.iter().any(|(name, _)| *name == output.name) {
                        fields.push((output.name, false));
                    }
                }
            }
            resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => {
                for output in aggregated_outputs {
                    fields.push((output.name, false));
                }
            }
        }

        let model_name = data_source_read_model_type_name(data_source.name);
        code.push_str(&format!(
            "/// Exhaustive canonical data-source state for `{}`.\n\
             pub(crate) struct {model_name} {{\n",
            data_source.name
        ));
        for (name, required) in &fields {
            let field = escape_rust_keyword(name);
            let field_type = if *required { "Value" } else { "Option<Value>" };
            code.push_str(&format!("    pub(crate) {field}: {field_type},\n"));
        }
        code.push_str("}\n\n");
        code.push_str(&format!("impl sealed::Sealed for {model_name} {{}}\n\n"));
        code.push_str(&format!(
            "impl DataSourceReadModel for {model_name} {{\n\
             \x20   fn into_attributes(self) -> ProviderResult<HashMap<String, Value>> {{\n\
             \x20       let Self {{\n"
        ));
        for (name, _) in &fields {
            let field = escape_rust_keyword(name);
            code.push_str(&format!("            {field},\n"));
        }
        code.push_str("        } = self;\n");
        code.push_str(&format!(
            "        let mut attributes = HashMap::with_capacity({});\n",
            fields.len()
        ));
        for (name, required) in &fields {
            let field = escape_rust_keyword(name);
            if *required {
                code.push_str(&format!(
                    "        attributes.insert(\"{name}\".to_string(), {field});\n"
                ));
            } else {
                code.push_str(&format!(
                    "        if let Some(value) = {field} {{\n\
                     \x20           attributes.insert(\"{name}\".to_string(), value);\n\
                     \x20       }}\n"
                ));
            }
        }
        let (service, resource) = split_service_resource_snake(data_source.name);
        let dual_suffix = if managed_names.contains(data_source.name) {
            "_data_source"
        } else {
            ""
        };
        let module = format!("{resource}{dual_suffix}");
        let config_fn = format!("{}{dual_suffix}_config", module_name(data_source.name));
        code.push_str(&format!(
            "        static VALIDATED_SCHEMA: OnceLock<Result<ValidatedResourceSchema, String>> = OnceLock::new();\n\
             \x20       let schema = cached_validated_read_schema(\n\
             \x20           \"{}\",\n\
             \x20           &VALIDATED_SCHEMA,\n\
             \x20           || crate::schemas::generated::{service}::{module}::{config_fn}().schema,\n\
             \x20       )?;\n\
             \x20       validate_read_attributes(\n\
             \x20           \"{}\",\n\
             \x20           schema,\n\
             \x20           &mut attributes,\n\
             \x20       )?;\n\
             \x20       Ok(attributes)\n\
             \x20   }}\n\
             }}\n\n",
            data_source.name, data_source.name
        ));
    }

    Ok(code)
}

/// Returns true when `attr` (as a PascalCase / Smithy member name) is
/// declared as a top-level attribute in the resource's generated schema.
///
/// Single source of truth for the [`DerivedSource::FromStruct`] hard-error
/// gate: a derived attribute whose name is not in the schema is dead code
/// (carina-provider-aws#444 surfaced one such case — the orphan
/// `VpcPeeringConnection.PeerRegion` entry that wrote a state key invisible
/// to the schema / differ).
///
/// Schema-attribute sources checked (mirroring what `generate_resource`
/// emits as an `AttributeSchema` typestate builder): `extra_attributes`,
/// `extra_read_only`, `update_ops[].fields`, `schema_structure` members
/// (excluding `Tags` and `exclude_fields`), the create-op input members when
/// `schema_structure` is None, and `read_structure` members.
fn schema_declares_attribute(res: &ResourceDef, model: &SmithyModel, attr: &str) -> bool {
    schema_attribute_shape_kind(res, model, attr).is_some()
}

/// Returns the Smithy shape kind for a declared schema attribute, using the
/// same attribute-source walk as [`schema_declares_attribute`].
fn schema_attribute_shape_kind(
    res: &ResourceDef,
    model: &SmithyModel,
    attr: &str,
) -> Option<ShapeKind> {
    let exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();
    let attr_snake = attr.to_snake_case();

    let matches_attr = |name: &str| name.to_snake_case() == attr_snake;

    if let Some(extra) = res
        .extra_attributes
        .iter()
        .find(|extra| matches_attr(extra.name))
    {
        if let Some(source_field) = extra.read_source
            && let Some(read_struct_name) = res.read_structure
        {
            let read_structure_id = format!("{}#{}", res.service_namespace, read_struct_name);
            if let Some(read_struct) = model.get_structure(&read_structure_id)
                && let Some(member_ref) = read_struct.members.get(source_field)
            {
                return model.shape_kind(&member_ref.target);
            }
        }

        return Some(ShapeKind::String);
    }

    if res.extra_read_only.iter().any(|name| matches_attr(name)) {
        if let Some(read_struct_name) = res.read_structure {
            let read_structure_id = format!("{}#{}", res.service_namespace, read_struct_name);
            if let Some(read_struct) = model.get_structure(&read_structure_id)
                && let Some((_, member_ref)) = read_struct
                    .members
                    .iter()
                    .find(|(name, _)| matches_attr(name))
            {
                return model.shape_kind(&member_ref.target);
            }
        }

        return Some(ShapeKind::String);
    }

    let update_field = res
        .update_ops
        .iter()
        .flat_map(|op| op.fields.field_names())
        .find(|name| matches_attr(name));
    if let Some(update_field) = update_field {
        if let Some(member_ref) = schema_member_ref(res, model, update_field, &exclude) {
            return model.shape_kind(&member_ref.target);
        }
        return Some(ShapeKind::String);
    }

    if let Some(schema_struct_name) = res.schema_structure {
        let schema_structure_id = format!("{}#{}", res.service_namespace, schema_struct_name);
        if let Some(schema_struct) = model.get_structure(&schema_structure_id)
            && let Some((_, member_ref)) = schema_struct
                .members
                .iter()
                .filter(|(name, _)| {
                    !exclude.contains(name.as_str()) && !name.eq_ignore_ascii_case("tags")
                })
                .find(|(name, _)| matches_attr(name))
        {
            return model.shape_kind(&member_ref.target);
        }
    } else if !res.create_op.is_empty() {
        let create_op_id = format!("{}#{}", res.service_namespace, res.create_op);
        if let Some(create_input) = model.operation_input(&create_op_id)
            && let Some((_, member_ref)) = create_input
                .members
                .iter()
                .filter(|(name, _)| {
                    !exclude.contains(name.as_str()) && !name.eq_ignore_ascii_case("tags")
                })
                .find(|(name, _)| matches_attr(name))
        {
            return model.shape_kind(&member_ref.target);
        }
    }

    if let Some(read_struct_name) = res.read_structure {
        let read_structure_id = format!("{}#{}", res.service_namespace, read_struct_name);
        if let Some(read_struct) = model.get_structure(&read_structure_id)
            && let Some((_, member_ref)) = read_struct
                .members
                .iter()
                .filter(|(name, _)| {
                    !exclude.contains(name.as_str()) && !name.eq_ignore_ascii_case("tags")
                })
                .find(|(name, _)| matches_attr(name))
        {
            return model.shape_kind(&member_ref.target);
        }
    }

    None
}

fn schema_member_ref<'a>(
    res: &ResourceDef,
    model: &'a SmithyModel,
    name: &str,
    exclude: &HashSet<&str>,
) -> Option<&'a carina_smithy::ShapeRef> {
    if let Some(schema_struct_name) = res.schema_structure {
        let schema_structure_id = format!("{}#{}", res.service_namespace, schema_struct_name);
        return model
            .get_structure(&schema_structure_id)
            .and_then(|schema_struct| schema_struct.members.get(name))
            .filter(|_| !exclude.contains(name) && !name.eq_ignore_ascii_case("tags"));
    }

    if !res.create_op.is_empty() {
        let create_op_id = format!("{}#{}", res.service_namespace, res.create_op);
        if let Some(member_ref) = model
            .operation_input(&create_op_id)
            .and_then(|create_input| create_input.members.get(name))
            .filter(|_| !exclude.contains(name) && !name.eq_ignore_ascii_case("tags"))
        {
            return Some(member_ref);
        }
    }

    if let Some(read_struct_name) = res.read_structure {
        let read_structure_id = format!("{}#{}", res.service_namespace, read_struct_name);
        return model
            .get_structure(&read_structure_id)
            .and_then(|read_struct| read_struct.members.get(name))
            .filter(|_| !exclude.contains(name) && !name.eq_ignore_ascii_case("tags"));
    }

    None
}

/// Derive a short suffix from an operation name by stripping the verb prefix and identifier.
/// e.g., "GetBucketVersioning" with identifier "Bucket" -> "versioning"
fn op_suffix(operation: &str, identifier: &str) -> String {
    let without_verb = operation
        .strip_prefix("Get")
        .or_else(|| operation.strip_prefix("Put"))
        .or_else(|| operation.strip_prefix("Describe"))
        .unwrap_or(operation);
    let without_id = if !identifier.is_empty() {
        without_verb
            .strip_prefix(identifier)
            .unwrap_or(without_verb)
    } else {
        without_verb
    };
    without_id.to_snake_case()
}

/// Get the SDK crate name from a service namespace.
/// e.g., "com.amazonaws.s3" -> "aws_sdk_s3"
fn sdk_crate_name(service_namespace: &str) -> String {
    let service = service_namespace
        .strip_prefix("com.amazonaws.")
        .unwrap_or(service_namespace);
    format!("aws_sdk_{}", service)
}

/// Get the client field name from a service namespace.
/// e.g., "com.amazonaws.ec2" -> "ec2_client", "com.amazonaws.s3" -> "s3_client"
fn client_field_name(service_namespace: &str) -> String {
    let service = service_namespace
        .strip_prefix("com.amazonaws.")
        .unwrap_or(service_namespace);
    format!("{}_client", service)
}

// ── Markdown documentation generation ──

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum MarkdownEnumSource {
    Smithy { target: String, type_name: String },
    KnownOverride(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MarkdownEnumKey {
    namespace: String,
    heading_path: Vec<String>,
    source: MarkdownEnumSource,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MarkdownStructKey {
    namespace: String,
    heading_path: Vec<String>,
    target: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum MarkdownTarget {
    Enum(MarkdownEnumKey),
    Struct(MarkdownStructKey),
}

#[derive(Debug)]
enum MarkdownType {
    Plain(String),
    Enum {
        key: MarkdownEnumKey,
        type_name: String,
    },
    Struct {
        key: MarkdownStructKey,
        name: String,
    },
    List(Box<MarkdownType>),
}

impl MarkdownType {
    fn contains_link(&self) -> bool {
        match self {
            Self::Enum { .. } | Self::Struct { .. } => true,
            Self::List(item) => item.contains_link(),
            Self::Plain(_) => false,
        }
    }

    fn plain_text(&self) -> String {
        match self {
            Self::Plain(value) => value
                .strip_prefix('`')
                .and_then(|value| value.strip_suffix('`'))
                .unwrap_or(value)
                .to_string(),
            Self::List(item) => format!("List<{}>", item.plain_text()),
            Self::Enum { type_name, .. } => format!("Enum ({type_name})"),
            Self::Struct { name, .. } => format!("Struct({name})"),
        }
    }

    fn write_to(&self, page: &mut PageBuilder<MarkdownTarget>) {
        match self {
            Self::Plain(value) => page.push_str(value),
            Self::Enum { key, type_name } => {
                page.link(
                    MarkdownTarget::Enum(key.clone()),
                    format!("Enum ({type_name})"),
                );
            }
            Self::Struct { key, name } => {
                page.link(
                    MarkdownTarget::Struct(key.clone()),
                    format!("Struct({name})"),
                );
            }
            Self::List(item) if item.contains_link() => {
                page.push_str("List\\<");
                item.write_to(page);
                page.push_str("\\>");
            }
            Self::List(item) => page.push_str(&format!("`List<{}>`", item.plain_text())),
        }
    }
}

#[derive(Debug)]
struct MarkdownEnum {
    prop_name: String,
    info: EnumInfo,
}

#[derive(Debug)]
struct MarkdownStruct {
    fields: Vec<MarkdownStructField>,
}

#[derive(Debug)]
struct MarkdownStructField {
    snake_name: String,
    field_type: MarkdownType,
    is_required: bool,
    description: String,
}

#[derive(Debug)]
struct MarkdownAttrInfo {
    snake_name: String,
    attr_type: MarkdownType,
    is_required: bool,
    description: Option<String>,
}

/// Generate markdown documentation for a single resource.
fn generate_markdown_resource(
    res: &ResourceDef,
    model: &SmithyModel,
    example: Option<&str>,
) -> Result<String> {
    let ns = res.service_namespace;
    let namespace = format!("aws.{}", res.name);

    let is_data_source = res.create_op.is_empty();

    let mut exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();
    for disposition in res.read_dispositions() {
        if let resource_defs::ReadDisposition::OmitSchema { field, .. } = disposition {
            exclude.insert(field);
        }
    }
    let type_overrides: HashMap<&str, &str> = res.type_overrides.iter().copied().collect();
    let struct_field_type_overrides: HashMap<(&str, &str), &str> = res
        .struct_field_type_overrides
        .iter()
        .map(|(p, m, t)| ((*p, *m), *t))
        .collect();
    let required_overrides: HashSet<&str> = res.required_overrides.iter().copied().collect();
    let read_only_overrides: HashSet<&str> = res.read_only_overrides.iter().copied().collect();
    let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();
    let read_shape_overrides: HashSet<&str> = res.read_shape_overrides.iter().copied().collect();
    let enum_alias_map: HashMap<&str, Vec<(&str, &str)>> = {
        let mut m: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (attr, alias, canonical) in &res.enum_aliases {
            m.entry(attr).or_default().push((canonical, alias));
        }
        m
    };

    // Resolve create input (skip for data sources)
    let create_input = if !is_data_source {
        let create_op_id = format!("{}#{}", ns, res.create_op);
        Some(
            model
                .operation_input(&create_op_id)
                .with_context(|| format!("Cannot find create input for {}", create_op_id))?,
        )
    } else {
        None
    };

    // Resolve schema_structure (overrides create input for field discovery)
    let schema_structure = if let Some(schema_struct_name) = res.schema_structure {
        let schema_structure_id = format!("{}#{}", ns, schema_struct_name);
        Some(
            model
                .get_structure(&schema_structure_id)
                .with_context(|| format!("Cannot find schema structure {}", schema_structure_id))?,
        )
    } else {
        None
    };

    // Resolve read structure
    let read_structure = if let Some(read_struct_name) = res.read_structure {
        let read_structure_id = format!("{}#{}", ns, read_struct_name);
        Some(
            model
                .get_structure(&read_structure_id)
                .with_context(|| format!("Cannot find read structure {}", read_structure_id))?,
        )
    } else {
        None
    };

    // Resolve update fields
    let mut updatable_fields: HashSet<String> = HashSet::new();
    for update_op in &res.update_ops {
        for field in update_op.fields.field_names() {
            updatable_fields.insert(field.to_string());
        }
    }

    // Collect writable fields: from schema_structure if set, otherwise from create input
    let mut writable_fields: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    if let Some(schema_struct) = &schema_structure {
        for (name, member_ref) in &schema_struct.members {
            if exclude.contains(name.as_str()) || name.eq_ignore_ascii_case("tags") {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    } else if let Some(create_input) = &create_input {
        for (name, member_ref) in &create_input.members {
            if exclude.contains(name.as_str())
                || name.eq_ignore_ascii_case("tags")
                || read_shape_overrides.contains(name.as_str())
            {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    }

    // Read ops fields
    let mut read_op_read_only: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    for read_op in &res.read_ops {
        let op_id = format!("{}#{}", ns, read_op.operation);
        let output = model
            .operation_output(&op_id)
            .with_context(|| format!("Cannot find output for {}", op_id))?;
        for (field_name, rename) in &read_op.fields {
            let effective_name = rename.unwrap_or(field_name);
            if let Some(member_ref) = output.members.get(*field_name) {
                if updatable_fields.contains(effective_name)
                    && !writable_fields.contains_key(effective_name)
                {
                    writable_fields.insert(effective_name.to_string(), member_ref);
                } else if !writable_fields.contains_key(effective_name) {
                    read_op_read_only.insert(effective_name.to_string(), member_ref);
                }
            }
        }
    }

    // Add updatable-only fields from read structure
    if let Some(read_struct) = read_structure {
        for (name, member_ref) in &read_struct.members {
            if exclude.contains(name.as_str())
                || name.eq_ignore_ascii_case("tags")
                || name == res.identifier
            {
                continue;
            }
            if !writable_fields.contains_key(name) && updatable_fields.contains(name.as_str()) {
                writable_fields.insert(name.clone(), member_ref);
            }
        }
    }

    // Add extra_attributes fields sourced from the read structure
    for extra in &res.extra_attributes {
        if writable_fields.contains_key(extra.name) {
            continue;
        }
        if let Some(source_field) = extra.read_source
            && let Some(read_struct) = read_structure
            && let Some(member_ref) = read_struct.members.get(source_field)
        {
            writable_fields.insert(extra.name.to_string(), member_ref);
        }
    }

    // Read-only fields
    let mut read_only_fields: BTreeMap<String, &carina_smithy::ShapeRef> = BTreeMap::new();
    if let Some(read_struct) = read_structure {
        for (name, member_ref) in &read_struct.members {
            if exclude.contains(name.as_str())
                || name.eq_ignore_ascii_case("tags")
                || writable_fields.contains_key(name)
            {
                continue;
            }
            if name == res.identifier || extra_read_only.contains(name.as_str()) {
                read_only_fields.insert(name.clone(), member_ref);
            }
        }
    }
    for (name, member_ref) in read_op_read_only {
        if !writable_fields.contains_key(&name) && !read_only_fields.contains_key(&name) {
            read_only_fields.insert(name, member_ref);
        }
    }

    // Discover the complete semantic target graph before emission. Struct
    // traversal finds enums nested below top-level attributes; emitted links
    // retain these typed keys until their target headings are actually written.
    let mut all_enums: BTreeMap<MarkdownEnumKey, MarkdownEnum> = BTreeMap::new();
    let mut struct_defs: BTreeMap<MarkdownStructKey, MarkdownStruct> = BTreeMap::new();
    let mut discovering_structs = HashMap::new();

    // Build extra_attributes description override map
    let extra_attribute_descs: HashMap<&str, Option<&str>> = res
        .extra_attributes
        .iter()
        .map(|e| (e.name, e.description))
        .collect();

    let mut writable_attrs: Vec<MarkdownAttrInfo> = Vec::new();
    for (name, member_ref) in &writable_fields {
        let snake_name = name.to_snake_case();
        let is_required = (SmithyModel::is_required(member_ref)
            || required_overrides.contains(name.as_str()))
            && !read_only_overrides.contains(name.as_str());
        let description = if let Some(Some(desc)) = extra_attribute_descs.get(name.as_str()) {
            Some(desc.to_string())
        } else {
            SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string())
        };
        let attr_type = discover_markdown_type(
            model,
            &member_ref.target,
            name,
            std::slice::from_ref(&snake_name),
            &namespace,
            &type_overrides,
            &struct_field_type_overrides,
            None,
            &mut all_enums,
            &mut struct_defs,
            &mut discovering_structs,
        );

        writable_attrs.push(MarkdownAttrInfo {
            snake_name,
            attr_type,
            is_required,
            description,
        });
    }

    let mut read_only_attrs: Vec<MarkdownAttrInfo> = Vec::new();

    // Add synthetic extra_attributes fields (no read_source) to markdown.
    // A field listed in `read_only_overrides` goes into the read-only
    // table; otherwise into the writable table.
    for extra in &res.extra_attributes {
        if extra.read_source.is_some() {
            continue;
        }
        let snake_name = extra.name.to_snake_case();
        let type_display = if let Some(&override_type) = type_overrides.get(extra.name) {
            type_code_to_display(override_type)
        } else if is_aws_resource_id_property(extra.name) {
            resource_id_display(extra.name)
        } else {
            "String".to_string()
        };
        let entry = MarkdownAttrInfo {
            snake_name,
            attr_type: MarkdownType::Plain(type_display),
            is_required: required_overrides.contains(extra.name)
                && !read_only_overrides.contains(extra.name),
            description: extra.description.map(|s| s.to_string()),
        };
        if read_only_overrides.contains(extra.name) {
            read_only_attrs.push(entry);
        } else {
            writable_attrs.push(entry);
        }
    }

    for (name, member_ref) in &read_only_fields {
        let snake_name = name.to_snake_case();
        let description = SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string());
        let attr_type = if name == "Arn" {
            resource_identity_parts(res.name)
                .and_then(|(service, resource)| shared_arn_constructor(service, resource))
                .map(|constructor| type_code_to_display(&format!("super::{constructor}()")))
                .map(MarkdownType::Plain)
        } else {
            None
        }
        .unwrap_or_else(|| {
            discover_markdown_type(
                model,
                &member_ref.target,
                name,
                std::slice::from_ref(&snake_name),
                &namespace,
                &type_overrides,
                &struct_field_type_overrides,
                None,
                &mut all_enums,
                &mut struct_defs,
                &mut discovering_structs,
            )
        });

        read_only_attrs.push(MarkdownAttrInfo {
            snake_name,
            attr_type,
            is_required: false,
            description,
        });
    }

    let markdown_root_targets: HashMap<String, String> = writable_fields
        .iter()
        .chain(&read_only_fields)
        .map(|(name, member_ref)| (name.to_snake_case(), member_ref.target.clone()))
        .collect();
    validate_markdown_heading_paths(
        model,
        &markdown_root_targets,
        all_enums
            .keys()
            .map(|key| key.heading_path.as_slice())
            .chain(struct_defs.keys().map(|key| key.heading_path.as_slice())),
    )
    .with_context(|| format!("validating Markdown heading paths for aws.{}", res.name))?;

    let enum_heading_texts = markdown_enum_heading_texts(&all_enums);
    let struct_heading_texts = markdown_struct_heading_texts(&struct_defs);
    let mut page = PageBuilder::new();

    page.heading(Heading::plain(1, format!("aws.{}", res.name)))?;

    // Description
    let desc_traits = if let Some(read_struct) = read_structure {
        Some(&read_struct.traits)
    } else {
        create_input.as_ref().map(|ci| &ci.traits)
    };
    if let Some(traits) = desc_traits
        && let Some(desc) = SmithyModel::documentation(traits)
    {
        let cleaned = collapse_whitespace(&strip_html_tags(desc).replace(['\n', '\t'], " "));
        page.push_str(&format!("{}\n\n", cleaned.trim()));
    }

    write_markdown_example(&mut page, example)?;

    // Argument Reference (skip for data sources)
    if !is_data_source {
        page.heading(Heading::plain(2, "Argument Reference"))?;
    }

    for attr in &writable_attrs {
        page.heading(Heading::code(3, &attr.snake_name))?;
        page.push_str("- **Type:** ");
        attr.attr_type.write_to(&mut page);
        page.push_str("\n");
        page.push_str(&format!(
            "- **Required:** {}\n",
            if attr.is_required { "Yes" } else { "No" }
        ));
        page.push_str("\n");

        if let Some(ref desc) = attr.description {
            let cleaned = collapse_whitespace(&strip_html_tags(desc).replace(['\n', '\t'], " "));
            page.push_str(&format!("{}\n\n", cleaned.trim()));
        }
    }

    // Tags
    if res.has_tags {
        page.heading(Heading::code(3, "tags"))?;
        page.push_str("- **Type:** Map\n");
        page.push_str("- **Required:** No\n\n");
        page.push_str("The tags for the resource.\n\n");
    }

    // Enum Values section
    //
    // Per naming-conventions design D7, the documented "DSL Identifier" and
    // "Shorthand formats" use the snake_case spelling. Explicit aliases in
    // `ResourceDef::enum_aliases` win over the auto-derived form so that
    // existing documented overrides (e.g. `IpProtocol.all` for `-1`) keep
    // their intentional spelling.
    if !all_enums.is_empty() {
        page.heading(Heading::plain(2, "Enum Values"))?;
        for (key, heading_text) in &enum_heading_texts {
            let enum_doc = all_enums
                .get(key)
                .expect("every enum heading must refer to a discovered enum");
            let enum_info = &enum_doc.info;
            let attr_name = enum_doc.prop_name.to_snake_case();
            let prop_aliases = enum_alias_map.get(attr_name.as_str());

            let dsl_for = |value: &str| -> String {
                if let Some(alias_list) = prop_aliases
                    && let Some((_, alias)) = alias_list.iter().find(|(c, _)| *c == value)
                {
                    return alias.to_string();
                }
                dsl_enum_value(value)
            };

            page.target_heading(
                MarkdownTarget::Enum(key.clone()),
                Heading::plain(3, heading_text),
            )?;
            page.push_str("| Value | DSL Identifier |\n");
            page.push_str("|-------|----------------|\n");

            for value in &enum_info.values {
                let dsl_value = dsl_for(value);
                let dsl_id = format!("{}.{}.{}", key.namespace, enum_info.type_name, dsl_value);
                page.push_str(&format!("| `{}` | `{}` |\n", value, dsl_id));
            }
            page.push_str("\n");

            let first_value = enum_info.values.first().map(|s| s.as_str()).unwrap_or("");
            let first_dsl = dsl_for(first_value);
            page.push_str(&format!(
                "Shorthand formats: `{}` or `{}.{}`\n\n",
                first_dsl, enum_info.type_name, first_dsl,
            ));
        }
    }

    // Struct Definitions section
    if !struct_defs.is_empty() {
        page.heading(Heading::plain(2, "Struct Definitions"))?;
        for (key, heading_text) in &struct_heading_texts {
            let struct_doc = struct_defs
                .get(key)
                .expect("every struct heading must refer to a discovered struct");
            page.target_heading(
                MarkdownTarget::Struct(key.clone()),
                Heading::plain(3, heading_text),
            )?;
            page.push_str("| Field | Type | Required | Description |\n");
            page.push_str("|-------|------|----------|-------------|\n");
            for field in &struct_doc.fields {
                page.push_str(&format!("| `{}` | ", field.snake_name));
                field.field_type.write_to(&mut page);
                page.push_str(&format!(
                    " | {} | {} |\n",
                    if field.is_required { "Yes" } else { "No" },
                    field.description
                ));
            }
            page.push_str("\n");
        }
    }

    // Attribute Reference (read-only)
    if !read_only_attrs.is_empty() {
        page.heading(Heading::plain(2, "Attribute Reference"))?;
        for attr in &read_only_attrs {
            page.heading(Heading::code(3, &attr.snake_name))?;
            page.push_str("- **Type:** ");
            attr.attr_type.write_to(&mut page);
            page.push_str("\n\n");
        }
    }

    page.finish()
        .with_context(|| format!("generated invalid Markdown for aws.{}", res.name))
}

fn write_markdown_example(
    page: &mut PageBuilder<MarkdownTarget>,
    example: Option<&str>,
) -> Result<()> {
    let Some(example) = example else {
        return Ok(());
    };

    page.heading(Heading::plain(2, "Example"))?;
    page.push_str("```crn\n");
    page.push_str(example);
    if !example.ends_with('\n') {
        page.push_str("\n");
    }
    page.push_str("```\n\n");
    Ok(())
}

fn preprocess_markdown_example(source: &str) -> String {
    let mut filtered = Vec::new();
    let mut in_provider_block = false;
    for line in source.split_inclusive('\n') {
        if !in_provider_block && line.starts_with("provider ") {
            in_provider_block = true;
            continue;
        }
        if in_provider_block {
            if line.starts_with('}') {
                in_provider_block = false;
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        filtered.push(line);
    }

    let first_content = filtered
        .iter()
        .position(|line| !line.strip_suffix('\n').unwrap_or(line).is_empty())
        .unwrap_or(filtered.len());
    filtered[first_content..].concat()
}

fn markdown_enum_heading_texts(
    enums: &BTreeMap<MarkdownEnumKey, MarkdownEnum>,
) -> Vec<(MarkdownEnumKey, String)> {
    let candidates: Vec<_> = enums
        .iter()
        .map(|(key, enum_doc)| {
            let base = format!(
                "{} ({})",
                key.heading_path.join("."),
                enum_doc.info.type_name
            );
            let identity = match &key.source {
                MarkdownEnumSource::Smithy { target, type_name } => {
                    format!("{target} {type_name}")
                }
                MarkdownEnumSource::KnownOverride(field) => format!("known override {field}"),
            };
            (key.clone(), base, identity)
        })
        .collect();
    let mut base_counts = HashMap::new();
    for (_, heading, _) in &candidates {
        *base_counts
            .entry(carina_docgen::heading_slug(heading))
            .or_insert(0_usize) += 1;
    }
    let mut used_slugs = HashSet::new();
    let mut headings: Vec<_> = candidates
        .into_iter()
        .map(|(key, base, identity)| {
            let primary = if base_counts[&carina_docgen::heading_slug(&base)] > 1 {
                format!("{base} — {identity}")
            } else {
                base
            };
            let heading = claim_unique_heading(primary, &identity, &mut used_slugs);
            (key, heading)
        })
        .collect();
    sort_markdown_headings(&mut headings);
    headings
}

fn markdown_struct_heading_texts(
    structs: &BTreeMap<MarkdownStructKey, MarkdownStruct>,
) -> Vec<(MarkdownStructKey, String)> {
    let mut used_slugs = HashSet::new();
    let mut headings: Vec<_> = structs
        .keys()
        .map(|key| {
            let base = key.heading_path.join(".");
            let source = format!("{} {}", key.namespace, key.target);
            let heading = claim_unique_heading(base, &source, &mut used_slugs);
            (key.clone(), heading)
        })
        .collect();
    sort_markdown_headings(&mut headings);
    headings
}

fn sort_markdown_headings<K: Ord>(headings: &mut [(K, String)]) {
    headings.sort_by(|(left_key, left_heading), (right_key, right_heading)| {
        left_heading
            .cmp(right_heading)
            .then_with(|| left_key.cmp(right_key))
    });
}

fn claim_unique_heading(
    primary: String,
    stable_identity: &str,
    used_slugs: &mut HashSet<String>,
) -> String {
    let candidates = [primary.clone(), format!("{primary} — {stable_identity}")];

    for candidate in candidates {
        if used_slugs.insert(carina_docgen::heading_slug(&candidate)) {
            return candidate;
        }
    }

    for suffix in 2.. {
        let candidate = format!("{primary} {suffix}");
        if used_slugs.insert(carina_docgen::heading_slug(&candidate)) {
            return candidate;
        }
    }
    unreachable!("an integer suffix always yields a fresh heading")
}

/// Generate Rust schema code for a data source.
///
/// `dual_registered` should be true when the same DSL name (e.g. `s3.Bucket`)
/// is also registered as a Managed resource. In that case the file name and
/// config function are suffixed with `_data_source` to avoid colliding with
/// the Managed file (`s3/bucket.rs` + `s3_bucket_config`).
fn generate_data_source(
    ds: &resource_defs::DataSourceDef,
    _model: &SmithyModel,
    dual_registered: bool,
) -> Result<String> {
    let ns = ds.service_namespace;
    let suffix = if dual_registered { "_data_source" } else { "" };
    let config_fn = format!("{}{}_config", module_name(ds.name), suffix);
    let cf_type = cf_type_name(ds.name);

    // Build all attribute type strings up-front so we can compute imports.
    struct DsAttr {
        name: String,
        provider_name: String,
        type_str: String,
        description: String,
        required: bool,
        read_only: bool,
    }
    let mut ds_attrs: Vec<DsAttr> = Vec::new();
    for input in &ds.inputs {
        let type_str = if let Some(t) = input.type_override {
            t.to_string()
        } else if is_email_property(input.provider_name) {
            "types::email()".to_string()
        } else {
            PLAIN_STRING_TYPE_CODE.to_string()
        };
        ds_attrs.push(DsAttr {
            name: input.name.to_string(),
            provider_name: input.provider_name.to_string(),
            type_str,
            description: input.description.to_string(),
            required: input.required,
            read_only: false,
        });
    }
    let input_names: HashSet<&str> = ds.inputs.iter().map(|i| i.name).collect();
    match &ds.shape {
        resource_defs::DataSourceShape::Single { output_attributes } => {
            for output in output_attributes {
                // Skip outputs that echo an input — the input row already covers
                // both directions (e.g. `s3.Bucket.bucket`: user supplies it as
                // lookup input, runtime echoes it back as the same attribute).
                if input_names.contains(output.name) {
                    continue;
                }
                ds_attrs.push(DsAttr {
                    name: output.name.to_string(),
                    provider_name: output.provider_name.unwrap_or("").to_string(),
                    type_str: output.type_code.to_string(),
                    description: output.description.to_string(),
                    required: false,
                    read_only: true,
                });
            }
        }
        resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => {
            for output in aggregated_outputs {
                ds_attrs.push(DsAttr {
                    name: output.name.to_string(),
                    provider_name: String::new(),
                    type_str: format!("AttributeType::unordered_list({})", output.item_type),
                    description: output.description.to_string(),
                    required: false,
                    read_only: true,
                });
            }
        }
    }

    // Determine needed imports based on actual type strings used.
    let needs_types = ds_attrs.iter().any(|a| a.type_str.contains("types::"));
    let needs_attribute_type = ds_attrs
        .iter()
        .any(|a| a.type_str.contains("AttributeType::"));
    let mut schema_imports = vec!["AttributeSchema", "ResourceSchema"];
    if needs_attribute_type {
        schema_imports.insert(1, "AttributeType");
    }
    if needs_types {
        schema_imports.push("types");
    }
    let arn_identity = resource_identity_parts(ds.name)
        .filter(|_| ds_attrs.iter().any(|a| a.name == "arn"))
        .map(|(service, resource)| (service, resource, arn_emit_choice(service, resource)));
    let arn_helper =
        arn_identity.map(|(service, resource, choice)| emit_arn_helper(service, resource, choice));
    let has_arn_helper = arn_helper.is_some();
    if has_arn_helper && !schema_imports.contains(&"AttributeType") {
        schema_imports.insert(1, "AttributeType");
    }
    let schema_imports_str = schema_imports.join(", ");
    let arn_helper_code = arn_helper.unwrap_or_default();

    let mut code = String::new();
    code.push_str(&format!(
        "//! {} schema definition for AWS Cloud Control\n\
         //!\n\
         //! Auto-generated from Smithy model: {}\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with smithy-codegen\n\n\
         use super::AwsSchemaConfig;\n\
         use carina_core::schema::{{{}}};\n\n\
         {arn_helper_code}\
         /// Returns the schema config for {} (Smithy: {})\n\
         pub fn {}() -> AwsSchemaConfig {{\n\
         \x20   AwsSchemaConfig {{\n\
         \x20       aws_type_name: \"{}\",\n\
         \x20       resource_type_name: \"{}\",\n\
         \x20       has_tags: false,\n\
         \x20       schema: ResourceSchema::new(\"{}\")\n\
         \x20       .as_data_source()\n",
        ds.name.split('.').next_back().unwrap_or(ds.name),
        ns,
        schema_imports_str,
        ds.name,
        ns,
        config_fn,
        cf_type,
        ds.name,
        ds.name,
    ));

    // Emit attributes.
    for attr in &ds_attrs {
        let type_str = if has_arn_helper && attr.name == "arn" {
            "self::arn()"
        } else {
            attr.type_str.as_str()
        };
        let constructor = mode_constructor(attr.required, attr.read_only);
        code.push_str(&format!(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::{}(\"{}\", {})\n\
             \x20               .with_description(\"{}\")\n\
             \x20               .with_provider_name(\"{}\")\n\
             \x20               .build(),\n\
             \x20       )\n",
            constructor, attr.name, type_str, attr.description, attr.provider_name,
        ));
    }

    code.push_str("    }\n}\n\n");

    // Enum values stub
    code.push_str(&format!(
        "/// Returns the resource type name and all enum valid values for this module\n\
         pub fn enum_valid_values() -> (&'static str, &'static [(&'static str, &'static [&'static str])]) {{\n\
         \x20   (\"{}\", &[])\n\
         }}\n",
        ds.name,
    ));

    Ok(code)
}

/// Generate markdown documentation for a data source.
fn generate_markdown_data_source(
    ds: &resource_defs::DataSourceDef,
    _model: &SmithyModel,
    example: Option<&str>,
) -> Result<String> {
    struct MarkdownDataSourceOutput<'a> {
        name: &'a str,
        type_display: String,
        description: &'a str,
    }

    let input_names: HashSet<&str> = ds.inputs.iter().map(|input| input.name).collect();
    let outputs: Vec<MarkdownDataSourceOutput<'_>> = match &ds.shape {
        resource_defs::DataSourceShape::Single { output_attributes } => output_attributes
            .iter()
            .filter(|output| !input_names.contains(output.name))
            .map(|output| MarkdownDataSourceOutput {
                name: output.name,
                type_display: type_code_to_display(output.type_code),
                description: output.description,
            })
            .collect(),
        resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => aggregated_outputs
            .iter()
            .map(|output| MarkdownDataSourceOutput {
                name: output.name,
                type_display: format!("List({})", type_code_to_display(output.item_type)),
                description: output.description,
            })
            .collect(),
    };

    let mut page: PageBuilder<MarkdownTarget> = PageBuilder::new();
    page.heading(Heading::plain(1, format!("aws.{}", ds.name)))?;
    page.push_str("This is a **data source** (read-only). Use with the `read` keyword.\n\n");
    write_markdown_example(&mut page, example)?;

    // Lookup inputs section
    if !ds.inputs.is_empty() {
        page.heading(Heading::plain(2, "Lookup Inputs"))?;
        for input in &ds.inputs {
            page.heading(Heading::code(3, input.name))?;
            let required = if input.required { "Yes" } else { "No" };
            page.push_str(&format!("- **Required:** {}\n\n", required));
            if !input.description.is_empty() {
                page.push_str(&format!("{}\n\n", input.description));
            }
        }
    }

    // Output attributes section
    page.heading(Heading::plain(2, "Attributes"))?;
    for output in outputs {
        page.heading(Heading::code(3, output.name))?;
        page.push_str(&format!("- **Type:** {}\n", output.type_display));
        page.push_str("- **Read-only**\n\n");
        if !output.description.is_empty() {
            page.push_str(&format!("{}\n\n", output.description));
        }
    }

    page.finish()
        .with_context(|| format!("generated invalid Markdown for aws.{}", ds.name))
}

fn validate_markdown_heading_paths<'a>(
    model: &SmithyModel,
    root_targets: &HashMap<String, String>,
    heading_paths: impl IntoIterator<Item = &'a [String]>,
) -> Result<()> {
    for path in heading_paths {
        let Some((root, nested)) = path.split_first() else {
            anyhow::bail!("Markdown heading path must not be empty");
        };
        let mut target = root_targets
            .get(root)
            .with_context(|| {
                format!(
                    "Markdown heading path `{}` has no root field",
                    path.join(".")
                )
            })?
            .clone();
        let mut parent_path = root.clone();

        for field in nested {
            target = markdown_nested_field_target(model, &target, field).with_context(|| {
                format!(
                    "Markdown heading path `{}` has no field `{field}` below `{parent_path}`",
                    path.join(".")
                )
            })?;
            parent_path.push('.');
            parent_path.push_str(field);
        }
    }
    Ok(())
}

fn markdown_nested_field_target(
    model: &SmithyModel,
    target: &str,
    field_name: &str,
) -> Result<String> {
    let mut target = target.to_string();
    loop {
        match model.get_shape(&target) {
            Some(carina_smithy::Shape::List(list)) => {
                target.clone_from(&list.member.target);
            }
            Some(carina_smithy::Shape::Structure(structure)) => {
                return structure
                    .members
                    .iter()
                    .find(|(name, _)| name.to_snake_case() == field_name)
                    .map(|(_, member)| member.target.clone())
                    .with_context(|| format!("Smithy structure `{target}` has no such member"));
            }
            Some(_) => anyhow::bail!("Smithy shape `{target}` is not a structure"),
            None => anyhow::bail!("Smithy shape `{target}` does not exist"),
        }
    }
}

/// Discover the semantic representation of a type used in markdown docs.
///
/// `parent_struct` is `Some(struct_name)` when this is called for a
/// member of a struct-fields table (so per-`(parent, member)` overrides
/// can fire), and `None` for top-level attributes and list-element
/// recursion where no parent struct is in scope.
#[allow(clippy::too_many_arguments)]
fn discover_markdown_type(
    model: &SmithyModel,
    target: &str,
    field_name: &str,
    heading_path: &[String],
    enum_namespace: &str,
    type_overrides: &HashMap<&str, &str>,
    struct_field_type_overrides: &HashMap<(&str, &str), &str>,
    parent_struct: Option<&str>,
    all_enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    struct_defs: &mut BTreeMap<MarkdownStructKey, MarkdownStruct>,
    discovering_structs: &mut HashMap<String, MarkdownStructKey>,
) -> MarkdownType {
    // Per-(parent_struct, member) override wins over per-member and Smithy
    // default. Distinguishes a member name shared across structs (aws#302).
    if let Some(parent) = parent_struct
        && let Some(&override_type) = struct_field_type_overrides.get(&(parent, field_name))
    {
        return MarkdownType::Plain(type_code_to_display(override_type));
    }
    // Check type overrides
    if let Some(&override_type) = type_overrides.get(field_name) {
        return MarkdownType::Plain(type_code_to_display(override_type));
    }

    // Check known enum overrides
    if let Some(values) = known_enum_overrides().get(field_name) {
        let type_name = field_name.to_string();
        let key = register_markdown_enum(
            MarkdownEnumSource::KnownOverride(field_name.to_string()),
            field_name,
            &type_name,
            heading_path,
            enum_namespace,
            values.iter().map(|value| value.to_string()).collect(),
            all_enums,
        );
        return MarkdownType::Enum { key, type_name };
    }

    let kind = model.shape_kind(target);

    match kind {
        Some(ShapeKind::String) => {
            if let Some(inferred) = infer_string_type(field_name) {
                return MarkdownType::Plain(type_code_to_display(&inferred));
            }
            MarkdownType::Plain("String".to_string())
        }
        Some(ShapeKind::Boolean) => MarkdownType::Plain("Bool".to_string()),
        Some(ShapeKind::Integer) | Some(ShapeKind::Long) => {
            let range = get_int_range(model, target, field_name);
            if let Some(r) = range {
                MarkdownType::Plain(format!("Int({})", range_display_string(r.min, r.max)))
            } else {
                MarkdownType::Plain("Int".to_string())
            }
        }
        Some(ShapeKind::Float) | Some(ShapeKind::Double) => {
            MarkdownType::Plain("Float".to_string())
        }
        Some(ShapeKind::Enum) => {
            if let Some(values) = model.enum_values(target) {
                // Prefer the Smithy shape name (PascalCase, e.g. "LogGroupClass")
                // over the field name (often camelCase, e.g. "logGroupClass") so
                // the rendered enum heading reads naturally.
                let type_name =
                    pascalize_enum_type_name(SmithyModel::shape_name(target), field_name);
                let key = register_markdown_enum(
                    MarkdownEnumSource::Smithy {
                        target: target.to_string(),
                        type_name: type_name.clone(),
                    },
                    field_name,
                    &type_name,
                    heading_path,
                    enum_namespace,
                    values.into_iter().map(|(_, value)| value).collect(),
                    all_enums,
                );
                MarkdownType::Enum { key, type_name }
            } else {
                MarkdownType::Plain("String".to_string())
            }
        }
        Some(ShapeKind::IntEnum) => MarkdownType::Plain("Int".to_string()),
        Some(ShapeKind::List) => {
            if let Some(carina_smithy::Shape::List(list_shape)) = model.get_shape(target) {
                let item = discover_markdown_type(
                    model,
                    &list_shape.member.target,
                    field_name,
                    heading_path,
                    enum_namespace,
                    type_overrides,
                    struct_field_type_overrides,
                    parent_struct,
                    all_enums,
                    struct_defs,
                    discovering_structs,
                );
                MarkdownType::List(Box::new(item))
            } else {
                MarkdownType::List(Box::new(MarkdownType::Plain("String".to_string())))
            }
        }
        Some(ShapeKind::Map) => MarkdownType::Plain("Map".to_string()),
        Some(ShapeKind::Structure) => {
            let shape_name = SmithyModel::shape_name(target).to_string();
            if shape_name == "TagList" || shape_name == "Tag" {
                return MarkdownType::Plain("Map".to_string());
            }
            if shape_name == "AttributeBooleanValue" {
                return MarkdownType::Plain("Bool".to_string());
            }
            let Some(structure) = model.get_structure(target) else {
                return MarkdownType::Plain("String".to_string());
            };
            if let Some(active_key) = discovering_structs.get(target) {
                return MarkdownType::Struct {
                    key: active_key.clone(),
                    name: shape_name,
                };
            }

            let child_enum_namespace = format!("{enum_namespace}.{shape_name}");
            let key = MarkdownStructKey {
                namespace: child_enum_namespace.clone(),
                heading_path: heading_path.to_vec(),
                target: target.to_string(),
            };
            if !struct_defs.contains_key(&key) {
                discovering_structs.insert(target.to_string(), key.clone());
                let fields = structure
                    .members
                    .iter()
                    .map(|(name, member_ref)| {
                        let mut child_heading_path = heading_path.to_vec();
                        child_heading_path.push(name.to_snake_case());
                        let field_type = discover_markdown_type(
                            model,
                            &member_ref.target,
                            name,
                            &child_heading_path,
                            &child_enum_namespace,
                            type_overrides,
                            struct_field_type_overrides,
                            Some(&shape_name),
                            all_enums,
                            struct_defs,
                            discovering_structs,
                        );
                        let description = SmithyModel::documentation(&member_ref.traits)
                            .map(|description| {
                                let cleaned = collapse_whitespace(
                                    &strip_html_tags(description).replace(['\n', '\t'], " "),
                                );
                                cleaned.trim().replace('|', "\\|")
                            })
                            .unwrap_or_default();
                        MarkdownStructField {
                            snake_name: name.to_snake_case(),
                            field_type,
                            is_required: SmithyModel::is_required(member_ref),
                            description,
                        }
                    })
                    .collect();
                discovering_structs.remove(target);
                struct_defs.insert(key.clone(), MarkdownStruct { fields });
            }
            MarkdownType::Struct {
                key,
                name: shape_name,
            }
        }
        _ => {
            if let Some(inferred) = infer_string_type(field_name) {
                MarkdownType::Plain(type_code_to_display(&inferred))
            } else {
                MarkdownType::Plain("String".to_string())
            }
        }
    }
}

fn register_markdown_enum(
    source: MarkdownEnumSource,
    prop_name: &str,
    type_name: &str,
    heading_path: &[String],
    namespace: &str,
    values: Vec<String>,
    all_enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
) -> MarkdownEnumKey {
    let key = MarkdownEnumKey {
        namespace: namespace.to_string(),
        heading_path: heading_path.to_vec(),
        source,
    };
    all_enums
        .entry(key.clone())
        .or_insert_with(|| MarkdownEnum {
            prop_name: prop_name.to_string(),
            info: EnumInfo {
                type_name: type_name.to_string(),
                values,
            },
        });
    key
}

// Split union members without treating commas inside constructor arguments or
// nested collection expressions as member separators.
fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;

    for (index, character) in value.char_indices() {
        match character {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                parts.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&value[start..]);
    parts
}

/// Convert a Rust type code string to a human-readable display name.
///
/// Display names use PascalCase consistently (e.g. `IamRoleArn`, not
/// `iam_role_arn`). Container types like `AttributeType::list(...)` are
/// rendered as `List<Inner>` so docs never leak Rust constructor syntax.
fn type_code_to_display(type_code: &str) -> String {
    // Container types: AttributeType::list(...) and AttributeType::map(...)
    if let Some(inner) = type_code
        .strip_prefix("AttributeType::union(vec![")
        .and_then(|s| s.strip_suffix("])"))
    {
        return split_top_level_commas(inner)
            .into_iter()
            .map(|member| type_code_to_display(member.trim()))
            .collect::<Vec<_>>()
            .join(" \\| ");
    }
    if let Some(inner) = type_code
        .strip_prefix("AttributeType::list(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return format!("`List<{}>`", type_code_to_display(inner));
    }
    if let Some(inner) = type_code
        .strip_prefix("AttributeType::map(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return format!("`Map<String, {}>`", type_code_to_display(inner));
    }

    match type_code {
        PLAIN_STRING_TYPE_CODE => "String".to_string(),
        "AttributeType::bool()" => "Bool".to_string(),
        "AttributeType::int()" => "Int".to_string(),
        "AttributeType::float()" => "Float".to_string(),
        "AttributeType::duration()" => "Duration".to_string(),
        s if s.contains("ipv4_cidr") => "Ipv4Cidr".to_string(),
        s if s.contains("ipv6_cidr") => "Ipv6Cidr".to_string(),
        s if s.contains("ipv4_address") => "Ipv4Address".to_string(),
        s if s.contains("ipv6_address") => "Ipv6Address".to_string(),
        s if s.contains("iam_role_arn") => "IamRoleArn".to_string(),
        s if s.contains("iam_role_id") => "IamRoleId".to_string(),
        s if s.contains("iam_policy_arn") => "IamPolicyArn".to_string(),
        s if s.contains("iam_oidc_provider_arn") => "IamOidcProviderArn".to_string(),
        s if s.contains("iam_policy_document") => "PolicyDocument".to_string(),
        s if s.contains("cloudfront_hosted_zone_id") => "CloudFrontHostedZoneId".to_string(),
        s if s.contains("route53_hosted_zone_id") => "Route53HostedZoneId".to_string(),
        s if s.contains("kms_key_arn") || s.contains("kms::key::arn") => "KmsKeyArn".to_string(),
        s if s.contains("kms_key_id") || s.contains("kms::key::id") => "KmsKeyId".to_string(),
        s if s.contains("vpc_id") => "VpcId".to_string(),
        s if s.contains("subnet_id") => "SubnetId".to_string(),
        s if s.contains("security_group_rule_id") => "SecurityGroupRuleId".to_string(),
        s if s.contains("security_group_id") => "SecurityGroupId".to_string(),
        s if s.contains("ipam_pool_id") => "IpamPoolId".to_string(),
        s if s.contains("instance_id") => "InstanceId".to_string(),
        s if s.contains("internet_gateway_id") => "InternetGatewayId".to_string(),
        s if s.contains("nat_gateway_id") => "NatGatewayId".to_string(),
        s if s.contains("route_table_id") => "RouteTableId".to_string(),
        s if s.contains("network_interface_id") => "NetworkInterfaceId".to_string(),
        s if s.contains("allocation_id") => "AllocationId".to_string(),
        s if s.contains("prefix_list_id") => "PrefixListId".to_string(),
        s if s.contains("carrier_gateway_id") => "CarrierGatewayId".to_string(),
        s if s.contains("local_gateway_id") => "LocalGatewayId".to_string(),
        s if s.contains("network_acl_id") => "NetworkAclId".to_string(),
        s if s.contains("s3_grantee") => "S3Grantee".to_string(),
        "super::gateway_id()" => "GatewayId".to_string(),
        s if s.contains("arn()") => "Arn".to_string(),
        s if s.contains("aws_account_id") => "AwsAccountId".to_string(),
        s if s.contains("aws_resource_id") => "AwsResourceId".to_string(),
        s if s.contains("availability_zone_id") => "AvailabilityZoneId".to_string(),
        s if s.contains("availability_zone") => "AvailabilityZone".to_string(),
        s if s.contains("email") => "Email".to_string(),
        _ => snake_to_pascal_type(
            type_code
                .trim_start_matches("super::")
                .trim_start_matches("types::")
                .trim_end_matches("()"),
        ),
    }
}

/// Pick a PascalCase enum type name for use in markdown headings and the
/// `name:` field of `AttributeType::StringEnum`.
///
/// Strategy:
/// 1. If the field name is already PascalCase (starts uppercase), use it
///    unchanged. This preserves established conventions where the Smithy
///    field name is the natural type label (e.g. `InstanceTenancy`).
/// 2. Otherwise (camelCase field name like `logGroupClass`), uppercase its
///    first letter to produce `LogGroupClass`.
///
/// `shape_name` is accepted for forward compatibility (e.g. if we want to
/// fall back to the Smithy shape) but is not currently consulted, since the
/// shape name is sometimes a generic alias (`Tenancy` for `InstanceTenancy`)
/// that doesn't match user expectations.
fn pascalize_enum_type_name(_shape_name: &str, field_name: &str) -> String {
    // Initialism (all ASCII uppercase, e.g. "ACL", "MFA", "VPC") → first
    // letter kept, rest lowercased so the DSL TypeName segment follows
    // Rust's RFC 430 acronym convention. Mixed-case names (e.g.
    // "VersioningStatus") pass through unchanged. Names starting with a
    // lowercase letter get their first character uppercased.
    if !field_name.is_empty() && field_name.chars().all(|c| c.is_ascii_uppercase()) {
        let mut chars = field_name.chars();
        let first = chars.next().unwrap();
        return format!("{}{}", first, chars.as_str().to_ascii_lowercase());
    }
    let mut chars = field_name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => field_name.to_string(),
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// Convert a snake_case identifier to PascalCase for display in docs.
/// Already-PascalCase or single-word inputs are returned unchanged.
fn snake_to_pascal_type(s: &str) -> String {
    if !s.contains('_') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for part in s.split('_') {
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.push(c.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Get the human-readable display name for a resource ID type.
fn resource_id_display(prop_name: &str) -> String {
    match classify_resource_id(prop_name) {
        ResourceIdKind::VpcId => "VpcId".to_string(),
        ResourceIdKind::SubnetId => "SubnetId".to_string(),
        ResourceIdKind::SecurityGroupId => "SecurityGroupId".to_string(),
        ResourceIdKind::EgressOnlyInternetGatewayId => "EgressOnlyInternetGatewayId".to_string(),
        ResourceIdKind::InternetGatewayId => "InternetGatewayId".to_string(),
        ResourceIdKind::RouteTableId => "RouteTableId".to_string(),
        ResourceIdKind::NatGatewayId => "NatGatewayId".to_string(),
        ResourceIdKind::VpcPeeringConnectionId => "VpcPeeringConnectionId".to_string(),
        ResourceIdKind::TransitGatewayId => "TransitGatewayId".to_string(),
        ResourceIdKind::VpnGatewayId => "VpnGatewayId".to_string(),
        ResourceIdKind::VpcEndpointId => "VpcEndpointId".to_string(),
        ResourceIdKind::InstanceId => "InstanceId".to_string(),
        ResourceIdKind::NetworkInterfaceId => "NetworkInterfaceId".to_string(),
        ResourceIdKind::AllocationId => "AllocationId".to_string(),
        ResourceIdKind::PrefixListId => "PrefixListId".to_string(),
        ResourceIdKind::CarrierGatewayId => "CarrierGatewayId".to_string(),
        ResourceIdKind::LocalGatewayId => "LocalGatewayId".to_string(),
        ResourceIdKind::NetworkAclId => "NetworkAclId".to_string(),
        ResourceIdKind::Generic => "AwsResourceId".to_string(),
    }
}

// ── Type inference helpers (ported from codegen.rs) ──

fn known_string_type_overrides() -> &'static HashMap<&'static str, &'static str> {
    static OVERRIDES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("DefaultSecurityGroup", "super::security_group_id()");
        m.insert("DefaultNetworkAcl", "super::network_acl_id()");
        m.insert("AccountId", "super::aws_account_id()");
        m.insert("GatewayId", "super::gateway_id()");
        m.insert("DeliverCrossAccountRole", "super::super::iam::role::arn()");
        m.insert("DeliverLogsPermissionArn", "super::super::iam::role::arn()");
        m.insert("PeerRoleArn", "super::super::iam::role::arn()");
        m.insert("PermissionsBoundary", "super::super::iam::policy::arn()");
        m.insert("ManagedPolicyArns", "super::super::iam::policy::arn()");
        m.insert("KmsKeyId", "super::super::kms::key::id()");
        m.insert("kmsKeyId", "super::super::kms::key::id()");
        m.insert("KMSMasterKeyID", "super::super::kms::key::id()");
        m.insert("ReplicaKmsKeyID", "super::super::kms::key::id()");
        m.insert("KmsKeyArn", "super::super::kms::key::arn()");
        m.insert("SecurityGroupRuleId", "super::security_group_rule_id()");
        m.insert("Locale", "super::aws_region()");
        m.insert("BucketAccountId", "super::aws_account_id()");
        m.insert("PublicIp", "types::ipv4_address()");
        m.insert("LogDestination", "super::arn()");
        m.insert("GrantFullControl", "super::s3_grantee()");
        m.insert("GrantRead", "super::s3_grantee()");
        m.insert("GrantReadACP", "super::s3_grantee()");
        m.insert("GrantWrite", "super::s3_grantee()");
        m.insert("GrantWriteACP", "super::s3_grantee()");
        m
    });
    &OVERRIDES
}

fn known_enum_overrides() -> &'static HashMap<&'static str, Vec<&'static str>> {
    static OVERRIDES: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("IpProtocol", vec!["tcp", "udp", "icmp", "icmpv6", "-1"]);
        m.insert("HostnameType", vec!["ip-name", "resource-name"]);
        m
    });
    &OVERRIDES
}

/// Generate condition string and display string for integer range validation.
#[allow(dead_code)]
fn int_range_condition_and_display(min: Option<i64>, max: Option<i64>) -> (String, String) {
    match (min, max) {
        (Some(min), Some(max)) => (
            format!("*n < {} || *n > {}", min, max),
            format!("{}..={}", min, max),
        ),
        (Some(min), None) => (format!("*n < {}", min), format!("{}..", min)),
        (None, Some(max)) => (format!("*n > {}", max), format!("..={}", max)),
        (None, None) => unreachable!("at least one bound must be present"),
    }
}

/// Format a range display string for type names.
fn range_display_string(min: Option<i64>, max: Option<i64>) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!("{}..={}", min, max),
        (Some(min), None) => format!("{}..", min),
        (None, Some(max)) => format!("..={}", max),
        (None, None) => unreachable!("at least one bound must be present"),
    }
}

fn known_int_range_overrides() -> &'static HashMap<&'static str, (i64, i64)> {
    static OVERRIDES: LazyLock<HashMap<&'static str, (i64, i64)>> = LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("Ipv4NetmaskLength", (0, 32));
        m.insert("Ipv6NetmaskLength", (0, 128));
        m.insert("FromPort", (-1, 65535));
        m.insert("ToPort", (-1, 65535));
        m
    });
    &OVERRIDES
}

/// Unified resource-specific property type overrides.
/// Maps (Smithy resource name, property name) to a TypeOverride.
/// Use this when a property needs resource-specific type treatment that differs
/// from global overrides or pattern-based inference.
#[allow(dead_code)]
fn resource_type_overrides() -> &'static HashMap<(&'static str, &'static str), TypeOverride> {
    static OVERRIDES: LazyLock<HashMap<(&'static str, &'static str), TypeOverride>> =
        LazyLock::new(HashMap::new);
    &OVERRIDES
}

fn infer_string_type(prop_name: &str) -> Option<String> {
    // Check known string type overrides
    if let Some(&override_type) = known_string_type_overrides().get(prop_name) {
        return Some(override_type.to_string());
    }

    // Normalize plural forms for type inference
    let singular_name = if prop_name.ends_with("Ids")
        || prop_name.ends_with("ids")
        || prop_name.ends_with("Arns")
        || prop_name.ends_with("arns")
    {
        &prop_name[..prop_name.len() - 1]
    } else {
        prop_name
    };

    // Check overrides for singular form too (e.g., list items)
    if let Some(&override_type) = known_string_type_overrides().get(singular_name) {
        return Some(override_type.to_string());
    }

    let prop_lower = singular_name.to_lowercase();

    // CIDR types - differentiate IPv4 vs IPv6 based on property name
    if prop_lower.contains("cidr") {
        if prop_lower.contains("ipv6") {
            return Some("types::ipv6_cidr()".to_string());
        }
        return Some("types::ipv4_cidr()".to_string());
    }

    // IP address types (not CIDR) - e.g., PrivateIpAddress, PublicIp
    if (prop_lower.contains("ipaddress")
        || prop_lower.ends_with("ip")
        || prop_lower.contains("ipaddresses"))
        && !prop_lower.contains("count")
        && !prop_lower.contains("type")
    {
        if prop_lower.contains("ipv6") {
            return Some("types::ipv6_address()".to_string());
        }
        return Some("types::ipv4_address()".to_string());
    }

    // Availability zone (but not AvailabilityZoneId which uses AZ ID format like "use1-az1")
    if prop_lower == "availabilityzone" || prop_lower == "availabilityzones" {
        return Some("super::availability_zone()".to_string());
    }

    // Availability zone ID (e.g., "use1-az1", "usw2-az2")
    if prop_lower == "availabilityzoneid" || prop_lower == "availabilityzoneids" {
        return Some("super::availability_zone_id()".to_string());
    }

    // Region types (e.g., PeerRegion, ServiceRegion, RegionName, ResourceRegion)
    if prop_lower.ends_with("region") || prop_lower == "regionname" {
        return Some("super::aws_region()".to_string());
    }

    // Check ARN pattern
    if prop_lower.ends_with("arn") || prop_lower.ends_with("arns") || prop_lower.contains("_arn") {
        return Some("super::arn()".to_string());
    }

    // IPAM Pool IDs
    if is_ipam_pool_id_property(singular_name) {
        return Some("super::ipam_pool_id()".to_string());
    }

    // Check resource ID pattern
    if is_aws_resource_id_property(singular_name) {
        return Some(get_resource_id_type(singular_name).to_string());
    }

    // AWS Account ID (owner IDs and account IDs are 12-digit account IDs)
    if prop_lower.ends_with("ownerid") || prop_lower.ends_with("accountid") {
        return Some("super::aws_account_id()".to_string());
    }

    // Email address fields. Match conservatively: only when the field IS the
    // email value itself, not arbitrary names that happen to contain "email"
    // (e.g. EmailEnabled, EmailNotificationConfig).
    if is_email_property(prop_name) {
        return Some("types::email()".to_string());
    }

    None
}

/// Returns true if a property name represents an email address value.
///
/// Conservative match: the name must be exactly "Email"/"EmailAddress" (or
/// a plural form), or end with "Email"/"EmailAddress" preceded by a word
/// boundary (typically PascalCase, e.g. "MasterAccountEmail",
/// "ContactEmailAddress"). Names like "EmailEnabled" or
/// "EmailNotificationConfig" are intentionally NOT matched.
fn is_email_property(prop_name: &str) -> bool {
    let lower = prop_name.to_lowercase();
    lower.ends_with("email")
        || lower.ends_with("emails")
        || lower.ends_with("emailaddress")
        || lower.ends_with("emailaddresses")
}

fn is_aws_resource_id_property(prop_name: &str) -> bool {
    let lower = prop_name.to_lowercase();
    let resource_id_suffixes = [
        "vpcid",
        "subnetid",
        "groupid",
        "gatewayid",
        "routetableid",
        "allocationid",
        "networkinterfaceid",
        "instanceid",
        "endpointid",
        "connectionid",
        "prefixlistid",
        "eniid",
    ];
    if lower.contains("owner") || lower.contains("availabilityzone") || lower == "resourceid" {
        return false;
    }
    let singular = if lower.ends_with("ids") {
        &lower[..lower.len() - 1]
    } else {
        &lower
    };
    resource_id_suffixes
        .iter()
        .any(|suffix| lower.ends_with(suffix) || singular.ends_with(suffix))
}

fn is_ipam_pool_id_property(prop_name: &str) -> bool {
    let lower = prop_name.to_lowercase();
    if lower.contains("owner") || lower.contains("availabilityzone") || lower == "resourceid" {
        return false;
    }
    lower.ends_with("poolid")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceIdKind {
    VpcId,
    SubnetId,
    SecurityGroupId,
    EgressOnlyInternetGatewayId,
    InternetGatewayId,
    RouteTableId,
    NatGatewayId,
    VpcPeeringConnectionId,
    TransitGatewayId,
    VpnGatewayId,
    VpcEndpointId,
    InstanceId,
    NetworkInterfaceId,
    AllocationId,
    PrefixListId,
    CarrierGatewayId,
    LocalGatewayId,
    NetworkAclId,
    Generic,
}

fn classify_resource_id(prop_name: &str) -> ResourceIdKind {
    let lower = prop_name.to_lowercase();
    if lower.ends_with("vpcid") || lower == "vpcid" {
        return ResourceIdKind::VpcId;
    }
    if lower.ends_with("subnetid") || lower == "subnetid" {
        return ResourceIdKind::SubnetId;
    }
    if (lower.contains("securitygroup") || lower.contains("groupid")) && lower.ends_with("id") {
        return ResourceIdKind::SecurityGroupId;
    }
    if lower.contains("egressonlyinternetgateway") && lower.ends_with("id") {
        return ResourceIdKind::EgressOnlyInternetGatewayId;
    }
    if lower.contains("internetgateway") && lower.ends_with("id") {
        return ResourceIdKind::InternetGatewayId;
    }
    if lower.contains("routetable") && lower.ends_with("id") {
        return ResourceIdKind::RouteTableId;
    }
    if lower.contains("natgateway") && lower.ends_with("id") {
        return ResourceIdKind::NatGatewayId;
    }
    if lower.contains("peeringconnection") && lower.ends_with("id") {
        return ResourceIdKind::VpcPeeringConnectionId;
    }
    if lower.contains("transitgateway") && lower.ends_with("id") {
        return ResourceIdKind::TransitGatewayId;
    }
    if lower.contains("vpngateway") && lower.ends_with("id") {
        return ResourceIdKind::VpnGatewayId;
    }
    if lower.contains("vpcendpoint") && lower.ends_with("id") {
        return ResourceIdKind::VpcEndpointId;
    }
    // Instance IDs (e.g., InstanceId)
    if lower.ends_with("instanceid") {
        return ResourceIdKind::InstanceId;
    }
    // Network Interface IDs (e.g., NetworkInterfaceId, EniId)
    if lower.ends_with("networkinterfaceid") || lower.ends_with("eniid") {
        return ResourceIdKind::NetworkInterfaceId;
    }
    // Allocation IDs (e.g., AllocationId)
    if lower.ends_with("allocationid") {
        return ResourceIdKind::AllocationId;
    }
    // Prefix List IDs (e.g., PrefixListId, DestinationPrefixListId)
    if lower.ends_with("prefixlistid") {
        return ResourceIdKind::PrefixListId;
    }
    // Carrier Gateway IDs (e.g., CarrierGatewayId)
    if lower.contains("carriergateway") && lower.ends_with("id") {
        return ResourceIdKind::CarrierGatewayId;
    }
    // Local Gateway IDs (e.g., LocalGatewayId)
    if lower.contains("localgateway") && lower.ends_with("id") {
        return ResourceIdKind::LocalGatewayId;
    }
    // Network ACL IDs (e.g., NetworkAclId)
    if lower.contains("networkacl") && lower.ends_with("id") {
        return ResourceIdKind::NetworkAclId;
    }
    ResourceIdKind::Generic
}

fn get_resource_id_type(prop_name: &str) -> &'static str {
    match classify_resource_id(prop_name) {
        ResourceIdKind::VpcId => "super::vpc_id()",
        ResourceIdKind::SubnetId => "super::subnet_id()",
        ResourceIdKind::SecurityGroupId => "super::security_group_id()",
        ResourceIdKind::EgressOnlyInternetGatewayId => "super::egress_only_internet_gateway_id()",
        ResourceIdKind::InternetGatewayId => "super::internet_gateway_id()",
        ResourceIdKind::RouteTableId => "super::route_table_id()",
        ResourceIdKind::NatGatewayId => "super::nat_gateway_id()",
        ResourceIdKind::VpcPeeringConnectionId => "super::vpc_peering_connection_id()",
        ResourceIdKind::TransitGatewayId => "super::transit_gateway_id()",
        ResourceIdKind::VpnGatewayId => "super::vpn_gateway_id()",
        ResourceIdKind::VpcEndpointId => "super::vpc_endpoint_id()",
        ResourceIdKind::InstanceId => "super::instance_id()",
        ResourceIdKind::NetworkInterfaceId => "super::network_interface_id()",
        ResourceIdKind::AllocationId => "super::allocation_id()",
        ResourceIdKind::PrefixListId => "super::prefix_list_id()",
        ResourceIdKind::CarrierGatewayId => "super::carrier_gateway_id()",
        ResourceIdKind::LocalGatewayId => "super::local_gateway_id()",
        ResourceIdKind::NetworkAclId => "super::network_acl_id()",
        ResourceIdKind::Generic => "super::aws_resource_id()",
    }
}

/// Map resource name to CloudFormation type name for backward compatibility.
fn cf_type_name(resource_name: &str) -> &'static str {
    match resource_name {
        "ec2.Vpc" => "AWS::EC2::VPC",
        "ec2.Subnet" => "AWS::EC2::Subnet",
        "ec2.InternetGateway" => "AWS::EC2::InternetGateway",
        "ec2.RouteTable" => "AWS::EC2::RouteTable",
        "ec2.Route" => "AWS::EC2::Route",
        "ec2.SecurityGroup" => "AWS::EC2::SecurityGroup",
        "ec2.SecurityGroupIngress" => "AWS::EC2::SecurityGroupIngress",
        "ec2.SecurityGroupEgress" => "AWS::EC2::SecurityGroupEgress",
        "ec2.EgressOnlyInternetGateway" => "AWS::EC2::EgressOnlyInternetGateway",
        "ec2.Eip" => "AWS::EC2::EIP",
        "ec2.FlowLog" => "AWS::EC2::FlowLog",
        "ec2.NatGateway" => "AWS::EC2::NatGateway",
        "ec2.SubnetRouteTableAssociation" => "AWS::EC2::SubnetRouteTableAssociation",
        "ec2.TransitGateway" => "AWS::EC2::TransitGateway",
        "ec2.TransitGatewayAttachment" => "AWS::EC2::TransitGatewayAttachment",
        "ec2.VpcEndpoint" => "AWS::EC2::VPCEndpoint",
        "ec2.VpcGatewayAttachment" => "AWS::EC2::VPCGatewayAttachment",
        "ec2.VpcPeeringConnection" => "AWS::EC2::VPCPeeringConnection",
        "ec2.VpnGateway" => "AWS::EC2::VPNGateway",
        "s3.Bucket" => "AWS::S3::Bucket",
        "s3.BucketPolicy" => "AWS::S3::BucketPolicy",
        // No native CloudFormation type — PublicAccessBlock is a property
        // of AWS::S3::Bucket. We synthesize a name to keep cf_type_name a
        // total function for codegen consumers.
        "s3.BucketPublicAccessBlock" => "AWS::S3::BucketPublicAccessBlock",
        // No native CloudFormation type; synthesize for cf_type_name totality.
        "s3.BucketVersioning" => "AWS::S3::BucketVersioning",
        // No native CloudFormation type; synthesize for cf_type_name totality.
        "s3.BucketObjectLockConfiguration" => "AWS::S3::BucketObjectLockConfiguration",
        // No native CloudFormation type; synthesize for cf_type_name totality.
        "s3.BucketServerSideEncryptionConfiguration" => {
            "AWS::S3::BucketServerSideEncryptionConfiguration"
        }
        // No native CloudFormation type; synthesize for cf_type_name totality.
        "s3.BucketAcl" => "AWS::S3::BucketAcl",
        "s3.BucketOwnershipControls" => "AWS::S3::BucketOwnershipControls",
        "s3.BucketReplicationConfiguration" => "AWS::S3::BucketReplicationConfiguration",
        "s3.BucketLifecycleConfiguration" => "AWS::S3::BucketLifecycleConfiguration",
        "s3.BucketWebsiteConfiguration" => "AWS::S3::BucketWebsiteConfiguration",
        "s3.BucketCorsConfiguration" => "AWS::S3::BucketCorsConfiguration",
        "s3.BucketNotificationConfiguration" => "AWS::S3::BucketNotificationConfiguration",
        "s3.BucketLogging" => "AWS::S3::BucketLogging",
        "sts.CallerIdentity" => "AWS::STS::CallerIdentity",
        "organizations.Organization" => "AWS::Organizations::Organization",
        "organizations.Account" => "AWS::Organizations::Account",
        "route53.RecordSet" => "AWS::Route53::RecordSet",
        "iam.Role" => "AWS::IAM::Role",
        // Synthetic: no native CloudFormation type.
        "iam.Roles" => "AWS::IAM::Roles",
        "logs.LogGroup" => "AWS::Logs::LogGroup",
        "identitystore.User" => "AWS::IdentityStore::User",
        "acm.Certificate" => "AWS::CertificateManager::Certificate",
        "sqs.Queue" => "AWS::SQS::Queue",
        _ => panic!(
            "Unknown resource: {}. Add it to cf_type_name().",
            resource_name
        ),
    }
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result
}

fn escape_description(desc: &str) -> String {
    let stripped = strip_html_tags(desc);
    let normalized = stripped.replace('"', "\\\"").replace(['\n', '\t'], " ");
    collapse_whitespace(&normalized).trim().to_string()
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        // Find a safe UTF-8 boundary at or before max_len
        let boundary = s
            .char_indices()
            .take_while(|&(i, _)| i <= max_len)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("{}...", &s[..boundary])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_model(service: &str, namespace: &str) -> SmithyModel {
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy");
        load_model(&model_dir, namespace)
            .unwrap_or_else(|error| panic!("load {service} Smithy fixture: {error:#}"))
    }

    #[test]
    fn unclassified_read_attribute_fails_generation() {
        let model = fixture_model("sqs", "com.amazonaws.sqs");
        let resource = resource_defs::sqs_resources()
            .into_iter()
            .find(|resource| resource.name == "sqs.Queue")
            .expect("sqs.Queue definition");
        let mut dispositions = resource.read_dispositions();
        dispositions.retain(|disposition| disposition.field() != "QueueName");

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("a schema attribute without a read disposition must fail generation");
        assert!(error.to_string().contains("QueueName"), "{error:#}");
    }

    #[test]
    fn write_only_read_disposition_requires_create_only_attribute() {
        let model = fixture_model("logs", "com.amazonaws.cloudwatchlogs");
        let resource = resource_defs::logs_resources()
            .into_iter()
            .find(|resource| resource.name == "logs.LogGroup")
            .expect("logs.LogGroup definition");
        let mut dispositions = resource.read_dispositions();
        dispositions.retain(|disposition| disposition.field() != "RetentionInDays");
        dispositions.push(resource_defs::ReadDisposition::WriteOnly {
            field: "RetentionInDays",
            reason: "test fixture",
        });

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("write-only must not hide an updatable attribute");
        assert!(error.to_string().contains("RetentionInDays"), "{error:#}");
        assert!(error.to_string().contains("create-only"), "{error:#}");
    }

    #[test]
    fn schema_omission_requires_a_non_empty_reason() {
        let model = fixture_model("acm", "com.amazonaws.acm");
        let resource = resource_defs::acm_resources()
            .into_iter()
            .find(|resource| resource.name == "acm.Certificate")
            .expect("acm.Certificate definition");
        let mut dispositions = resource.read_dispositions();
        dispositions.retain(|disposition| disposition.field() != "DomainName");
        dispositions.push(resource_defs::ReadDisposition::OmitSchema {
            field: "DomainName",
            reason: "",
        });

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("omitting a schema attribute without a reason must fail generation");
        assert!(error.to_string().contains("DomainName"), "{error:#}");
        assert!(error.to_string().contains("reason"), "{error:#}");
    }

    #[test]
    fn mode_constructor_golden() {
        let rendered = [(false, false), (true, false), (false, true), (true, true)]
            .map(|(required, read_only)| {
                format!(
                    "AttributeSchema::{}(\"value\", AttributeType::string())",
                    mode_constructor(required, read_only)
                )
            })
            .join("\n");

        assert_eq!(
            rendered,
            "AttributeSchema::optional(\"value\", AttributeType::string())\n\
             AttributeSchema::required(\"value\", AttributeType::string())\n\
             AttributeSchema::provider_populated(\"value\", AttributeType::string())\n\
             AttributeSchema::provider_populated(\"value\", AttributeType::string())"
        );
    }

    #[test]
    fn markdown_output_names_encode_data_source_kind() {
        assert_eq!(markdown_file_name("Bucket", false), "bucket.md");
        assert_eq!(markdown_file_name("Bucket", true), "bucket_data_source.md");
        assert_eq!(
            markdown_file_name("CallerIdentity", true),
            "caller_identity_data_source.md"
        );
        assert_eq!(markdown_file_name("Roles", true), "roles_data_source.md");
    }

    #[test]
    fn markdown_examples_dir_must_be_an_existing_directory() {
        let root = tempfile::tempdir().expect("temporary root");
        let missing = root.path().join("missing");
        let file = tempfile::NamedTempFile::new_in(root.path()).expect("temporary file");

        assert!(validate_examples_dir(Some(root.path())).is_ok());
        assert!(validate_examples_dir(None).is_ok());
        assert!(validate_examples_dir(Some(&missing)).is_err());
        assert!(validate_examples_dir(Some(file.path())).is_err());
    }

    #[test]
    fn emit_arn_helper_uses_shared_iam_constructors() {
        assert_eq!(
            emit_arn_helper("iam", "Role", arn_emit_choice("iam", "Role")),
            "pub fn arn() -> AttributeType {\n    super::iam_role_arn()\n}\n\n"
        );
        assert_eq!(
            emit_arn_helper("iam", "Policy", arn_emit_choice("iam", "Policy")),
            "pub fn arn() -> AttributeType {\n    super::iam_policy_arn()\n}\n\n"
        );
        assert_eq!(
            emit_arn_helper(
                "iam",
                "OidcProvider",
                arn_emit_choice("iam", "OidcProvider")
            ),
            "pub fn arn() -> AttributeType {\n    super::iam_oidc_provider_arn()\n}\n\n"
        );
    }

    #[test]
    fn emit_arn_helper_uses_shared_kms_constructor() {
        assert_eq!(
            emit_arn_helper("kms", "Key", arn_emit_choice("kms", "Key")),
            "pub fn arn() -> AttributeType {\n    super::kms_key_arn()\n}\n\n"
        );
    }

    #[test]
    fn virtual_kms_helper_matches_shared_catalog_constructor_output() {
        let output_dir = tempfile::tempdir().expect("create output directory");
        let mut modules = Vec::new();

        generate_virtual_helper_modules(output_dir.path(), &mut modules)
            .expect("generate virtual helper modules");

        let generated = std::fs::read_to_string(output_dir.path().join("kms/key.rs"))
            .expect("read generated KMS helper");
        assert_eq!(
            generated,
            "//! Auto-generated helper schema module for AWS KMS Key identifiers\n\
             //!\n\
             //! DO NOT EDIT MANUALLY - regenerate with:\n\
             //!   ./carina-provider-aws/scripts/generate-schemas-smithy.sh\n\n\
             use carina_core::schema::AttributeType;\n\n\
             pub fn arn() -> AttributeType {\n\
             \x20   super::kms_key_arn()\n\
             }\n\n\
             pub fn id() -> AttributeType {\n\
             \x20   super::kms_key_id()\n\
             }\n"
        );
    }

    #[test]
    fn type_display_names_include_shared_iam_arn_constructors() {
        assert_eq!(type_code_to_display("super::iam_role_arn()"), "IamRoleArn");
        assert_eq!(
            type_code_to_display("super::iam_policy_arn()"),
            "IamPolicyArn"
        );
        assert_eq!(
            type_code_to_display("super::iam_oidc_provider_arn()"),
            "IamOidcProviderArn"
        );
    }

    #[test]
    fn type_display_names_include_route53_hosted_zone_id_union() {
        assert_eq!(
            type_code_to_display(
                "AttributeType::union(vec![super::cloudfront_hosted_zone_id(), super::route53_hosted_zone_id()])"
            ),
            "CloudFrontHostedZoneId \\| Route53HostedZoneId"
        );
    }

    #[test]
    fn type_display_union_splits_only_on_top_level_commas() {
        assert_eq!(
            type_code_to_display(
                "AttributeType::union(vec![super::route53_hosted_zone_id(a, b), super::cloudfront_hosted_zone_id()])"
            ),
            "Route53HostedZoneId \\| CloudFrontHostedZoneId"
        );
    }

    fn test_attr(
        provider_name: &str,
        snake_name: &str,
        type_code: &str,
        is_read_only: bool,
        is_plain_string: bool,
        is_identity: bool,
        enum_info: Option<EnumInfo>,
    ) -> AttrInfo {
        AttrInfo {
            snake_name: snake_name.to_string(),
            provider_name: provider_name.to_string(),
            type_code: type_code.to_string(),
            is_required: false,
            is_create_only: false,
            is_read_only,
            is_write_only: false,
            read_presence: resource_defs::ReadPresence::Optional,
            read_reason: None,
            is_plain_string,
            is_identity,
            is_deferred_populate: false,
            description: None,
            enum_info,
        }
    }

    #[test]
    fn replacement_emission_covers_unique_name_coexisting_and_conflicting() {
        fn emitted(replacement: ReplacementClass, attrs: &[AttrInfo]) -> Result<String> {
            let mut code = String::new();
            emit_replacement_builder(&mut code, "test.Resource", replacement, attrs)?;
            Ok(code)
        }

        let attrs = vec![test_attr(
            "DBInstanceIdentifier",
            "db_instance_identifier",
            PLAIN_STRING_TYPE_CODE,
            false,
            true,
            false,
            None,
        )];
        assert_eq!(
            emitted(ReplacementClass::UniqueName("DbInstanceIdentifier"), &attrs).unwrap(),
            "\x20       .with_unique_name_attribute(\"db_instance_identifier\")\n"
        );
        assert_eq!(
            emitted(ReplacementClass::Coexisting, &[]).unwrap(),
            "\x20       .with_coexisting_replacement()\n"
        );
        assert_eq!(
            emitted(ReplacementClass::Conflicting, &[]).unwrap(),
            "\x20       .with_conflicting_replacement()\n"
        );
    }

    #[test]
    fn unique_name_replacement_requires_present_writable_plain_string_attribute() {
        fn error_for(attrs: &[AttrInfo]) -> String {
            emit_replacement_builder(
                &mut String::new(),
                "test.Resource",
                ReplacementClass::UniqueName("RoleName"),
                attrs,
            )
            .unwrap_err()
            .to_string()
        }

        assert_eq!(
            error_for(&[]),
            "test.Resource replacement unique-name attribute RoleName is missing"
        );
        assert_eq!(
            error_for(&[test_attr(
                "RoleName",
                "role_name",
                PLAIN_STRING_TYPE_CODE,
                true,
                true,
                false,
                None,
            )]),
            "test.Resource replacement unique-name attribute RoleName is read-only"
        );
        assert_eq!(
            error_for(&[test_attr(
                "RoleName",
                "role_name",
                "super::iam_role_name()",
                false,
                true,
                false,
                None,
            )]),
            "test.Resource replacement unique-name attribute RoleName is not a plain string"
        );
        assert_eq!(
            error_for(&[test_attr(
                "RoleName",
                "role_name",
                PLAIN_STRING_TYPE_CODE,
                false,
                false,
                false,
                None,
            )]),
            "test.Resource replacement unique-name attribute RoleName is not a plain string"
        );
        assert_eq!(
            error_for(&[
                test_attr(
                    "RoleName",
                    "role_name",
                    PLAIN_STRING_TYPE_CODE,
                    false,
                    true,
                    false,
                    None,
                ),
                test_attr(
                    "Path",
                    "path",
                    PLAIN_STRING_TYPE_CODE,
                    false,
                    true,
                    true,
                    None,
                ),
            ]),
            "test.Resource replacement unique-name attribute RoleName is on a resource with a composite identity"
        );
    }

    #[test]
    fn generate_data_source_for_sts_caller_identity_emits_explicit_outputs() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/sts.json");
        if !fixture.exists() {
            eprintln!("Skipping: Smithy fixture not found: {}", fixture.display());
            return;
        }
        let file = std::fs::File::open(&fixture).expect("open Smithy fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).expect("parse");
        let ds = resource_defs::sts_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let generated = generate_data_source(&ds, &model, false).expect("generate_data_source");

        assert!(
            generated.contains(".as_data_source()"),
            "must mark as data source: {generated}"
        );
        assert!(
            generated.contains(
                r#"AttributeSchema::provider_populated("account_id", super::aws_account_id())"#
            ),
            "account_id must use aws_account_id(): {generated}"
        );
        assert!(
            generated.contains(r#"AttributeSchema::provider_populated("arn", self::arn())"#),
            "arn must use arn(): {generated}"
        );
        assert!(
            generated.contains(
                r#"AttributeSchema::provider_populated("user_id", AttributeType::string())"#
            ),
            "user_id must be String: {generated}"
        );
        assert!(
            generated.contains(".with_provider_name(\"Account\")"),
            "account_id keeps Account provider_name: {generated}"
        );
    }

    #[test]
    fn generate_data_source_for_identitystore_user_emits_inputs_and_outputs() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/identitystore.json");
        if !fixture.exists() {
            return;
        }
        let file = std::fs::File::open(&fixture).unwrap();
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
        let ds = resource_defs::identitystore_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let generated = generate_data_source(&ds, &model, false).expect("generate_data_source");

        assert!(generated.contains(
            r#"AttributeSchema::required("identity_store_id", AttributeType::string())"#
        ));
        assert!(generated.contains(
            r#"AttributeSchema::provider_populated("display_name", AttributeType::string())"#
        ));
        assert!(generated.contains(
            r#"AttributeSchema::provider_populated("name", super::identitystore_user_name())"#
        ));
        assert!(generated.contains(
            r#"AttributeSchema::provider_populated("emails", AttributeType::unordered_list(types::email()))"#
        ));
    }

    #[test]
    fn markdown_data_source_lists_explicit_output_attributes() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/sts.json");
        if !fixture.exists() {
            return;
        }
        let file = std::fs::File::open(&fixture).unwrap();
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
        let ds = resource_defs::sts_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let md = generate_markdown_data_source(&ds, &model, None).expect("md");

        assert!(md.contains("### `account_id`"), "{md}");
        assert!(md.contains("### `arn`"), "{md}");
        assert!(md.contains("### `user_id`"), "{md}");
    }

    /// `iam.Roles` is the first list-aggregated data source. The codegen
    /// must wrap each declared `item_type` in `AttributeType::List { ... }`
    /// in both the schema and the markdown output so consumers see
    /// `arns: List(...)` rather than scalar.
    #[test]
    fn generate_data_source_for_iam_roles_emits_list_attributes() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/iam.json");
        if !fixture.exists() {
            eprintln!("Skipping: Smithy fixture not found: {}", fixture.display());
            return;
        }
        let file = std::fs::File::open(&fixture).expect("open iam fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).expect("parse iam");
        let ds = resource_defs::iam_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let generated = generate_data_source(&ds, &model, false).expect("generate_data_source");

        assert!(
            generated.contains(".as_data_source()"),
            "must mark as data source: {generated}"
        );
        // arns is List<iam_role_arn>
        assert!(
            generated.contains(
                "AttributeSchema::provider_populated(\"arns\", AttributeType::unordered_list(super::super::iam::role::arn()))"
            ),
            "arns must be List(iam_role_arn): {generated}"
        );
        // names is List<String>
        assert!(
            generated.contains(
                "AttributeSchema::provider_populated(\"names\", AttributeType::unordered_list(AttributeType::string()))"
            ),
            "names must be List(String): {generated}"
        );
        // Inputs are both optional strings.
        assert!(
            generated
                .contains(r#"AttributeSchema::optional("path_prefix", AttributeType::string())"#),
            "path_prefix input: {generated}"
        );
        assert!(
            generated
                .contains(r#"AttributeSchema::optional("name_regex", AttributeType::string())"#),
            "name_regex input: {generated}"
        );
        // Neither input uses the required typestate constructor.
        let path_prefix_idx = generated
            .find("path_prefix")
            .expect("path_prefix in generated");
        let name_regex_idx = generated
            .find("name_regex")
            .expect("name_regex in generated");
        let path_prefix_block = &generated[path_prefix_idx..name_regex_idx];
        assert!(
            !path_prefix_block.contains("AttributeSchema::required"),
            "path_prefix must not be required: {path_prefix_block}"
        );
    }

    #[test]
    fn markdown_data_source_for_iam_roles_emits_list_types() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/iam.json");
        if !fixture.exists() {
            return;
        }
        let file = std::fs::File::open(&fixture).unwrap();
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
        let ds = resource_defs::iam_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let md = generate_markdown_data_source(&ds, &model, None).expect("md");

        assert!(md.contains("### `arns`"), "arns section: {md}");
        assert!(md.contains("### `names`"), "names section: {md}");
        assert!(
            md.contains("List(") || md.contains("- **Type:** List"),
            "List(...) type marker for aggregated outputs: {md}"
        );
    }

    #[test]
    fn markdown_resource_for_iam_role_displays_specific_arn_type() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/iam.json");
        if !fixture.exists() {
            return;
        }
        let file = std::fs::File::open(&fixture).unwrap();
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).unwrap();
        let res = resource_defs::iam_resources()
            .into_iter()
            .find(|r| r.name == "iam.Role")
            .expect("iam.Role missing");

        let md = generate_markdown_resource(&res, &model, None).expect("md");

        assert!(md.contains("### `arn`"), "{md}");
        assert!(md.contains("- **Type:** IamRoleArn"), "{md}");
    }

    #[test]
    fn markdown_marks_required_synthetic_object_lock_attribute() {
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.s3#PutObjectLockConfiguration": {
                  "type": "operation",
                  "input": {
                    "target": "com.amazonaws.s3#PutObjectLockConfigurationRequest"
                  },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.s3#PutObjectLockConfigurationRequest": {
                  "type": "structure",
                  "members": {},
                  "traits": { "smithy.api#input": {} }
                }
              }
            }"##,
        )
        .expect("parse minimal S3 Object Lock model");
        let resource = resource_defs::s3_resources()
            .into_iter()
            .find(|resource| resource.name == "s3.BucketObjectLockConfiguration")
            .expect("missing Object Lock resource");

        let markdown = generate_markdown_resource(&resource, &model, None).expect("markdown");
        let object_lock_section = markdown
            .split("### `object_lock_enabled`")
            .nth(1)
            .expect("object_lock_enabled section")
            .split("### `rule`")
            .next()
            .unwrap();
        assert!(
            object_lock_section.contains("- **Required:** Yes"),
            "{object_lock_section}"
        );
        let rule_section = markdown.split("### `rule`").nth(1).expect("rule section");
        assert!(
            rule_section.contains("must contain exactly one of `days` or `years`"),
            "{rule_section}"
        );
    }

    /// Per naming-conventions design D7 the codegen must emit a
    /// `dsl_aliases` data table for every `StringEnum`, not just the
    /// hyphen-only case. route53.RecordSet's Type enum (`A`, `AAAA`,
    /// `CNAME`, ...) is the canonical SHOUTY-snake case: each value gets
    /// a snake_case alias (`a`, `aaaa`, `cname`) so users can write
    /// `aws.route53.RecordSet.Type.a`.
    #[test]
    fn generate_resource_emits_dsl_aliases_for_pascal_enum() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/route53.json");
        if !fixture.exists() {
            eprintln!(
                "Skipping: Smithy fixture not found: {}\nRun scripts/download-smithy-models.sh to enable this test",
                fixture.display()
            );
            return;
        }
        let file = std::fs::File::open(&fixture).expect("failed to open route53 fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file))
            .expect("failed to parse route53 fixture");
        let resource = resource_defs::route53_resources()
            .into_iter()
            .find(|res| res.name == "route53.RecordSet")
            .expect("missing route53.RecordSet resource def");

        let generated = generate_resource(&resource, &model).expect("failed to generate resource");

        // The validator's allow-list (VALID_TYPE) must stay API-canonical;
        // DSL spellings are carried by schema dsl_aliases.
        let valid_type_line = generated
            .lines()
            .find(|line| line.starts_with("const VALID_TYPE:"))
            .expect("VALID_TYPE constant should be generated");
        assert!(
            valid_type_line.contains("\"A\"")
                && valid_type_line.contains("\"CNAME\"")
                && !valid_type_line.contains("\"a\"")
                && !valid_type_line.contains("\"cname\""),
            "VALID_TYPE should list only API spellings: {generated}"
        );
        // The StringEnum's `dsl_aliases` must contain `(api, dsl)` pairs
        // as data so the mapping survives the WASM-component boundary.
        assert!(
            generated.contains("(\"A\".to_string(), \"a\".to_string())"),
            "dsl_aliases should include (A, a): {generated}"
        );
        assert!(
            generated.contains("(\"CNAME\".to_string(), \"cname\".to_string())"),
            "dsl_aliases should include (CNAME, cname): {generated}"
        );
        assert!(
            !generated.contains("enum_alias_reverse"),
            "provider-side reverse alias lookup must not be generated: {generated}"
        );
        assert!(
            !generated.contains("enum_alias_entries"),
            "provider-side alias tables must not be generated: {generated}"
        );
    }

    /// PascalCase mixed-case enums (e.g. `BucketOwnerEnforced`) must also
    /// produce `bucket_owner_enforced` aliases. Exercises s3.Bucket's
    /// ObjectOwnership-style enum if available, falling back to a check
    /// that the codegen at least emits a non-empty `dsl_aliases` for
    /// PascalCase values.
    #[test]
    fn generate_resource_dsl_aliases_handles_pascal_case_mixed() {
        // Use the dsl helper directly so the assertion does not depend on
        // a specific Smithy fixture being present.
        use carina_aws_types::dsl_enum_value;
        assert_eq!(
            dsl_enum_value("BucketOwnerEnforced"),
            "bucket_owner_enforced"
        );
        assert_eq!(dsl_enum_value("VersioningStatus"), "versioning_status");
        assert_eq!(dsl_enum_value("Enabled"), "enabled");
        assert_eq!(dsl_enum_value("AES256"), "aes256");
    }

    #[test]
    fn generate_resource_uses_enum_for_namespaced_enums() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/s3.json");
        if !fixture.exists() {
            eprintln!(
                "Skipping: Smithy fixture not found: {}\nRun scripts/download-smithy-models.sh to enable this test",
                fixture.display()
            );
            return;
        }
        let file = std::fs::File::open(&fixture).expect("failed to open Smithy fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file))
            .expect("failed to parse Smithy fixture");
        let resource = resource_defs::s3_resources()
            .into_iter()
            .find(|res| res.name == "s3.Bucket")
            .expect("missing s3.Bucket resource def");

        let generated = generate_resource(&resource, &model).expect("failed to generate resource");

        assert!(
            generated.contains("AttributeType::enum_("),
            "enum-like strings should be emitted as enum_: {generated}"
        );
        assert!(
            !generated.contains(".with_completions("),
            "enum completions should come from schema type metadata: {generated}"
        );
    }

    /// Build the inputs `generate_provider_code` needs for a single
    /// resource def, loading the Smithy fixture if available. Returns
    /// `None` when the fixture is missing so the caller can skip
    /// gracefully.
    fn provider_code_for_single_resource(res: ResourceDef, fixture: &str) -> Option<String> {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/")
            .join(fixture);
        if !fixture_path.exists() {
            eprintln!(
                "Skipping: Smithy fixture not found: {}",
                fixture_path.display()
            );
            return None;
        }
        let file = std::fs::File::open(&fixture_path).expect("open fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file)).expect("parse");
        let mut models: HashMap<&str, SmithyModel> = HashMap::new();
        // Leak the namespace string to obtain a 'static borrow for the
        // models map without redesigning the signature.
        let ns: &'static str = Box::leak(res.service_namespace.to_string().into_boxed_str());
        models.insert(ns, model);

        let resources = vec![res];
        let data_sources: Vec<resource_defs::DataSourceDef> = vec![];
        let manual = std::collections::HashSet::new();
        Some(generate_provider_code(
            &resources,
            &data_sources,
            &models,
            &manual,
        ))
    }

    fn provider_code_for_single_resource_with_model(
        res: ResourceDef,
        model: SmithyModel,
    ) -> String {
        let mut models: HashMap<&str, SmithyModel> = HashMap::new();
        let ns = res.service_namespace;
        models.insert(ns, model);

        let resources = vec![res];
        let data_sources: Vec<resource_defs::DataSourceDef> = vec![];
        let manual = std::collections::HashSet::new();
        generate_provider_code(&resources, &data_sources, &models, &manual)
    }

    fn minimal_from_struct_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "metadata": {},
              "shapes": {
                "com.amazonaws.ec2#CreateFromStructFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateFromStructFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateFromStructFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "FlatOptions": { "target": "smithy.api#String", "traits": {} },
                    "Options": { "target": "com.amazonaws.ec2#Options", "traits": {} },
                    "VpcId": { "target": "smithy.api#String", "traits": {} },
                    "PeerVpcId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#FromStructFixture": {
                  "type": "structure",
                  "members": {
                    "RequesterVpcInfo": { "target": "com.amazonaws.ec2#VpcInfo", "traits": {} },
                    "AccepterVpcInfo": { "target": "com.amazonaws.ec2#VpcInfo", "traits": {} },
                    "FlatOptions": { "target": "com.amazonaws.ec2#Options", "traits": {} },
                    "FooBar": { "target": "smithy.api#String", "traits": {} },
                    "Options": { "target": "com.amazonaws.ec2#Options", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#VpcInfo": {
                  "type": "structure",
                  "members": {
                    "VpcId": { "target": "smithy.api#String", "traits": {} },
                    "OwnerId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#Options": {
                  "type": "structure",
                  "members": {
                    "AmazonSideAsn": { "target": "smithy.api#Long", "traits": {} },
                    "DnsSupport": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse minimal FromStruct model")
    }

    fn minimal_from_struct_resource(
        attr: &'static str,
        source: resource_defs::DerivedSource,
    ) -> ResourceDef {
        ResourceDef {
            name: "ec2.FromStructFixture",
            // Synthetic fixture does not exercise replacement behavior.
            replacement: ReplacementClass::Conflicting,
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "CreateFromStructFixture",
            read_structure: Some("FromStructFixture"),
            read_ops: vec![],
            delete_op: "",
            update_ops: vec![],
            identifier: "",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: ["VpcId", "PeerVpcId"]
                .into_iter()
                .filter(|field| *field != attr)
                .collect(),
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![resource_defs::DerivedAttribute { attr, source }],
        }
    }

    fn markdown_regression_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateMarkdownFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateMarkdownFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateMarkdownFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "DomainValidation": { "target": "com.amazonaws.ec2#DomainValidation", "traits": {} },
                    "Example": { "target": "com.amazonaws.ec2#Example", "traits": {} },
                    "Items": { "target": "com.amazonaws.ec2#OptionsList", "traits": {} },
                    "Options": { "target": "com.amazonaws.ec2#Options", "traits": {} },
                    "RenewalSummary": { "target": "com.amazonaws.ec2#RenewalSummary", "traits": {} },
                    "Type": { "target": "com.amazonaws.ec2#TopType", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#OptionsList": {
                  "type": "list",
                  "member": { "target": "com.amazonaws.ec2#Options", "traits": {} },
                  "traits": {}
                },
                "com.amazonaws.ec2#Options": {
                  "type": "structure",
                  "members": {
                    "HostnameType": { "target": "smithy.api#String", "traits": {} },
                    "Type": { "target": "com.amazonaws.ec2#NestedType", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#Example": {
                  "type": "structure",
                  "members": {
                    "Name": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#RenewalSummary": {
                  "type": "structure",
                  "members": {
                    "DomainValidation": { "target": "com.amazonaws.ec2#DomainValidation", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#DomainValidation": {
                  "type": "structure",
                  "members": {
                    "ValidationMethod": { "target": "com.amazonaws.ec2#ValidationMethod", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#ValidationMethod": {
                  "type": "enum",
                  "members": {
                    "DNS": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "DNS" }
                    }
                  }
                },
                "com.amazonaws.ec2#TopType": {
                  "type": "enum",
                  "members": {
                    "TOP": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "TOP" }
                    }
                  }
                },
                "com.amazonaws.ec2#NestedType": {
                  "type": "enum",
                  "members": {
                    "NESTED": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "NESTED" }
                    }
                  }
                }
              }
            }"##,
        )
        .expect("parse markdown regression model")
    }

    fn markdown_nested_enum_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateMarkdownFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateMarkdownFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateMarkdownFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "Options": { "target": "com.amazonaws.ec2#Options", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#Options": {
                  "type": "structure",
                  "members": {
                    "Type": { "target": "com.amazonaws.ec2#NestedType", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#NestedType": {
                  "type": "enum",
                  "members": {
                    "NESTED": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "NESTED" }
                    }
                  }
                }
              }
            }"##,
        )
        .expect("parse nested enum model")
    }

    fn markdown_struct_order_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateMarkdownFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateMarkdownFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateMarkdownFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "Alpha": { "target": "com.amazonaws.ec2#Alpha", "traits": {} },
                    "Zulu": { "target": "com.amazonaws.ec2#Zulu", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#Alpha": {
                  "type": "structure",
                  "members": {
                    "Child": { "target": "com.amazonaws.ec2#Child", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#Child": {
                  "type": "structure",
                  "members": {
                    "Name": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#Zulu": {
                  "type": "structure",
                  "members": {
                    "Name": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse struct-order model")
    }

    fn markdown_transit_gateway_attachment_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateTransitGatewayVpcAttachment": {
                  "type": "operation",
                  "input": {
                    "target": "com.amazonaws.ec2#CreateTransitGatewayVpcAttachmentRequest"
                  },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateTransitGatewayVpcAttachmentRequest": {
                  "type": "structure",
                  "members": {
                    "Options": {
                      "target": "com.amazonaws.ec2#CreateTransitGatewayVpcAttachmentRequestOptions",
                      "traits": {}
                    }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#CreateTransitGatewayVpcAttachmentRequestOptions": {
                  "type": "structure",
                  "members": {
                    "DnsSupport": {
                      "target": "com.amazonaws.ec2#DnsSupportValue",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.ec2#DnsSupportValue": {
                  "type": "enum",
                  "members": {
                    "ENABLE": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "enable" }
                    }
                  }
                },
                "com.amazonaws.ec2#TransitGatewayVpcAttachment": {
                  "type": "structure",
                  "members": {
                    "TransitGatewayAttachmentId": {
                      "target": "smithy.api#String",
                      "traits": {}
                    }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse transit gateway attachment model")
    }

    fn markdown_read_shape_override_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.acm#RequestCertificate": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.acm#RequestCertificateRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.acm#RequestCertificateRequest": {
                  "type": "structure",
                  "members": {
                    "Options": {
                      "target": "com.amazonaws.acm#CertificateOptions",
                      "traits": {}
                    },
                    "DomainValidationOptions": {
                      "target": "com.amazonaws.acm#RequestDomainValidationList",
                      "traits": {}
                    }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.acm#CertificateOptions": {
                  "type": "structure",
                  "members": {
                    "Export": {
                      "target": "com.amazonaws.acm#Export",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#Export": {
                  "type": "enum",
                  "members": {
                    "ENABLED": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "ENABLED" }
                    }
                  }
                },
                "com.amazonaws.acm#RequestDomainValidationList": {
                  "type": "list",
                  "member": {
                    "target": "com.amazonaws.acm#RequestDomainValidation",
                    "traits": {}
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#RequestDomainValidation": {
                  "type": "structure",
                  "members": {
                    "DomainName": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#CertificateDetail": {
                  "type": "structure",
                  "members": {
                    "CertificateArn": { "target": "smithy.api#String", "traits": {} },
                    "DomainValidationOptions": {
                      "target": "com.amazonaws.acm#DomainValidationList",
                      "traits": {}
                    },
                    "RenewalSummary": {
                      "target": "com.amazonaws.acm#RenewalSummary",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#DomainValidationList": {
                  "type": "list",
                  "member": {
                    "target": "com.amazonaws.acm#DomainValidation",
                    "traits": {}
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#DomainValidation": {
                  "type": "structure",
                  "members": {
                    "ResourceRecord": {
                      "target": "com.amazonaws.acm#ResourceRecord",
                      "traits": {}
                    },
                    "ValidationMethod": {
                      "target": "com.amazonaws.acm#ValidationMethod",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#ResourceRecord": {
                  "type": "structure",
                  "members": {
                    "Type": {
                      "target": "com.amazonaws.acm#RecordType",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#RecordType": {
                  "type": "enum",
                  "members": {
                    "CNAME": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "CNAME" }
                    }
                  }
                },
                "com.amazonaws.acm#RenewalSummary": {
                  "type": "structure",
                  "members": {
                    "DomainValidationOptions": {
                      "target": "com.amazonaws.acm#DomainValidationList",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#ValidationMethod": {
                  "type": "enum",
                  "members": {
                    "DNS": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "DNS" }
                    }
                  }
                }
              }
            }"##,
        )
        .expect("parse read-shape override model")
    }

    fn markdown_regression_resource() -> ResourceDef {
        let mut resource = minimal_from_struct_resource(
            "Options",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "Options",
                children: &["HostnameType"],
            },
        );
        resource.name = "ec2.Vpc";
        resource.create_op = "CreateMarkdownFixture";
        resource.read_structure = None;
        resource.derived_attributes.clear();
        resource
    }

    #[test]
    fn markdown_discovers_nested_enums_before_emitting_sections() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");

        assert!(markdown.contains("## Enum Values"), "{markdown}");
        assert!(
            markdown.contains("### options.hostname_type (HostnameType)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_transit_gateway_attachment_headings_use_dsl_field_paths() {
        let resource = resource_defs::ec2_resources()
            .into_iter()
            .find(|resource| resource.name == "ec2.TransitGatewayAttachment")
            .expect("transit gateway attachment resource");
        let markdown = generate_markdown_resource(
            &resource,
            &markdown_transit_gateway_attachment_model(),
            None,
        )
        .expect("generate transit gateway attachment markdown");

        assert!(
            markdown.contains("### options.dns_support (DnsSupport)"),
            "{markdown}"
        );
        assert!(markdown.contains("### options\n\n| Field |"), "{markdown}");
        assert!(
            !markdown.contains("create_transit_gateway_vpc_attachment_request_options"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_acm_headings_use_dsl_field_paths() {
        let resource = resource_defs::acm_resources()
            .into_iter()
            .find(|resource| resource.name == "acm.Certificate")
            .expect("certificate resource");
        let markdown =
            generate_markdown_resource(&resource, &markdown_read_shape_override_model(), None)
                .expect("generate certificate markdown");

        for heading in [
            "### options.export (Export)",
            "### domain_validation_options.resource_record.type (Type)",
            "### options",
            "### domain_validation_options",
            "### renewal_summary.domain_validation_options",
        ] {
            assert!(markdown.contains(heading), "missing {heading}:\n{markdown}");
        }
        assert!(!markdown.contains("### CertificateOptions"), "{markdown}");
        assert!(!markdown.contains("### DomainValidation\n"), "{markdown}");
    }

    #[test]
    fn markdown_heading_paths_only_use_schema_field_names() {
        let model = markdown_read_shape_override_model();
        let root_targets = HashMap::from([(
            "renewal_summary".to_string(),
            "com.amazonaws.acm#RenewalSummary".to_string(),
        )]);
        let valid = [
            "renewal_summary",
            "domain_validation_options",
            "resource_record",
            "type",
        ]
        .map(str::to_string);

        validate_markdown_heading_paths(&model, &root_targets, [valid.as_slice()])
            .expect("every qualified heading segment is a schema field");

        let invalid = [
            "renewal_summary",
            "domain_validation",
            "resource_record",
            "type",
        ]
        .map(str::to_string);
        let error = validate_markdown_heading_paths(&model, &root_targets, [invalid.as_slice()])
            .expect_err("a Smithy shape name is not a DSL field path");
        assert!(error.to_string().contains("domain_validation"), "{error:#}");
    }

    #[test]
    fn markdown_uses_registered_slug_for_attribute_struct_collision() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");

        assert!(
            markdown.contains("- **Type:** [Struct(Options)](#options-1)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_renders_list_links_outside_code_spans() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");

        assert!(
            markdown.contains("List\\<[Struct(Options)](#items-1)\\>"),
            "{markdown}"
        );
        assert!(!markdown.contains("`List<["), "{markdown}");
    }

    #[test]
    fn markdown_qualifies_distinct_nested_enums_with_the_same_field_name() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");

        assert!(markdown.contains("### type (Type)"), "{markdown}");
        assert!(markdown.contains("### options.type (Type)"), "{markdown}");
        assert!(markdown.contains("`aws.ec2.Vpc.Type.top`"), "{markdown}");
        assert!(
            markdown.contains("`aws.ec2.Vpc.Options.Type.nested`"),
            "{markdown}"
        );
        assert!(
            markdown.contains("[Enum (Type)](#optionstype-type)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_enum_sections_are_sorted_by_visible_heading() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");
        let enum_section = markdown
            .split_once("## Enum Values\n\n")
            .expect("enum values section")
            .1
            .split_once("\n## Struct Definitions")
            .expect("struct definitions section")
            .0;
        let headings: Vec<&str> = enum_section
            .lines()
            .filter(|line| line.starts_with("### "))
            .collect();
        let mut sorted = headings.clone();
        sorted.sort_unstable();

        assert_eq!(headings, sorted, "{markdown}");
    }

    #[test]
    fn markdown_struct_sections_are_sorted_by_visible_heading() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_struct_order_model(),
            None,
        )
        .expect("generate markdown");
        let struct_section = markdown
            .split_once("## Struct Definitions\n\n")
            .expect("struct definitions section")
            .1;
        let headings: Vec<&str> = struct_section
            .lines()
            .filter(|line| line.starts_with("### "))
            .collect();
        let mut sorted = headings.clone();
        sorted.sort_unstable();

        assert_eq!(headings, sorted, "{markdown}");
    }

    #[test]
    fn nested_enum_heading_does_not_depend_on_an_unrelated_collision() {
        let without_collision = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_nested_enum_model(),
            None,
        )
        .expect("generate markdown without collision");
        let with_collision = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown with collision");

        for markdown in [&without_collision, &with_collision] {
            assert!(
                markdown.contains("### options.type (Type)"),
                "nested enum heading must always include its parent path:\n{markdown}"
            );
        }
    }

    #[test]
    fn markdown_pages_do_not_claim_cloudformation_types() {
        let model =
            carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).expect("parse empty model");
        let data_source = resource_defs::sts_data_sources()
            .into_iter()
            .next()
            .expect("STS data source");

        let markdown = generate_markdown_data_source(&data_source, &model, None).expect("markdown");

        assert!(!markdown.contains("CloudFormation Type"), "{markdown}");
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("resource markdown");
        assert!(!markdown.contains("CloudFormation Type"), "{markdown}");
    }

    #[test]
    fn markdown_lists_nested_enum_identifiers_for_every_struct_path() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            None,
        )
        .expect("generate markdown");

        assert!(
            markdown.contains("`aws.ec2.Vpc.DomainValidation.ValidationMethod.dns`"),
            "{markdown}"
        );
        assert!(
            markdown.contains("`aws.ec2.Vpc.RenewalSummary.DomainValidation.ValidationMethod.dns`"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_lists_every_schema_namespace_for_read_shape_overrides() {
        let model = markdown_read_shape_override_model();
        let mut resource = markdown_regression_resource();
        resource.name = "acm.Certificate";
        resource.service_namespace = "com.amazonaws.acm";
        resource.create_op = "RequestCertificate";
        resource.read_structure = Some("CertificateDetail");
        resource.identifier = "CertificateArn";
        resource.extra_read_only = vec!["DomainValidationOptions", "RenewalSummary"];
        resource.read_shape_overrides = vec!["DomainValidationOptions"];

        let markdown =
            generate_markdown_resource(&resource, &model, None).expect("generate markdown");

        assert!(
            markdown.contains("`aws.acm.Certificate.DomainValidation.ValidationMethod.dns`"),
            "{markdown}"
        );
        assert!(
            markdown.contains(
                "`aws.acm.Certificate.RenewalSummary.DomainValidation.ValidationMethod.dns`"
            ),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_gives_distinct_headings_to_distinct_enums_in_one_namespace() {
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateMarkdownFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateMarkdownFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateMarkdownFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "First": { "target": "fixture.one#Options", "traits": {} },
                    "FirstProtocol": { "target": "fixture.one#Protocol", "traits": {} },
                    "SecondProtocol": { "target": "fixture.one#Protocol", "traits": {} },
                    "Second": { "target": "fixture.two#Options", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "fixture.one#Options": {
                  "type": "structure",
                  "members": {
                    "Protocol": { "target": "fixture.one#Protocol", "traits": {} }
                  },
                  "traits": {}
                },
                "fixture.two#Options": {
                  "type": "structure",
                  "members": {
                    "Protocol": { "target": "fixture.two#Protocol", "traits": {} }
                  },
                  "traits": {}
                },
                "fixture.one#Protocol": {
                  "type": "enum",
                  "members": {
                    "ONE": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "ONE" }
                    }
                  }
                },
                "fixture.two#Protocol": {
                  "type": "enum",
                  "members": {
                    "TWO": {
                      "target": "smithy.api#Unit",
                      "traits": { "smithy.api#enumValue": "TWO" }
                    }
                  }
                }
              }
            }"##,
        )
        .expect("parse same-namespace enum model");

        let markdown = generate_markdown_resource(&markdown_regression_resource(), &model, None)
            .expect("distinct enums must generate unique headings");
        let ids = carina_docgen::heading_ids(&markdown);
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "{markdown}");
        assert!(markdown.contains("`aws.ec2.Vpc.Options.Protocol.one`"));
        assert!(markdown.contains("`aws.ec2.Vpc.Options.Protocol.two`"));
        assert!(markdown.contains("`aws.ec2.Vpc.FirstProtocol.one`"));
        assert!(markdown.contains("`aws.ec2.Vpc.SecondProtocol.one`"));
    }

    #[test]
    fn markdown_example_preprocessing_matches_the_previous_script_pipeline() {
        let source = concat!(
            "\n",
            "# leading comment\n",
            "provider aws {\n",
            "  region = \"us-east-1\"\n",
            "}\n",
            "\n",
            "resource bucket \"fixture\" {}\n",
            "# trailing comment\n",
        );

        assert_eq!(
            preprocess_markdown_example(source),
            "resource bucket \"fixture\" {}\n"
        );

        let trailing_blank = "resource bucket \"fixture\" {}\n\n# trailing comment\n";
        assert_eq!(
            preprocess_markdown_example(trailing_blank),
            "resource bucket \"fixture\" {}\n\n"
        );
    }

    fn provider_code_panic_for_single_resource_with_model(
        res: ResourceDef,
        model: SmithyModel,
    ) -> Option<String> {
        let result =
            std::panic::catch_unwind(|| provider_code_for_single_resource_with_model(res, model));
        match result {
            Ok(_) => None,
            Err(payload) => Some(
                payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string()),
            ),
        }
    }

    #[test]
    fn from_struct_panics_on_empty_children() {
        let res = minimal_from_struct_resource(
            "Options",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "Options",
                children: &[],
            },
        );

        let panic =
            provider_code_panic_for_single_resource_with_model(res, minimal_from_struct_model())
                .expect("FromStruct with empty children should panic");
        assert!(
            panic.contains("must list at least one child member"),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn from_struct_panics_on_flat_rename_with_multiple_children() {
        let res = minimal_from_struct_resource(
            "PeerVpcId",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "AccepterVpcInfo",
                children: &["VpcId", "OwnerId"],
            },
        );

        let panic =
            provider_code_panic_for_single_resource_with_model(res, minimal_from_struct_model())
                .expect("FromStruct flat rename with multiple children should panic");
        assert!(
            panic.contains(
                "emits a top-level leaf attribute 'peer_vpc_id' but lists more than one child"
            ),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn from_struct_panics_when_attr_is_not_declared_in_schema() {
        let res = minimal_from_struct_resource(
            "TypoVpcId",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "RequesterVpcInfo",
                children: &["VpcId"],
            },
        );

        let panic =
            provider_code_panic_for_single_resource_with_model(res, minimal_from_struct_model())
                .expect("FromStruct with undeclared attr should panic");
        assert!(
            panic.contains(
                "FromStruct's `attr` 'TypoVpcId' is not declared in the resource's schema"
            ),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn from_struct_extra_read_only_attr_is_schema_declared() {
        let mut res = minimal_from_struct_resource(
            "FooBar",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "Options",
                children: &["DnsSupport"],
            },
        );
        res.extra_read_only = vec!["FooBar"];

        let code = provider_code_for_single_resource_with_model(res, minimal_from_struct_model());

        assert!(
            code.contains("let foo_bar = obj.options()")
                && code.contains("foo_bar,\n            applied_identifier,"),
            "extra_read_only FromStruct attr should emit without schema-missing panic: {code}"
        );
    }

    #[test]
    fn from_struct_panics_on_name_equality_vs_schema_shape_mismatch() {
        let res = minimal_from_struct_resource(
            "FlatOptions",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "FlatOptions",
                children: &["DnsSupport"],
            },
        );

        let panic =
            provider_code_panic_for_single_resource_with_model(res, minimal_from_struct_model())
                .expect("FromStruct layout mismatch should panic");
        assert!(
            panic.contains("name-equality heuristic and schema-shape lookup disagree on layout")
                && panic.contains("ec2.FromStructFixture")
                && panic.contains("FlatOptions"),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn from_struct_emits_flat_rename_for_leaf_attr() {
        let res = minimal_from_struct_resource(
            "VpcId",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "RequesterVpcInfo",
                children: &["VpcId"],
            },
        );
        let code = provider_code_for_single_resource_with_model(res, minimal_from_struct_model());

        assert!(
            code.contains(
                "let vpc_id = obj.requester_vpc_info()\n            .and_then(|opts| opts.vpc_id())"
            ),
            "must emit flat rename shape: {code}"
        );
        assert!(
            code.contains("vpc_id,\n            applied_identifier,"),
            "must return the leaf as a typed field: {code}"
        );
    }

    #[test]
    fn from_struct_emits_nested_map_for_struct_attr() {
        let res = minimal_from_struct_resource(
            "Options",
            resource_defs::DerivedSource::FromStruct {
                struct_member: "Options",
                children: &["AmazonSideAsn", "DnsSupport"],
            },
        );
        let code = provider_code_for_single_resource_with_model(res, minimal_from_struct_model());

        assert!(
            code.contains("let options = obj.options().and_then(|dns_opts| {"),
            "must emit nested map source walk: {code}"
        );
        assert!(
            code.contains("let mut fields = IndexMap::new();"),
            "must build nested fields map: {code}"
        );
        assert!(
            code.contains(
                "(!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))"
            ) && code.contains("options,\n            applied_identifier,"),
            "must return the nested map as a typed field: {code}"
        );
    }

    #[test]
    fn extract_iam_role_uses_required_field_pattern() {
        let res = resource_defs::iam_resources()
            .into_iter()
            .find(|r| r.name == "iam.Role")
            .expect("iam.Role missing");
        let Some(code) = provider_code_for_single_resource(res, "iam.json") else {
            return;
        };

        // Required strings must use the `let v = obj.<f>(); if !v.is_empty()` shape.
        // The wrong (current) shape is `if let Some(v) = obj.role_name()`, which
        // fails to compile because the SDK returns &str directly for required
        // members.
        assert!(
            code.contains("let v = obj.role_name();"),
            "required role_name must use let-then-empty pattern: {code}"
        );
        assert!(
            code.contains("let v = obj.arn();"),
            "required arn must use let-then-empty pattern: {code}"
        );
        assert!(
            code.contains("let v = obj.path();"),
            "required path must use let-then-empty pattern: {code}"
        );
        assert!(
            code.contains("let v = obj.role_id();"),
            "required role_id must use let-then-empty pattern: {code}"
        );

        // role_name is the identifier; the trailing identifier-return must
        // unwrap directly (no .map(String::from) on a non-Option).
        assert!(
            code.contains("Some(obj.role_name().to_string())"),
            "required identifier role_name must Some(.to_string()): {code}"
        );

        // No regression: optional fields stay optional in the typed result.
        assert!(
            code.contains("let description = obj.description().map(|v|"),
            "optional description must use Option::map: {code}"
        );
    }

    #[test]
    fn generated_extractors_return_typed_read_fields() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.Vpc")
            .expect("ec2.Vpc missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        assert!(
            code.contains("pub(crate) struct Ec2VpcReadFields"),
            "extractor output must be a generated typed struct: {code}"
        );
        assert!(
            code.contains(") -> Ec2VpcReadFields"),
            "extractor must return its typed fields directly: {code}"
        );
        assert!(
            code.contains("cidr_block: Option<Value>"),
            "SDK-optional fields must remain typed options: {code}"
        );
        assert!(
            !code.contains("pub(crate) cidr_block: Option<Value>"),
            "extracted fields must be private so handwritten reads cannot destructure them: {code}"
        );
        assert!(
            code.contains("pub(crate) fn into_model(")
                && code.contains("let Self {")
                && code.contains("let crate::read_models_generated::Ec2VpcReadModelSupplement"),
            "generated code must consume every extracted and hand-supplied field: {code}"
        );
        assert!(
            !code.contains("attributes: &mut HashMap<String, Value>"),
            "extractors must not communicate through string-keyed maps: {code}"
        );
    }

    #[test]
    fn legacy_map_based_read_ops_fail_generation() {
        let mut resource = resource_defs::ec2_resources()
            .into_iter()
            .find(|resource| resource.name == "ec2.Vpc")
            .expect("ec2.Vpc missing");
        resource.read_ops = vec![resource_defs::ReadOp {
            operation: "DescribeVpcs",
            fields: vec![("VpcId", None)],
            defaults: vec![],
        }];

        let panic = provider_code_panic_for_single_resource_with_model(
            resource,
            fixture_model("ec2", "com.amazonaws.ec2"),
        )
        .expect("legacy map-based read_ops must fail generation");
        assert!(
            panic.contains("legacy map-based read_ops are unsupported for ec2.Vpc"),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn generated_read_models_cache_validated_schemas() {
        let resource = resource_defs::ec2_resources()
            .into_iter()
            .find(|resource| resource.name == "ec2.Vpc")
            .expect("ec2.Vpc definition");
        let model = fixture_model("ec2", "com.amazonaws.ec2");
        let mut models = HashMap::new();
        models.insert(resource.service_namespace, model);

        let code = generate_read_models_code(&[resource], &[], &models)
            .expect("generate ec2.Vpc read model");

        assert!(
            code.contains("static VALIDATED_SCHEMA: OnceLock"),
            "each model must cache its validated schema: {code}"
        );
        assert!(
            code.contains("cached_validated_read_schema("),
            "generated models must use the cache seam: {code}"
        );
        assert_eq!(
            code.matches("ec2_vpc_config().schema").count(),
            1,
            "the schema must be built only inside the cache initializer: {code}"
        );
    }

    #[test]
    fn generated_data_source_models_exhaustively_name_schema_attributes() {
        let data_sources = resource_defs::sts_data_sources();
        let code = generate_read_models_code(&[], &data_sources, &HashMap::new())
            .expect("generate STS data-source read model");

        assert!(
            code.contains("pub(crate) struct StsCallerIdentityDataSourceReadModel")
                && code.contains("pub(crate) account_id: Option<Value>")
                && code.contains("pub(crate) arn: Option<Value>")
                && code.contains("pub(crate) user_id: Option<Value>"),
            "data-source output names must be compiler-checked fields: {code}"
        );
        assert!(
            code.contains("impl DataSourceReadModel for StsCallerIdentityDataSourceReadModel"),
            "data-source maps must be produced only at the generated seam: {code}"
        );
    }

    #[test]
    fn extract_route53_record_set_required_name_compiles() {
        let res = resource_defs::route53_resources()
            .into_iter()
            .find(|r| r.name == "route53.RecordSet")
            .expect("route53.RecordSet missing");
        let Some(code) = provider_code_for_single_resource(res, "route53.json") else {
            return;
        };

        assert!(
            code.contains("let v = obj.name();"),
            "required name must use let-then-empty pattern: {code}"
        );
        assert!(
            code.contains("Some(obj.name().to_string())"),
            "required identifier name must Some(.to_string()): {code}"
        );
    }

    #[test]
    fn extract_ec2_nat_gateway_emits_list_first_for_allocation_id() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.NatGateway")
            .expect("ec2.NatGateway missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // The hand-written extractor reads `obj.nat_gateway_addresses().first()`
        // and projects `.allocation_id()` from the inner struct. The codegen
        // must reproduce that walk for the DSL `allocation_id` attribute.
        assert!(
            code.contains("let allocation_id = obj.nat_gateway_addresses().first()"),
            "must walk nat_gateway_addresses().first(): {code}"
        );
        assert!(
            code.contains(".and_then(|item| item.allocation_id())"),
            "must project allocation_id from the list element: {code}"
        );
        assert!(
            code.contains("allocation_id,\n            availability_mode,"),
            "must return allocation_id as a typed field: {code}"
        );
        // Regression guard: the regular pass must not also emit
        // `obj.allocation_id()` (NatGateway has no top-level allocation_id getter,
        // so that would be a compile error).
        assert!(
            !code.contains("obj.allocation_id()"),
            "must not emit obj.allocation_id() at the top level: {code}"
        );
    }

    #[test]
    fn extract_ec2_egress_only_internet_gateway_emits_list_first_for_vpc_id() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.EgressOnlyInternetGateway")
            .expect("ec2.EgressOnlyInternetGateway missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        assert!(
            code.contains("let vpc_id = obj.attachments().first()"),
            "must walk attachments().first(): {code}"
        );
        assert!(
            code.contains(".and_then(|item| item.vpc_id())"),
            "must project vpc_id from the list element: {code}"
        );
        assert!(
            code.contains("vpc_id,\n            applied_identifier,"),
            "must return vpc_id as a typed field: {code}"
        );
        assert!(
            !code.contains("obj.vpc_id()"),
            "must not emit obj.vpc_id() at the top level (EgressOnlyInternetGateway has no top-level vpc_id getter): {code}"
        );
    }

    #[test]
    fn extract_ec2_vpc_endpoint_emits_list_of_string_for_route_table_ids() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.VpcEndpoint")
            .expect("ec2.VpcEndpoint missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // Hand-written shape that the codegen must reproduce for any
        // List<String> read-structure member.
        assert!(
            code.contains("let ids = obj.route_table_ids();"),
            "must read .route_table_ids(): {code}"
        );
        assert!(
            code.contains(
                "let list: Vec<Value> = ids.iter().map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into()))).collect();"
            ),
            "must collect into Vec<Value>: {code}"
        );
        assert!(
            code.contains("let route_table_ids = (!ids.is_empty()).then(||")
                && code.contains("route_table_ids,"),
            "must return route_table_ids as a typed Value::List: {code}"
        );

        assert!(
            code.contains("let ids = obj.subnet_ids();"),
            "must read .subnet_ids(): {code}"
        );
        assert!(
            code.contains("let subnet_ids = (!ids.is_empty()).then(||")
                && code.contains("subnet_ids,"),
            "must return subnet_ids as a typed Value::List: {code}"
        );
    }

    #[test]
    fn extract_ec2_transit_gateway_attachment_emits_subnet_ids_and_from_struct_options() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.TransitGatewayAttachment")
            .expect("ec2.TransitGatewayAttachment missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };
        assert!(
            code.contains("let ids = obj.subnet_ids();"),
            "must read .subnet_ids(): {code}"
        );
        assert!(
            code.contains("let subnet_ids = (!ids.is_empty()).then(||")
                && code.contains("subnet_ids,"),
            "must return subnet_ids as a typed Value::List: {code}"
        );
        assert!(
            code.contains("let options = obj.options().and_then(|dns_opts| {"),
            "must walk obj.options() as a FromStruct source: {code}"
        );
        assert!(
            code.contains("let mut fields = IndexMap::new();"),
            "must build nested options fields map: {code}"
        );
        assert!(
            code.contains("if let Some(v) = dns_opts.dns_support()"),
            "dns_support must read from options map source: {code}"
        );
        assert!(
            code.contains("fields.insert(\"dns_support\".to_string(), Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));"),
            "dns_support must insert inside fields map: {code}"
        );
        assert!(
            code.contains(
                "(!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))"
            ) && code.contains("options,\n            subnet_ids,"),
            "must return the nested options map as a typed field: {code}"
        );
    }

    #[test]
    fn extract_ec2_vpc_endpoint_emits_list_all_for_security_group_ids() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.VpcEndpoint")
            .expect("ec2.VpcEndpoint missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // Walk groups list, project group_id off each element via
        // filter_map, drop the attribute when the result is empty (so an
        // empty list does not show up as Value::List([])).
        assert!(
            code.contains("let list: Vec<Value> = obj.groups()"),
            "must read .groups() as the list source: {code}"
        );
        assert!(
            code.contains(".filter_map(|item| item.group_id().map(|id| Value::Concrete(ConcreteValue::String(id.to_string().into()))))"),
            "must filter_map child .group_id(): {code}"
        );
        assert!(
            code.contains("let security_group_ids = (!list.is_empty())")
                && code.contains("security_group_ids,"),
            "must return security_group_ids as a typed Value::List: {code}"
        );
    }

    #[test]
    fn extract_ec2_transit_gateway_emits_from_struct_options() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.TransitGateway")
            .expect("ec2.TransitGateway missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // Each child of obj.options() lands inside a nested `options`
        // map matching the schema struct.
        // Pin one numeric (amazon_side_asn) and one enum (dns_support) to
        // exercise both Int and Enum emission paths.
        assert!(
            code.contains("let options = obj.options().and_then(|dns_opts| {"),
            "must walk obj.options() as a FromStruct source: {code}"
        );
        assert!(
            code.contains("let mut fields = IndexMap::new();"),
            "must build nested options fields map: {code}"
        );
        assert!(
            code.contains("if let Some(v) = dns_opts.amazon_side_asn()"),
            "amazon_side_asn must read from options map source: {code}"
        );
        assert!(
            // amazon_side_asn is Smithy `Long`, so the SDK getter returns
            // i64 directly — no widening cast.
            code.contains("fields.insert(\"amazon_side_asn\".to_string(), Value::Concrete(ConcreteValue::Int(v)));"),
            "amazon_side_asn must insert as Int inside fields map: {code}"
        );
        assert!(
            code.contains("if let Some(v) = dns_opts.dns_support()"),
            "dns_support must read from options map source: {code}"
        );
        assert!(
            code.contains("fields.insert(\"dns_support\".to_string(), Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));"),
            "dns_support must insert as enum-as-String inside fields map: {code}"
        );
        assert!(
            code.contains(
                "(!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))"
            ) && code.contains("options,\n            transit_gateway_id,"),
            "must return the nested options map as a typed field: {code}"
        );
    }

    #[test]
    fn extract_ec2_vpc_peering_connection_emits_struct_with_rename() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.VpcPeeringConnection")
            .expect("ec2.VpcPeeringConnection missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // Requester: child `vpc_id` keeps its DSL name.
        assert!(
            code.contains(
                "let vpc_id = obj.requester_vpc_info()\n            .and_then(|opts| opts.vpc_id())"
            ),
            "requester_vpc_info.vpc_id must walk obj.requester_vpc_info(): {code}"
        );
        assert!(
            code.contains("vpc_id,\n            vpc_peering_connection_id,"),
            "requester vpc_id returns under the typed vpc_id field: {code}"
        );

        // Accepter: child `vpc_id` is renamed to `peer_vpc_id` (the rename
        // is the whole reason this can't be a direct read-structure member
        // pull). Owner / region get the same rename treatment.
        assert!(
            code.contains("let peer_vpc_id = obj.accepter_vpc_info()\n            .and_then(|opts| opts.vpc_id())"),
            "accepter_vpc_info.vpc_id must walk obj.accepter_vpc_info(): {code}"
        );
        assert!(
            code.contains("peer_vpc_id,\n            vpc_id,"),
            "accepter vpc_id returns under the typed peer_vpc_id field: {code}"
        );
        assert!(
            code.contains("let peer_owner_id = obj.accepter_vpc_info()")
                && code.contains("peer_owner_id,\n            peer_vpc_id,"),
            "accepter owner_id returns under the typed peer_owner_id field: {code}"
        );
        assert!(
            !code.contains("peer_region: Option<Value>") && !code.contains("let peer_region ="),
            "peer_region is excluded from the schema and must not be extracted: {code}"
        );
    }

    #[test]
    fn extract_ec2_subnet_emits_from_struct_map_for_private_dns_options() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.Subnet")
            .expect("ec2.Subnet missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // The whole inner struct collapses into one Value::Map attribute.
        // Pin the outer wrap, the IndexMap collection of children, the
        // empty-map guard, and the final insert.
        assert!(
            code.contains("let private_dns_name_options_on_launch = obj.private_dns_name_options_on_launch().and_then(|dns_opts|"),
            "must walk obj.private_dns_name_options_on_launch(): {code}"
        );
        assert!(
            code.contains("let mut fields = IndexMap::new();"),
            "must use an IndexMap as the collector: {code}"
        );
        // hostname_type → enum-as-String
        assert!(
            code.contains("if let Some(v) = dns_opts.hostname_type()"),
            "must walk inner hostname_type: {code}"
        );
        assert!(
            code.contains("fields.insert(\"hostname_type\".to_string(), Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));"),
            "hostname_type inserts as enum-as-String into fields: {code}"
        );
        // enable_resource_name_dns_a_record → Bool
        assert!(
            code.contains("if let Some(v) = dns_opts.enable_resource_name_dns_a_record()"),
            "must walk inner enable_resource_name_dns_a_record: {code}"
        );
        assert!(
            code.contains(
                "fields.insert(\"enable_resource_name_dns_a_record\".to_string(), Value::Concrete(ConcreteValue::Bool(v)));"
            ),
            "enable_resource_name_dns_a_record inserts as Bool into fields: {code}"
        );
        // Empty-map guard
        assert!(
            code.contains(
                "(!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))"
            ),
            "must guard the typed nested value on non-empty fields: {code}"
        );
        // Typed return field under the DSL attr name.
        assert!(
            code.contains("private_dns_name_options_on_launch,\n            subnet_id,"),
            "must return the collected map under private_dns_name_options_on_launch: {code}"
        );
    }

    #[test]
    fn scan_orphaned_modules_finds_legacy_resources_not_in_resource_defs() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let output_dir = tmp.path();

        // Create a "known" service directory with a known resource
        let ec2_dir = output_dir.join("ec2");
        std::fs::create_dir_all(&ec2_dir).unwrap();
        std::fs::write(ec2_dir.join("vpc.rs"), "// known resource").unwrap();
        std::fs::write(ec2_dir.join("mod.rs"), "pub mod vpc;").unwrap();

        // Create an "orphaned" service directory with a resource not in known_names
        let iam_dir = output_dir.join("iam");
        std::fs::create_dir_all(&iam_dir).unwrap();
        std::fs::write(iam_dir.join("role.rs"), "// orphaned resource").unwrap();
        std::fs::write(iam_dir.join("mod.rs"), "pub mod role;").unwrap();

        // Another orphaned service with a different resource
        let logs_dir = output_dir.join("logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(logs_dir.join("log_group.rs"), "// orphaned").unwrap();
        std::fs::write(logs_dir.join("mod.rs"), "pub mod log_group;").unwrap();

        let known = vec![GeneratedModule {
            service: "ec2".to_string(),
            file_stem: "vpc".to_string(),
            config_fn: "ec2_vpc_config".to_string(),
        }];
        let orphaned = scan_orphaned_modules(output_dir, &known);

        assert_eq!(orphaned, vec!["iam.Role", "logs.LogGroup"]);
    }

    #[test]
    fn generate_mod_rs_preserves_orphaned_modules() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let output_dir = tmp.path();

        // Set up service directories with legacy resources
        let iam_dir = output_dir.join("iam");
        std::fs::create_dir_all(&iam_dir).unwrap();
        std::fs::write(iam_dir.join("role.rs"), "// orphaned").unwrap();

        // Generate with a different known resource (ec2.Vpc)
        let known = vec![GeneratedModule {
            service: "ec2".to_string(),
            file_stem: "vpc".to_string(),
            config_fn: "ec2_vpc_config".to_string(),
        }];
        let generated = generate_mod_rs(&known, output_dir);

        assert!(
            generated.contains("pub mod iam;"),
            "orphaned iam module should be declared: {generated}"
        );
        assert!(
            generated.contains("iam::role::iam_role_config()"),
            "orphaned iam.Role config should be included: {generated}"
        );
        assert!(
            generated.contains("iam::role::enum_valid_values()"),
            "orphaned iam.Role enum values should be included: {generated}"
        );
    }

    #[test]
    fn generate_mod_rs_registers_dual_kinds_for_s3_bucket() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let output_dir = tmp.path();
        let known = vec![
            GeneratedModule {
                service: "s3".to_string(),
                file_stem: "bucket".to_string(),
                config_fn: "s3_bucket_config".to_string(),
            },
            GeneratedModule {
                service: "s3".to_string(),
                file_stem: "bucket_data_source".to_string(),
                config_fn: "s3_bucket_data_source_config".to_string(),
            },
        ];
        let generated = generate_mod_rs(&known, output_dir);
        assert!(generated.contains("s3::bucket::s3_bucket_config()"));
        assert!(generated.contains("s3::bucket_data_source::s3_bucket_data_source_config()"));
        assert!(!generated.contains("get_enum_alias_reverse"));
        assert!(!generated.contains("build_enum_aliases_map"));
    }

    #[test]
    fn infer_string_type_emits_email_for_email_props() {
        // Exact and plural matches.
        assert_eq!(
            infer_string_type("Email"),
            Some("types::email()".to_string())
        );
        assert_eq!(
            infer_string_type("Emails"),
            Some("types::email()".to_string())
        );
        assert_eq!(
            infer_string_type("EmailAddress"),
            Some("types::email()".to_string())
        );
        assert_eq!(
            infer_string_type("EmailAddresses"),
            Some("types::email()".to_string())
        );
        // PascalCase suffix.
        assert_eq!(
            infer_string_type("MasterAccountEmail"),
            Some("types::email()".to_string())
        );
        assert_eq!(
            infer_string_type("ContactEmailAddress"),
            Some("types::email()".to_string())
        );
    }

    #[test]
    fn infer_string_type_does_not_emit_email_for_unrelated_names() {
        // Names that contain "email" but are not email values.
        assert_ne!(
            infer_string_type("EmailEnabled"),
            Some("types::email()".to_string())
        );
        assert_ne!(
            infer_string_type("EmailNotificationConfig"),
            Some("types::email()".to_string())
        );
    }

    #[test]
    fn organizations_account_email_is_email_type() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../carina-provider-aws/tests/fixtures/smithy/organizations.json");
        if !fixture.exists() {
            eprintln!(
                "Skipping: Smithy fixture not found: {}\nRun scripts/download-smithy-models.sh to enable this test",
                fixture.display()
            );
            return;
        }
        let file = std::fs::File::open(&fixture).expect("failed to open Smithy fixture");
        let model = carina_smithy::parse_reader(std::io::BufReader::new(file))
            .expect("failed to parse Smithy fixture");
        let resource = resource_defs::organizations_resources()
            .into_iter()
            .find(|res| res.name == "organizations.Account")
            .expect("missing organizations.Account resource def");

        let generated = generate_resource(&resource, &model).expect("failed to generate resource");

        assert!(
            generated.contains("\"email\", types::email()"),
            "organizations.account.email should be types::email(): {generated}"
        );
        assert!(
            !generated.contains("\"email\", AttributeType::string()"),
            "organizations.account.email should NOT be AttributeType::string(): {generated}"
        );
    }

    #[test]
    fn escape_rust_keyword_escapes_type() {
        assert_eq!(escape_rust_keyword("type"), "r#type");
    }

    #[test]
    fn escape_rust_keyword_escapes_other_keywords() {
        assert_eq!(escape_rust_keyword("match"), "r#match");
        assert_eq!(escape_rust_keyword("ref"), "r#ref");
        assert_eq!(escape_rust_keyword("mod"), "r#mod");
    }

    #[test]
    fn escape_rust_keyword_leaves_non_keywords_unchanged() {
        assert_eq!(escape_rust_keyword("vpc_id"), "vpc_id");
        assert_eq!(escape_rust_keyword("type_name"), "type_name");
        assert_eq!(escape_rust_keyword("cidr_block"), "cidr_block");
    }

    #[test]
    fn scan_manual_methods_finds_async_and_plain_fn_definitions() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let services_dir = tmp.path();

        // A service file with async delete, async update, and a plain extract fn
        let ec2_dir = services_dir.join("ec2");
        std::fs::create_dir_all(&ec2_dir).unwrap();
        std::fs::write(
            ec2_dir.join("vpc.rs"),
            r#"impl AwsProvider {
    pub(crate) async fn delete_ec2_vpc(&self) -> Result<()> { Ok(()) }
    pub(crate) async fn update_ec2_vpc(&self) -> Result<()> { Ok(()) }
    pub(crate) fn extract_ec2_vpc_attributes(obj: &Vpc) -> Option<String> { None }
}
"#,
        )
        .unwrap();

        // A service file with only a read (no delete/update)
        std::fs::write(
            ec2_dir.join("subnet.rs"),
            "impl AwsProvider {\n    pub(crate) async fn read_ec2_subnet(&self) {}\n}\n",
        )
        .unwrap();

        let methods = scan_manual_methods(services_dir);
        assert!(methods.contains("delete_ec2_vpc"));
        assert!(methods.contains("update_ec2_vpc"));
        assert!(methods.contains("extract_ec2_vpc_attributes"));
        assert!(methods.contains("read_ec2_subnet"));
        assert!(!methods.contains("delete_ec2_subnet"));
    }

    #[test]
    fn scan_manual_methods_returns_empty_for_missing_dir() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let missing = tmp.path().join("does_not_exist");
        let methods = scan_manual_methods(&missing);
        assert!(methods.is_empty());
    }

    #[test]
    fn pascalize_enum_type_name_initialism_to_pascal() {
        // All-uppercase initialisms become Rust-RFC-430 PascalCase so the
        // DSL TypeName segment matches the convention used by every other
        // enum (e.g. `Acl`, not `ACL`).
        assert_eq!(pascalize_enum_type_name("BucketCannedACL", "ACL"), "Acl");
        assert_eq!(pascalize_enum_type_name("MFADelete", "MFA"), "Mfa");
        assert_eq!(pascalize_enum_type_name("VPC", "VPC"), "Vpc");
    }

    #[test]
    fn pascalize_enum_type_name_mixed_case_passes_through() {
        // Already-PascalCase names with mixed case (the common case for
        // AWS enums like `InstanceTenancy`) must not be touched.
        assert_eq!(
            pascalize_enum_type_name("InstanceTenancy", "InstanceTenancy"),
            "InstanceTenancy"
        );
        assert_eq!(
            pascalize_enum_type_name("VersioningStatus", "VersioningStatus"),
            "VersioningStatus"
        );
    }

    #[test]
    fn pascalize_enum_type_name_camel_case_first_letter_uppercased() {
        // camelCase field names like `logGroupClass` get their first
        // letter uppercased only — the rest of the casing is preserved.
        assert_eq!(
            pascalize_enum_type_name("LogGroupClass", "logGroupClass"),
            "LogGroupClass"
        );
    }

    #[test]
    fn pascalize_enum_type_name_empty_input_returns_empty() {
        assert_eq!(pascalize_enum_type_name("", ""), "");
    }
}
