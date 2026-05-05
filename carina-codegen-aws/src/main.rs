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
use carina_smithy::{ShapeKind, SmithyModel};
use clap::Parser;
use heck::ToSnakeCase;

use carina_codegen_aws::resource_defs::{self, ResourceDef};

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
}

/// Unified type override for resource-scoped property overrides.
/// Allows overriding string type, enum values, integer range, or integer enum
/// for a specific (resource_type, property_name) pair.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum TypeOverride {
    /// Override to a specific string type (e.g., "super::iam_role_arn()")
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

/// Information about an attribute to generate
#[derive(Debug, Clone)]
struct AttrInfo {
    /// Snake_case attribute name (e.g., "cidr_block")
    snake_name: String,
    /// PascalCase provider name (e.g., "CidrBlock")
    provider_name: String,
    /// Rust code for the attribute type
    type_code: String,
    /// Whether the field is required
    is_required: bool,
    /// Whether the field is create-only
    is_create_only: bool,
    /// Whether the field is read-only
    is_read_only: bool,
    /// Whether the field contributes to anonymous resource identity hashing
    is_identity: bool,
    /// Description from Smithy docs
    description: Option<String>,
    /// Enum info if this attribute is an enum
    enum_info: Option<EnumInfo>,
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

    std::fs::create_dir_all(&args.output_dir)?;

    // Collect all resource definitions
    let mut all_resources = resource_defs::ec2_resources();
    all_resources.extend(resource_defs::s3_resources());
    all_resources.extend(resource_defs::sts_resources());
    all_resources.extend(resource_defs::organizations_resources());
    all_resources.extend(resource_defs::route53_resources());
    all_resources.extend(resource_defs::iam_resources());
    all_resources.extend(resource_defs::logs_resources());

    // Collect all data source definitions
    let mut all_data_sources = resource_defs::sts_data_sources();
    all_data_sources.extend(resource_defs::identitystore_data_sources());
    all_data_sources.extend(resource_defs::s3_data_sources());

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
                    dsl_name: res.name.to_string(),
                    service: service.to_string(),
                    file_stem: resource_snake.clone(),
                    config_fn: format!("{}_config", module_name(res.name)),
                    is_data_source: false,
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
                    dsl_name: ds.name.to_string(),
                    service: service.to_string(),
                    file_stem,
                    config_fn: format!("{}{}_config", module_name(ds.name), suffix),
                    is_data_source: true,
                });
            }

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
        }
        "markdown" | "md" => {
            let managed_md_names: std::collections::HashSet<&str> =
                resources.iter().map(|r| r.name).collect();
            for res in &resources {
                let model = models.get(res.service_namespace).unwrap();
                let md = generate_markdown_resource(res, model)?;

                let (service, resource) = split_service_resource(res.name);
                let resource = pascal_to_snake(resource);
                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;
                let output_path = service_dir.join(format!("{}.md", resource));
                std::fs::write(&output_path, &md)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                eprintln!("Generated: {}", output_path.display());
            }
            for ds in &data_sources {
                let model = models.get(ds.service_namespace).unwrap();
                let md = generate_markdown_data_source(ds, model)?;

                let (service, resource) = split_service_resource(ds.name);
                let resource = pascal_to_snake(resource);
                let service_dir = args.output_dir.join(service);
                std::fs::create_dir_all(&service_dir)?;
                let suffix = if managed_md_names.contains(ds.name) {
                    "_data_source"
                } else {
                    ""
                };
                let output_path = service_dir.join(format!("{}{}.md", resource, suffix));
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

/// Generate Rust schema code for a single resource.
fn generate_resource(res: &ResourceDef, model: &SmithyModel) -> Result<String> {
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

    // Build extra read-only set
    let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();

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
            if name == "Tags" {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    } else if let Some(create_input) = &create_input {
        for (name, member_ref) in &create_input.members {
            if exclude.contains(name.as_str()) {
                continue;
            }
            if name == "Tags" {
                continue; // handled separately
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
            if exclude.contains(name.as_str()) || name == "Tags" || name == res.identifier {
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
            if exclude.contains(name.as_str()) || name == "Tags" || name == res.identifier {
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

    // Add extra writable fields from read structure
    for extra in &res.extra_writable {
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
            if name == "Tags" {
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

    // Build extra_writable description override map
    let extra_writable_descs: HashMap<&str, Option<&str>> = res
        .extra_writable
        .iter()
        .map(|e| (e.name, e.description))
        .collect();
    // Build attribute list
    let mut attrs: Vec<AttrInfo> = Vec::new();

    // Process writable fields
    for (name, member_ref) in &writable_fields {
        let snake_name = name.to_snake_case();
        let is_required = (SmithyModel::is_required(member_ref)
            || required_overrides.contains(name.as_str()))
            && !read_only_overrides.contains(name.as_str());
        let is_read_only = read_only_overrides.contains(name.as_str());
        // `extra_writable` fields with `read_source = None` are synthetic:
        // the codegen has no Smithy member to ground them in. They default
        // to create-only — UNLESS they appear in `update_ops` (typically as
        // `FieldLayout::InsideStruct` members, e.g. PutPublicAccessBlock's
        // `PublicAccessBlockConfiguration` sub-fields), in which case they
        // are updatable just like any other writable field.
        let is_synthetic_extra_writable = res
            .extra_writable
            .iter()
            .any(|e| e.name == name.as_str() && e.read_source.is_none());
        let is_create_only = if is_read_only {
            false
        } else if is_synthetic_extra_writable {
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
        let description = if let Some(Some(desc)) = extra_writable_descs.get(name.as_str()) {
            Some(desc.to_string())
        } else {
            SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string())
        };

        let (type_code, enum_info) = resolve_type(
            &mut TypeResolutionContext {
                model,
                namespace: &namespace,
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
            },
            &member_ref.target,
            name,
        );

        let is_identity = identity_overrides.contains(name.as_str());

        attrs.push(AttrInfo {
            snake_name,
            provider_name: name.clone(),
            type_code,
            is_required,
            is_create_only,
            is_read_only,
            is_identity,
            description,
            enum_info,
        });
    }

    // Process synthetic extra writable fields (no read_source).
    // A synthetic field defaults to create-only, but is treated as updatable
    // when it appears in `update_ops` (typically as a `FieldLayout::InsideStruct`
    // member of a wrapper struct in the API request shape).
    for extra in &res.extra_writable {
        if extra.read_source.is_some() {
            continue; // Already handled via writable_fields
        }
        let snake_name = extra.name.to_snake_case();
        let type_code = if let Some(&override_type) = type_overrides.get(extra.name) {
            override_type.to_string()
        } else if let Some(inferred) = infer_string_type(extra.name) {
            inferred
        } else {
            "AttributeType::String".to_string()
        };
        let is_create_only =
            !updatable_fields.contains(extra.name) || create_only_overrides.contains(extra.name);
        let is_required = required_overrides.contains(extra.name);
        attrs.push(AttrInfo {
            snake_name,
            provider_name: extra.name.to_string(),
            type_code,
            is_required,
            is_create_only,
            is_read_only: false,
            is_identity: identity_overrides.contains(extra.name),
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
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
            },
            &member_ref.target,
            name,
        );

        attrs.push(AttrInfo {
            snake_name,
            provider_name: name.clone(),
            type_code,
            is_required: false,
            is_create_only: false,
            is_read_only: true,
            is_identity: false,
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

    // Determine needed imports
    let has_ranged_ints = !all_ranged_ints.is_empty();
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
    if has_ranged_ints {
        schema_imports.push("legacy_validator");
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
        code.push_str("use super::validate_tags_map;\n");
    }
    if has_ranged_ints {
        code.push_str("use carina_core::resource::Value;\n");
    }
    code.push_str(&format!(
        "use carina_core::schema::{{{}}};\n\n",
        schema_imports_str
    ));

    // Generate enum constants.
    for (prop_name, enum_info) in &all_enums {
        let const_name = format!("VALID_{}", prop_name.to_snake_case().to_uppercase());

        // Generate constant
        let mut all_values: Vec<String> = enum_info
            .values
            .iter()
            .map(|v| format!("\"{}\"", v))
            .collect();
        // Add alias values (avoiding duplicates)
        let snake = prop_name.to_snake_case();
        if let Some(aliases) = enum_alias_map.get(snake.as_str()) {
            for (_, alias) in aliases {
                let formatted = format!("\"{}\"", alias);
                if !all_values.contains(&formatted) {
                    all_values.push(formatted);
                }
            }
        }
        let values_str = all_values.join(", ");
        code.push_str(&format!(
            "const {}: &[&str] = &[{}];\n\n",
            const_name, values_str
        ));
    }

    // Generate range validation functions
    for (prop_name, range) in &all_ranged_ints {
        let fn_name = format!("validate_{}_range", prop_name.to_snake_case());
        let (condition, display) = int_range_condition_and_display(range.min, range.max);
        code.push_str(&format!(
            "fn {}(value: &Value) -> Result<(), String> {{\n\
             \x20   if let Value::Int(n) = value {{\n\
             \x20       if {} {{\n\
             \x20           Err(format!(\"Value {{}} is out of range {}\", n))\n\
             \x20       }} else {{\n\
             \x20           Ok(())\n\
             \x20       }}\n\
             \x20   }} else {{\n\
             \x20       Err(\"Expected integer\".to_string())\n\
             \x20   }}\n\
             }}\n\n",
            fn_name, condition, display
        ));
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

    // Description from read structure (or create input for multi-op resources)
    let desc_traits = if let Some(read_struct) = read_structure {
        Some(&read_struct.traits)
    } else {
        create_input.as_ref().map(|ci| &ci.traits)
    };
    if let Some(traits) = desc_traits
        && let Some(desc) = SmithyModel::documentation(traits)
    {
        let escaped = escape_description(desc);
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

    // Generate attributes
    for attr in &attrs {
        let type_code = if let Some(ref ei) = attr.enum_info {
            // Use shared schema enum type for constrained strings.
            let to_dsl_code =
                if let Some(override_code) = to_dsl_overrides.get(attr.snake_name.as_str()) {
                    override_code.to_string()
                } else {
                    let has_hyphens = ei.values.iter().any(|v| v.contains('-'));
                    let snake = attr.provider_name.to_snake_case();
                    if let Some(aliases) = enum_alias_map.get(snake.as_str()) {
                        let mut match_arms: Vec<String> = aliases
                            .iter()
                            .map(|(canonical, alias)| {
                                format!("\"{}\" => \"{}\".to_string()", canonical, alias)
                            })
                            .collect();
                        let fallback = if has_hyphens {
                            "_ => s.replace('-', \"_\")"
                        } else {
                            "_ => s.to_string()"
                        };
                        match_arms.push(fallback.to_string());
                        format!("Some(|s: &str| match s {{ {} }})", match_arms.join(", "))
                    } else if has_hyphens {
                        "Some(|s: &str| s.replace('-', \"_\"))".to_string()
                    } else {
                        "None".to_string()
                    }
                };
            let values_str = ei
                .values
                .iter()
                .map(|v| format!("\"{}\".to_string()", v))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "AttributeType::StringEnum {{\n\
                 \x20               name: \"{}\".to_string(),\n\
                 \x20               values: vec![{}],\n\
                 \x20               namespace: Some(\"{}\".to_string()),\n\
                 \x20               to_dsl: {},\n\
                 \x20           }}",
                ei.type_name, values_str, namespace, to_dsl_code
            )
        } else {
            attr.type_code.clone()
        };

        let mut attr_code = format!(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::new(\"{}\", {})",
            attr.snake_name, type_code
        );

        if attr.is_required {
            attr_code.push_str("\n\x20               .required()");
        }
        if attr.is_create_only {
            attr_code.push_str("\n\x20               .create_only()");
        }
        if attr.is_identity {
            attr_code.push_str("\n\x20               .identity()");
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

        attr_code.push_str(",\n\x20       )\n");
        code.push_str(&attr_code);
    }

    // Tags attribute
    if res.has_tags {
        code.push_str(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::new(\"tags\", tags_type())\n\
             \x20               .with_description(\"The tags for the resource.\")\n\
             \x20               .with_provider_name(\"Tags\"),\n\
             \x20       )\n",
        );
    }

    // Tags validator
    if res.has_tags {
        code.push_str("\x20       .with_validator(validate_tags_map)\n");
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

    // Collect all alias entries: explicit aliases + to_dsl reverse mappings.
    let mut alias_entries: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    // Add explicit aliases from resource definition
    for (attr, alias, canonical) in &res.enum_aliases {
        if seen.insert((attr.to_string(), alias.to_string())) {
            alias_entries.push((attr.to_string(), alias.to_string(), canonical.to_string()));
        }
    }

    // Add to_dsl reverse mappings for values containing hyphens.
    for (prop_name, enum_info) in &all_enums {
        let attr_name = prop_name.to_snake_case();
        for value in &enum_info.values {
            if value.contains('-') {
                let dsl_form = value.replace('-', "_");
                if dsl_form != *value && seen.insert((attr_name.clone(), dsl_form.clone())) {
                    alias_entries.push((attr_name.clone(), dsl_form, value.clone()));
                }
            }
        }
    }

    // Generate enum_alias_reverse()
    code.push_str(
        "\n/// Maps DSL alias values back to canonical AWS values for this module.\n\
         /// e.g., (\"ip_protocol\", \"all\") -> Some(\"-1\")\n\
         pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {\n",
    );
    if alias_entries.is_empty() {
        code.push_str("    let _ = (attr_name, value);\n    None\n");
    } else {
        let match_arms: Vec<String> = alias_entries
            .iter()
            .map(|(attr, alias, canonical)| {
                format!(
                    "        (\"{}\", \"{}\") => Some(\"{}\")",
                    attr, alias, canonical
                )
            })
            .collect();
        code.push_str(&format!(
            "    match (attr_name, value) {{\n{},\n        _ => None\n    }}\n",
            match_arms.join(",\n")
        ));
    }
    code.push_str("}\n");

    // Generate enum_alias_entries()
    code.push_str(
        "\n/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.\n\
         pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {\n",
    );
    if alias_entries.is_empty() {
        code.push_str("    &[]\n");
    } else {
        let entry_strs: Vec<String> = alias_entries
            .iter()
            .map(|(attr, alias, canonical)| {
                format!("        (\"{}\", \"{}\", \"{}\")", attr, alias, canonical)
            })
            .collect();
        code.push_str(&format!("    &[\n{}\n    ]\n", entry_strs.join(",\n")));
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
    namespace: &'a str,
    type_overrides: &'a HashMap<&'a str, &'a str>,
    enum_alias_map: &'a HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    to_dsl_overrides: &'a HashMap<&'a str, &'a str>,
    all_enums: &'a mut BTreeMap<String, EnumInfo>,
    all_ranged_ints: &'a mut BTreeMap<String, IntRange>,
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

            ("AttributeType::String".to_string(), None)
        }
        Some(ShapeKind::Boolean) => ("AttributeType::Bool".to_string(), None),
        Some(ShapeKind::Integer) | Some(ShapeKind::Long) => {
            // Check for range traits on the target shape
            let range = get_int_range(ctx.model, target, field_name);
            if let Some(r) = range {
                ctx.all_ranged_ints
                    .entry(field_name.to_string())
                    .or_insert(r);
                let validate_fn = format!("validate_{}_range", field_name.to_snake_case());
                let length_expr = match (r.min, r.max) {
                    (Some(min), Some(max)) if min >= 0 && max >= 0 => {
                        format!("Some((Some({}), Some({})))", min, max)
                    }
                    (Some(min), None) if min >= 0 => format!("Some((Some({}), None))", min),
                    (None, Some(max)) if max >= 0 => format!("Some((None, Some({})))", max),
                    _ => "None".to_string(),
                };
                (
                    format!(
                        "AttributeType::Custom {{\n\
                         \x20               semantic_name: None,\n\
                         \x20               pattern: None,\n\
                         \x20               length: {},\n\
                         \x20               base: Box::new(AttributeType::Int),\n\
                         \x20               validate: legacy_validator({}),\n\
                         \x20               namespace: None,\n\
                         \x20               to_dsl: None,\n\
                         \x20           }}",
                        length_expr, validate_fn
                    ),
                    None,
                )
            } else {
                ("AttributeType::Int".to_string(), None)
            }
        }
        Some(ShapeKind::Float) | Some(ShapeKind::Double) => {
            ("AttributeType::Float".to_string(), None)
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
            ("AttributeType::String".to_string(), None)
        }
        Some(ShapeKind::IntEnum) => ("AttributeType::Int".to_string(), None),
        Some(ShapeKind::List) => {
            // Get list member type
            if let Some(carina_smithy::Shape::List(list_shape)) = ctx.model.get_shape(target) {
                let (item_type, _) = resolve_type(ctx, &list_shape.member.target, field_name);
                (format!("AttributeType::list({})", item_type), None)
            } else {
                (
                    "AttributeType::list(AttributeType::String)".to_string(),
                    None,
                )
            }
        }
        Some(ShapeKind::Map) => (
            "AttributeType::map(AttributeType::String)".to_string(),
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
                return ("AttributeType::Bool".to_string(), None);
            }

            // Generate struct type for nested structures
            if let Some(structure) = ctx.model.get_structure(target) {
                let struct_code = generate_struct_type(ctx, shape_name, structure);
                return (struct_code, None);
            }
            ("AttributeType::String".to_string(), None)
        }
        _ => {
            // Fallback: try name-based heuristics
            if let Some(inferred) = infer_string_type(field_name) {
                (inferred, None)
            } else {
                ("AttributeType::String".to_string(), None)
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
    let mut fields: Vec<String> = Vec::new();
    for (field_name, member_ref) in &structure.members {
        let snake_name = field_name.to_snake_case();
        let is_required = SmithyModel::is_required(member_ref);

        let (field_type, enum_info) = resolve_type(ctx, &member_ref.target, field_name);

        // If enum detected, use shared schema enum type.
        let field_type = if let Some(ei) = enum_info {
            let to_dsl_code =
                if let Some(override_code) = ctx.to_dsl_overrides.get(snake_name.as_str()) {
                    override_code.to_string()
                } else {
                    let has_hyphens = ei.values.iter().any(|v| v.contains('-'));
                    if let Some(aliases) = ctx.enum_alias_map.get(snake_name.as_str()) {
                        let mut match_arms: Vec<String> = aliases
                            .iter()
                            .map(|(canonical, alias)| {
                                format!("\"{}\" => \"{}\".to_string()", canonical, alias)
                            })
                            .collect();
                        let fallback = if has_hyphens {
                            "_ => s.replace('-', \"_\")"
                        } else {
                            "_ => s.to_string()"
                        };
                        match_arms.push(fallback.to_string());
                        format!("Some(|s: &str| match s {{ {} }})", match_arms.join(", "))
                    } else if has_hyphens {
                        "Some(|s: &str| s.replace('-', \"_\"))".to_string()
                    } else {
                        "None".to_string()
                    }
                };
            let values_str = ei
                .values
                .iter()
                .map(|v| format!("\"{}\".to_string()", v))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "AttributeType::StringEnum {{\n\
                 \x20               name: \"{}\".to_string(),\n\
                 \x20               values: vec![{}],\n\
                 \x20               namespace: Some(\"{}\".to_string()),\n\
                 \x20               to_dsl: {},\n\
                 \x20           }}",
                ei.type_name, values_str, ctx.namespace, to_dsl_code
            )
        } else {
            field_type
        };

        let mut field_code = format!("StructField::new(\"{}\", {})", snake_name, field_type);
        if is_required {
            field_code.push_str(".required()");
        }
        if let Some(desc) = SmithyModel::documentation(&member_ref.traits) {
            let escaped = escape_description(desc);
            let truncated = truncate_str(&escaped, 150);
            field_code.push_str(&format!(".with_description(\"{}\")", truncated));
        }
        field_code.push_str(&format!(".with_provider_name(\"{}\")", field_name));
        fields.push(field_code);
    }

    let fields_str = fields.join(",\n                    ");
    format!(
        "AttributeType::Struct {{\n\
         \x20                   name: \"{}\".to_string(),\n\
         \x20                   fields: vec![\n\
         \x20                   {}\n\
         \x20                   ],\n\
         \x20               }}",
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
    /// DSL resource name (e.g. "s3.Bucket"). Same value for the Managed and
    /// DataSource entries when a type is dual-registered.
    dsl_name: String,
    /// Service segment (e.g. "s3").
    service: String,
    /// File stem inside the service directory: "bucket" for the Managed
    /// resource, "bucket_data_source" for the DataSource sibling when both
    /// register under the same DSL name.
    file_stem: String,
    /// `<file_stem>::<config_fn>()` is the entry point for `configs()`.
    config_fn: String,
    is_data_source: bool,
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
         // Re-export all types and validators from types so that\n\
         // generated schema files can use `super::` to access them.\n\
         pub use super::types::*;\n\n",
    );

    // Scan for orphaned modules (files on disk not in resource_defs.rs) so we
    // preserve legacy schemas across codegen runs.
    let orphaned = scan_orphaned_modules(output_dir, modules);

    // Build a unified entry list. Sort key is (service, file_stem) so the
    // emitted text is stable across runs.
    #[derive(Debug, Clone)]
    struct Entry {
        dsl_name: String,
        service: String,
        file_stem: String,
        config_fn: String,
        is_data_source: bool,
    }
    let mut entries: Vec<Entry> = modules
        .iter()
        .map(|m| Entry {
            dsl_name: m.dsl_name.clone(),
            service: m.service.clone(),
            file_stem: m.file_stem.clone(),
            config_fn: m.config_fn.clone(),
            is_data_source: m.is_data_source,
        })
        .collect();
    for orphan in orphaned {
        // Orphan format is `"<service>.<PascalResource>"` (legacy spelling).
        let (service, resource) = split_service_resource(&orphan);
        let resource_snake = pascal_to_snake(resource);
        entries.push(Entry {
            dsl_name: orphan.clone(),
            service: service.to_string(),
            file_stem: resource_snake.clone(),
            config_fn: format!("{}_{}_config", service, resource_snake),
            is_data_source: false,
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

    // get_enum_alias_reverse() — only the Managed entry contributes; DataSource
    // entries share the DSL name and would emit a duplicate `if` arm whose
    // alias_reverse stub returns None anyway.
    code.push_str(
        "/// Maps DSL alias values back to canonical AWS values.\n\
         /// Dispatches to per-module enum_alias_reverse() functions.\n\
         pub fn get_enum_alias_reverse(resource_type: &str, attr_name: &str, value: &str) -> Option<&'static str> {\n",
    );
    for e in &entries {
        if e.is_data_source {
            continue;
        }
        code.push_str(&format!(
            "\x20   if resource_type == \"{}\" {{\n\
             \x20       return {}::{}::enum_alias_reverse(attr_name, value);\n\
             \x20   }}\n",
            e.dsl_name, e.service, e.file_stem
        ));
    }
    code.push_str("    None\n}\n\n");

    // build_enum_aliases_map() — same Managed-only filter as above.
    code.push_str(
        "/// Build a complete enum aliases map for all resource types.\n\
         /// Returns: resource_type -> attr_name -> alias -> canonical_value.\n\
         /// Used by CarinaProvider::enum_aliases() for the WASM host cache.\n\
         pub fn build_enum_aliases_map() -> std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>> {\n\
         \x20   let mut map = std::collections::HashMap::new();\n",
    );
    for e in &entries {
        if e.is_data_source {
            continue;
        }
        code.push_str(&format!(
            "\x20   for (attr, alias, canonical) in {}::{}::enum_alias_entries() {{\n\
             \x20       map.entry(\"{}\".to_string())\n\
             \x20           .or_insert_with(std::collections::HashMap::new)\n\
             \x20           .entry(attr.to_string())\n\
             \x20           .or_insert_with(std::collections::HashMap::new)\n\
             \x20           .insert(alias.to_string(), canonical.to_string());\n\
             \x20   }}\n",
            e.service, e.file_stem, e.dsl_name
        ));
    }
    code.push_str("    map\n}\n");

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
/// nested struct (used by `DerivedSource::Struct` and `StructAsMap`
/// emission). Returns `None` for shapes the emitter cannot handle.
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
        ShapeKind::String => format!("Value::String({}.to_string())", value_var),
        ShapeKind::Boolean => format!("Value::Bool({})", value_var),
        // SDK getters return `i32` for Integer, `i64` for Long. Cast only
        // when widening — clippy::unnecessary_cast catches the no-op
        // `i64 as i64` shape.
        ShapeKind::Integer => format!("Value::Int({} as i64)", value_var),
        ShapeKind::Long => format!("Value::Int({})", value_var),
        ShapeKind::Enum => format!("Value::String({}.as_str().to_string())", value_var),
        _ => return None,
    };
    Some(expr)
}

fn generate_provider_code(
    all_resources: &[ResourceDef],
    all_data_sources: &[resource_defs::DataSourceDef],
    models: &HashMap<&str, SmithyModel>,
    manual_methods: &std::collections::HashSet<String>,
) -> String {
    let mut code = String::new();

    // Header
    code.push_str(
        "//! Auto-generated provider boilerplate\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./carina-provider-aws/scripts/generate-provider.sh\n\n\
         use indexmap::IndexMap;\n\
         use std::collections::HashMap;\n\n\
         use carina_core::provider::{BoxFuture, ProviderError, ProviderResult};\n\
         use carina_core::resource::{Resource, ResourceId, State, Value};\n\
         #[allow(unused_imports)]\n\
         use carina_core::utils::extract_enum_value;\n\n\
         use crate::AwsProvider;\n\
         use crate::helpers::sdk_error_message;\n\n",
    );

    // Generate methods on AwsProvider
    code.push_str("// ===== Generated Methods on AwsProvider =====\n\n");
    // Some emitted helpers (e.g. `extract_<resource>_attributes` for
    // resources whose hand-written read paths build the attribute map
    // inline, or `update_<resource>` for noop-update resources whose
    // service file calls the underlying read method directly) have no
    // call sites in the current code. They are still emitted so the
    // generator can stay simple and uniform; gate dead-code on the impl
    // block instead of pruning case-by-case.
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
             \x20       identifier: &str,\n\
             \x20   ) -> ProviderResult<()> {{\n\
             \x20       self.{}.{}().{}(identifier).send().await.map_err(|e| {{\n\
             \x20           ProviderError::new(sdk_error_message(\"Failed to delete {}\", &e))\n\
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
                 \x20       identifier: &str,\n\
                 \x20       from: &State,\n\
                 \x20       to: Resource,\n\
                 \x20   ) -> ProviderResult<State> {{\n\
                 \x20       self.apply_ec2_tags(&id, identifier, &to.resolved_attributes(), Some(&from.attributes))\n\
                 \x20           .await?;\n\
                 \x20       self.{}(&id, Some(identifier)).await\n\
                 \x20   }}\n\n",
                res.name, method_name, read_method,
            ));
        } else {
            code.push_str(&format!(
                "\x20   /// Update {} (no-op, just read back current state) (generated)\n\
                 \x20   pub(crate) async fn {}(\n\
                 \x20       &self,\n\
                 \x20       id: ResourceId,\n\
                 \x20       identifier: &str,\n\
                 \x20       _to: Resource,\n\
                 \x20   ) -> ProviderResult<State> {{\n\
                 \x20       self.{}(&id, Some(identifier)).await\n\
                 \x20   }}\n\n",
                res.name, method_name, read_method,
            ));
        }
    }

    // Read helpers for read_ops (non-data-source resources only)
    for res in all_resources
        .iter()
        .filter(|r| !r.read_ops.is_empty() && !r.identifier.is_empty())
    {
        let model = match models.get(res.service_namespace) {
            Some(m) => m,
            None => continue,
        };
        let ns = res.service_namespace;
        let client_field = client_field_name(ns);
        let id_setter = res.identifier.to_snake_case();
        let resource_name = module_name(res.name);

        for read_op in &res.read_ops {
            let suffix = op_suffix(read_op.operation, res.identifier);
            let method_name = format!("read_{}_{}", resource_name, suffix);
            if manual_methods.contains(&method_name) {
                continue;
            }
            let sdk_method = read_op.operation.to_snake_case();

            // Resolve output structure
            let op_id = format!("{}#{}", ns, read_op.operation);
            let output = match model.operation_output(&op_id) {
                Some(o) => o,
                None => continue,
            };

            // Build defaults map
            let defaults: HashMap<&str, &str> = read_op.defaults.iter().copied().collect();

            // Method signature
            let op_desc = format!("{} {}", res.name, read_op.operation);
            code.push_str(&format!("\x20   /// Read {} (generated)\n", op_desc));
            code.push_str(&format!("\x20   pub(crate) async fn {}(\n", method_name));
            code.push_str("\x20       &self,\n");
            code.push_str("\x20       id: &ResourceId,\n");
            code.push_str("\x20       identifier: &str,\n");
            code.push_str("\x20       attributes: &mut HashMap<String, Value>,\n");
            code.push_str("\x20   ) -> ProviderResult<()> {\n");
            code.push_str(&format!(
                "\x20       let output = self.{}.{}().{}(identifier).send().await.map_err(|e| {{\n",
                client_field, sdk_method, id_setter
            ));
            code.push_str(&format!(
                "\x20           ProviderError::new(sdk_error_message(\"Failed to read {}\", &e))\n",
                op_desc
            ));
            code.push_str("\x20               .for_resource(id.clone())\n");
            code.push_str("\x20       })?;\n");

            // Extract each field
            for (field_name, rename) in &read_op.fields {
                let effective_name = rename.unwrap_or(field_name);
                let attr_snake = effective_name.to_snake_case();
                let accessor = escape_rust_keyword(&field_name.to_snake_case());

                // Determine if field is an enum
                let is_enum = if let Some(member_ref) = output.members.get(*field_name) {
                    matches!(model.shape_kind(&member_ref.target), Some(ShapeKind::Enum))
                } else {
                    false
                };

                let value_expr = if is_enum {
                    "v.as_str().to_string()"
                } else {
                    "v.to_string()"
                };

                if let Some(default_value) = defaults.get(effective_name) {
                    code.push_str(&format!(
                        "\x20       let value = output.{}().map(|v| {}).unwrap_or_else(|| \"{}\".to_string());\n",
                        accessor, value_expr, default_value,
                    ));
                    code.push_str(&format!(
                        "\x20       attributes.insert(\"{}\".to_string(), Value::String(value));\n",
                        attr_snake,
                    ));
                } else {
                    code.push_str(&format!(
                        "\x20       if let Some(v) = output.{}() {{\n",
                        accessor,
                    ));
                    code.push_str(&format!(
                        "\x20           attributes.insert(\"{}\".to_string(), Value::String({}));\n",
                        attr_snake, value_expr,
                    ));
                    code.push_str("\x20       }\n");
                }
            }

            code.push_str("\x20       Ok(())\n");
            code.push_str("\x20   }\n\n");
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
            code.push_str("\x20       attributes: &HashMap<String, Value>,\n");
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
                code.push_str(&format!(
                    "\x20       if let Some(Value::String(val)) = attributes.get(\"{}\") {{\n",
                    field.attr_snake
                ));
                if let Some(ref enum_type) = field.enum_type_name {
                    code.push_str("\x20           let normalized = extract_enum_value(val);\n");
                    code.push_str(&format!(
                        "\x20           builder = builder.{}({}::from(normalized));\n",
                        field.builder_setter, enum_type
                    ));
                } else {
                    code.push_str(&format!(
                        "\x20           builder = builder.{}(val.as_str());\n",
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
                "\x20               ProviderError::new(sdk_error_message(\"Failed to {}\", &e))\n",
                op_desc
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

        // Build exclude set
        let exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();

        // Get create input members
        let create_input = if !res.create_op.is_empty() {
            let create_op_id = format!("{}#{}", ns, res.create_op);
            model.operation_input(&create_op_id)
        } else {
            None
        };

        // Compute updatable field names
        let updatable_fields: HashSet<&str> = res
            .update_ops
            .iter()
            .flat_map(|op| op.fields.field_names().iter())
            .copied()
            .collect();

        let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();

        // Collect fields to extract: (attr_snake_name, accessor_snake_name, member_ref)
        let mut fields_to_extract: Vec<(String, String, &carina_smithy::ShapeRef)> = Vec::new();

        for (member_name, member_ref) in &read_struct.members {
            if exclude.contains(member_name.as_str()) || member_name == "Tags" {
                continue;
            }

            let is_schema_attr = member_name == res.identifier
                || extra_read_only.contains(member_name.as_str())
                || updatable_fields.contains(member_name.as_str())
                || create_input.is_some_and(|ci| ci.members.contains_key(member_name));

            if !is_schema_attr {
                continue;
            }

            let snake_name = member_name.to_snake_case();
            fields_to_extract.push((snake_name.clone(), snake_name, member_ref));
        }

        // Add extra_writable fields with read_source (different attr name vs accessor)
        for extra in &res.extra_writable {
            if let Some(read_source) = extra.read_source
                && let Some(member_ref) = read_struct.members.get(read_source)
            {
                let attr_name = extra.name.to_snake_case();
                let accessor_name = read_source.to_snake_case();
                // Avoid duplicates (if already extracted under the same accessor)
                if !fields_to_extract.iter().any(|(a, _, _)| a == &attr_name) {
                    fields_to_extract.push((attr_name, accessor_name, member_ref));
                }
            }
        }

        // Sort fields for deterministic output
        fields_to_extract.sort_by(|a, b| a.0.cmp(&b.0));

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
        code.push_str("\x20       attributes: &mut HashMap<String, Value>,\n");
        code.push_str("\x20   ) -> Option<String> {\n");

        for (attr_name, accessor_name, member_ref) in &fields_to_extract {
            let kind = model.shape_kind(&member_ref.target);
            let accessor = escape_rust_keyword(accessor_name);
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
                            "\x20       attributes.insert(\"{}\".to_string(), Value::String(obj.{}().as_str().to_string()));\n",
                            attr_name, accessor
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       if let Some(v) = obj.{}() {{\n",
                            accessor
                        ));
                        code.push_str(&format!(
                            "\x20           attributes.insert(\"{}\".to_string(), Value::String(v.as_str().to_string()));\n",
                            attr_name
                        ));
                        code.push_str("\x20       }\n");
                    }
                }
                Some(ShapeKind::Boolean) => {
                    if required {
                        code.push_str(&format!(
                            "\x20       attributes.insert(\"{}\".to_string(), Value::Bool(obj.{}()));\n",
                            attr_name, accessor
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       if let Some(v) = obj.{}() {{\n",
                            accessor
                        ));
                        code.push_str(&format!(
                            "\x20           attributes.insert(\"{}\".to_string(), Value::Bool(v));\n",
                            attr_name
                        ));
                        code.push_str("\x20       }\n");
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
                            "\x20       attributes.insert(\"{}\".to_string(), Value::Int(obj.{}(){}));\n",
                            attr_name, accessor, cast
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       if let Some(v) = obj.{}() {{\n",
                            accessor
                        ));
                        code.push_str(&format!(
                            "\x20           attributes.insert(\"{}\".to_string(), Value::Int(v{}));\n",
                            attr_name, cast
                        ));
                        code.push_str("\x20       }\n");
                    }
                }
                Some(ShapeKind::String) => {
                    if required {
                        // Required strings: skip empty-string sentinels the
                        // SDK uses for "missing on the wire," matching the
                        // pre-existing hand edits in provider_generated.rs.
                        code.push_str(&format!(
                            "\x20       let v = obj.{}();\n\
                             \x20       if !v.is_empty() {{\n\
                             \x20           attributes.insert(\"{}\".to_string(), Value::String(v.to_string()));\n\
                             \x20       }}\n",
                            accessor, attr_name
                        ));
                    } else {
                        code.push_str(&format!(
                            "\x20       if let Some(v) = obj.{}() {{\n",
                            accessor
                        ));
                        code.push_str(&format!(
                            "\x20           attributes.insert(\"{}\".to_string(), Value::String(v.to_string()));\n",
                            attr_name
                        ));
                        code.push_str("\x20       }\n");
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
                            "\x20       {{\n\
                             \x20           let ids = obj.{}();\n\
                             \x20           if !ids.is_empty() {{\n\
                             \x20               let list: Vec<Value> = ids.iter().map(|s| Value::String(s.to_string())).collect();\n\
                             \x20               attributes.insert(\"{}\".to_string(), Value::List(list));\n\
                             \x20           }}\n\
                             \x20       }}\n",
                            accessor, attr_name
                        ));
                    }
                    // Non-string-element lists need a declarative
                    // projection (DerivedSource::ListAll) — handled below.
                }
                _ => {
                    // Skip complex types (structures, maps) that need
                    // custom handling in hand-written code
                }
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
                        "\x20       if let Some(addr) = obj.{}().first()\n\
                         \x20           && let Some(v) = addr.{}()\n\
                         \x20       {{\n\
                         \x20           attributes.insert(\"{}\".to_string(), Value::String(v.to_string()));\n\
                         \x20       }}\n",
                        list_snake, child_snake, attr_snake
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
                        "\x20       {{\n\
                         \x20           let groups = obj.{}();\n\
                         \x20           if !groups.is_empty() {{\n\
                         \x20               let list: Vec<Value> = groups\n\
                         \x20                   .iter()\n\
                         \x20                   .filter_map(|g| g.{}().map(|id| Value::String(id.to_string())))\n\
                         \x20                   .collect();\n\
                         \x20               if !list.is_empty() {{\n\
                         \x20                   attributes.insert(\"{}\".to_string(), Value::List(list));\n\
                         \x20               }}\n\
                         \x20           }}\n\
                         \x20       }}\n",
                        list_snake, child_snake, attr_snake
                    ));
                }
                resource_defs::DerivedSource::Struct {
                    struct_member,
                    child_member,
                } => {
                    // Walk: read structure → struct member → inner struct
                    // → child member. The child's kind drives the value
                    // wrapping (String / Bool / Int / Long / Enum).
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
                    let Some(child_ref) = inner_struct.members.get(*child_member) else {
                        eprintln!(
                            "warning: derived_attributes for {}: child member '{}' not found on '{}'",
                            res.name, child_member, struct_ref.target
                        );
                        continue;
                    };
                    let attr_snake = derived.attr.to_snake_case();
                    let struct_snake = escape_rust_keyword(&struct_member.to_snake_case());
                    let child_snake = escape_rust_keyword(&child_member.to_snake_case());
                    let Some(insert_expr) = struct_child_value_expr(model, child_ref, "v") else {
                        eprintln!(
                            "warning: derived_attributes for {}: Struct child '{}' has unsupported shape; skipping",
                            res.name, child_member
                        );
                        continue;
                    };
                    code.push_str(&format!(
                        "\x20       if let Some(opts) = obj.{}()\n\
                         \x20           && let Some(v) = opts.{}()\n\
                         \x20       {{\n\
                         \x20           attributes.insert(\"{}\".to_string(), {});\n\
                         \x20       }}\n",
                        struct_snake, child_snake, attr_snake, insert_expr
                    ));
                }
                resource_defs::DerivedSource::StructAsMap {
                    struct_member,
                    children,
                } => {
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
                    let attr_snake = derived.attr.to_snake_case();
                    let struct_snake = escape_rust_keyword(&struct_member.to_snake_case());

                    code.push_str(&format!(
                        "\x20       if let Some(dns_opts) = obj.{}() {{\n\
                         \x20           let mut fields = IndexMap::new();\n",
                        struct_snake
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
                        let Some(insert_expr) = struct_child_value_expr(model, child_ref, "v")
                        else {
                            eprintln!(
                                "warning: derived_attributes for {}: StructAsMap child '{}' has unsupported shape; skipping",
                                res.name, child_name
                            );
                            continue;
                        };
                        code.push_str(&format!(
                            "\x20           if let Some(v) = dns_opts.{}() {{\n\
                             \x20               fields.insert(\"{}\".to_string(), {});\n\
                             \x20           }}\n",
                            child_snake, child_snake, insert_expr
                        ));
                    }
                    code.push_str(&format!(
                        "\x20           if !fields.is_empty() {{\n\
                         \x20               attributes.insert(\"{}\".to_string(), Value::Map(fields));\n\
                         \x20           }}\n\
                         \x20       }}\n",
                        attr_snake
                    ));
                }
                // DerivedSource is #[non_exhaustive]; future variants land in
                // later sub-issues and require this match to grow. Skip with
                // a loud message instead of silent ignore so the missing
                // emitter surfaces at codegen time.
                _ => eprintln!(
                    "warning: derived_attributes for {}: unhandled DerivedSource variant; teach the emitter when adding a new projection",
                    res.name
                ),
            }
        }

        // Return identifier value (only if identifier exists in read_structure)
        if !res.identifier.is_empty() && read_struct.members.contains_key(res.identifier) {
            let id_snake = escape_rust_keyword(&res.identifier.to_snake_case());
            let id_member = read_struct.members.get(res.identifier).unwrap();
            let id_required = SmithyModel::is_required(id_member);
            if id_required {
                code.push_str(&format!(
                    "\x20       Some(obj.{}().to_string())\n",
                    id_snake
                ));
            } else {
                code.push_str(&format!(
                    "\x20       obj.{}().map(String::from)\n",
                    id_snake
                ));
            }
        } else {
            code.push_str("\x20       None\n");
        }

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
             \x20       resource: &Resource,\n\
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
         \x20   resource: &'a Resource,\n\
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
         \x20               Err(ProviderError::new(format!(\n\
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

/// Generate markdown documentation for a single resource.
fn generate_markdown_resource(res: &ResourceDef, model: &SmithyModel) -> Result<String> {
    let ns = res.service_namespace;
    let namespace = format!("aws.{}", res.name);

    let is_data_source = res.create_op.is_empty();

    let exclude: HashSet<&str> = res.exclude_fields.iter().copied().collect();
    let type_overrides: HashMap<&str, &str> = res.type_overrides.iter().copied().collect();
    let required_overrides: HashSet<&str> = res.required_overrides.iter().copied().collect();
    let read_only_overrides: HashSet<&str> = res.read_only_overrides.iter().copied().collect();
    let extra_read_only: HashSet<&str> = res.extra_read_only.iter().copied().collect();
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
            if exclude.contains(name.as_str()) || name == "Tags" {
                continue;
            }
            writable_fields.insert(name.clone(), member_ref);
        }
    } else if let Some(create_input) = &create_input {
        for (name, member_ref) in &create_input.members {
            if exclude.contains(name.as_str()) || name == "Tags" {
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
            if exclude.contains(name.as_str()) || name == "Tags" || name == res.identifier {
                continue;
            }
            if !writable_fields.contains_key(name) && updatable_fields.contains(name.as_str()) {
                writable_fields.insert(name.clone(), member_ref);
            }
        }
    }

    // Add extra writable fields from read structure
    for extra in &res.extra_writable {
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
                || name == "Tags"
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

    // Collect enum info for documentation
    let mut all_enums: BTreeMap<String, EnumInfo> = BTreeMap::new();
    // Struct definitions for documentation
    let mut struct_defs: BTreeMap<String, Vec<(String, &carina_smithy::ShapeRef)>> =
        BTreeMap::new();

    // Build attr info for writable fields
    struct MdAttrInfo {
        snake_name: String,
        type_display: String,
        is_required: bool,
        description: Option<String>,
    }

    // Build extra_writable description override map
    let extra_writable_descs: HashMap<&str, Option<&str>> = res
        .extra_writable
        .iter()
        .map(|e| (e.name, e.description))
        .collect();

    let mut writable_attrs: Vec<MdAttrInfo> = Vec::new();
    for (name, member_ref) in &writable_fields {
        let snake_name = name.to_snake_case();
        let is_required = (SmithyModel::is_required(member_ref)
            || required_overrides.contains(name.as_str()))
            && !read_only_overrides.contains(name.as_str());
        let description = if let Some(Some(desc)) = extra_writable_descs.get(name.as_str()) {
            Some(desc.to_string())
        } else {
            SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string())
        };
        let type_display = type_display_string_md(
            model,
            &member_ref.target,
            name,
            &namespace,
            &type_overrides,
            &mut all_enums,
            &mut struct_defs,
        );

        writable_attrs.push(MdAttrInfo {
            snake_name,
            type_display,
            is_required,
            description,
        });
    }

    // Add synthetic extra writable fields (no read_source) to markdown
    for extra in &res.extra_writable {
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
        writable_attrs.push(MdAttrInfo {
            snake_name,
            type_display,
            is_required: false,
            description: extra.description.map(|s| s.to_string()),
        });
    }

    let mut read_only_attrs: Vec<MdAttrInfo> = Vec::new();
    for (name, member_ref) in &read_only_fields {
        let snake_name = name.to_snake_case();
        let description = SmithyModel::documentation(&member_ref.traits).map(|s| s.to_string());
        let type_display = type_display_string_md(
            model,
            &member_ref.target,
            name,
            &namespace,
            &type_overrides,
            &mut all_enums,
            &mut struct_defs,
        );

        read_only_attrs.push(MdAttrInfo {
            snake_name,
            type_display,
            is_required: false,
            description,
        });
    }

    // Build markdown output
    let mut md = String::new();

    // Title
    md.push_str(&format!("# aws.{}\n\n", res.name));
    md.push_str(&format!(
        "CloudFormation Type: `{}`\n\n",
        cf_type_name(res.name)
    ));

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
        md.push_str(&format!("{}\n\n", cleaned.trim()));
    }

    // Argument Reference (skip for data sources)
    if !is_data_source {
        md.push_str("## Argument Reference\n\n");
    }

    for attr in &writable_attrs {
        md.push_str(&format!("### `{}`\n\n", attr.snake_name));
        md.push_str(&format!("- **Type:** {}\n", attr.type_display));
        md.push_str(&format!(
            "- **Required:** {}\n",
            if attr.is_required { "Yes" } else { "No" }
        ));
        md.push('\n');

        if let Some(ref desc) = attr.description {
            let cleaned = collapse_whitespace(&strip_html_tags(desc).replace(['\n', '\t'], " "));
            md.push_str(&format!("{}\n\n", cleaned.trim()));
        }
    }

    // Tags
    if res.has_tags {
        md.push_str("### `tags`\n\n");
        md.push_str("- **Type:** Map\n");
        md.push_str("- **Required:** No\n\n");
        md.push_str("The tags for the resource.\n\n");
    }

    // Enum Values section
    if !all_enums.is_empty() {
        md.push_str("## Enum Values\n\n");
        for (prop_name, enum_info) in &all_enums {
            let attr_name = prop_name.to_snake_case();
            let has_hyphens = enum_info.values.iter().any(|v| v.contains('-'));
            let prop_aliases = enum_alias_map.get(attr_name.as_str());

            md.push_str(&format!("### {} ({})\n\n", attr_name, enum_info.type_name));
            md.push_str("| Value | DSL Identifier |\n");
            md.push_str("|-------|----------------|\n");

            for value in &enum_info.values {
                let dsl_value = if let Some(alias_list) = prop_aliases {
                    if let Some((_, alias)) = alias_list.iter().find(|(c, _)| *c == value.as_str())
                    {
                        alias.to_string()
                    } else if has_hyphens {
                        value.replace('-', "_")
                    } else {
                        value.clone()
                    }
                } else if has_hyphens {
                    value.replace('-', "_")
                } else {
                    value.clone()
                };
                let dsl_id = format!("{}.{}.{}", namespace, enum_info.type_name, dsl_value);
                md.push_str(&format!("| `{}` | `{}` |\n", value, dsl_id));
            }
            md.push('\n');

            let first_value = enum_info.values.first().map(|s| s.as_str()).unwrap_or("");
            let first_dsl = if let Some(alias_list) = prop_aliases {
                if let Some((_, alias)) = alias_list.iter().find(|(c, _)| *c == first_value) {
                    alias.to_string()
                } else if has_hyphens {
                    first_value.replace('-', "_")
                } else {
                    first_value.to_string()
                }
            } else if has_hyphens {
                first_value.replace('-', "_")
            } else {
                first_value.to_string()
            };
            md.push_str(&format!(
                "Shorthand formats: `{}` or `{}.{}`\n\n",
                first_dsl, enum_info.type_name, first_dsl,
            ));
        }
    }

    // Struct Definitions section
    if !struct_defs.is_empty() {
        md.push_str("## Struct Definitions\n\n");
        for (struct_name, fields) in &struct_defs {
            md.push_str(&format!("### {}\n\n", struct_name));
            md.push_str("| Field | Type | Required | Description |\n");
            md.push_str("|-------|------|----------|-------------|\n");
            for (field_name, member_ref) in fields {
                let snake_name = field_name.to_snake_case();
                let is_required = SmithyModel::is_required(member_ref);
                let field_type_display = type_display_string_md(
                    model,
                    &member_ref.target,
                    field_name,
                    &namespace,
                    &type_overrides,
                    &mut all_enums,
                    &mut BTreeMap::new(),
                );
                // Render the full description and escape pipe characters so the
                // markdown table doesn't break on them. Earlier versions
                // truncated at 100 chars, which lost important detail.
                let desc = SmithyModel::documentation(&member_ref.traits)
                    .map(|s| {
                        let cleaned =
                            collapse_whitespace(&strip_html_tags(s).replace(['\n', '\t'], " "));
                        cleaned.trim().replace('|', "\\|")
                    })
                    .unwrap_or_default();
                md.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    snake_name,
                    field_type_display,
                    if is_required { "Yes" } else { "No" },
                    desc
                ));
            }
            md.push('\n');
        }
    }

    // Attribute Reference (read-only)
    if !read_only_attrs.is_empty() {
        md.push_str("## Attribute Reference\n\n");
        for attr in &read_only_attrs {
            md.push_str(&format!("### `{}`\n\n", attr.snake_name));
            md.push_str(&format!("- **Type:** {}\n\n", attr.type_display));
        }
    }

    Ok(md)
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
            "AttributeType::String".to_string()
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
    for output in &ds.output_attributes {
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
    let schema_imports_str = schema_imports.join(", ");

    let mut code = String::new();
    code.push_str(&format!(
        "//! {} schema definition for AWS Cloud Control\n\
         //!\n\
         //! Auto-generated from Smithy model: {}\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with smithy-codegen\n\n\
         use super::AwsSchemaConfig;\n\
         use carina_core::schema::{{{}}};\n\n\
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
        if attr.read_only {
            code.push_str(&format!(
                "\x20       .attribute(\n\
                 \x20           AttributeSchema::new(\"{}\", {})\n\
                 \x20               .with_description(\"{}\")\n\
                 \x20               .with_provider_name(\"{}\"),\n\
                 \x20       )\n",
                attr.name, attr.type_str, attr.description, attr.provider_name,
            ));
        } else {
            let required = if attr.required {
                "\n            .required()"
            } else {
                ""
            };
            code.push_str(&format!(
                "\x20       .attribute(\n\
                 \x20           AttributeSchema::new(\"{}\", {}){}\n\
                 \x20               .with_description(\"{}\")\n\
                 \x20               .with_provider_name(\"{}\"),\n\
                 \x20       )\n",
                attr.name, attr.type_str, required, attr.description, attr.provider_name,
            ));
        }
    }

    code.push_str("    }\n}\n\n");

    // Enum stubs
    code.push_str(&format!(
        "/// Returns the resource type name and all enum valid values for this module\n\
         pub fn enum_valid_values() -> (&'static str, &'static [(&'static str, &'static [&'static str])]) {{\n\
         \x20   (\"{}\", &[])\n\
         }}\n\n\
         /// Maps DSL alias values back to canonical AWS values for this module.\n\
         /// e.g., (\"ip_protocol\", \"all\") -> Some(\"-1\")\n\
         pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {{\n\
         \x20   let _ = (attr_name, value);\n\
         \x20   None\n\
         }}\n\n\
         /// Returns all enum alias entries as (attr_name, alias, canonical) tuples.\n\
         pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {{\n\
         \x20   &[]\n\
         }}\n",
        ds.name,
    ));

    Ok(code)
}

/// Generate markdown documentation for a data source.
fn generate_markdown_data_source(
    ds: &resource_defs::DataSourceDef,
    _model: &SmithyModel,
) -> Result<String> {
    let cf_type = cf_type_name(ds.name);

    let mut md = String::new();
    md.push_str(&format!("# aws.{}\n\n", ds.name));
    md.push_str(&format!("CloudFormation Type: `{}`\n\n", cf_type));
    md.push_str("This is a **data source** (read-only). Use with the `read` keyword.\n\n");

    // Lookup inputs section
    if !ds.inputs.is_empty() {
        md.push_str("## Lookup Inputs\n\n");
        for input in &ds.inputs {
            md.push_str(&format!("### `{}`\n\n", input.name));
            let required = if input.required { "Yes" } else { "No" };
            md.push_str(&format!("- **Required:** {}\n\n", required));
            if !input.description.is_empty() {
                md.push_str(&format!("{}\n\n", input.description));
            }
        }
    }

    // Output attributes section
    let input_names: HashSet<&str> = ds.inputs.iter().map(|i| i.name).collect();
    md.push_str("## Attributes\n\n");
    for output in &ds.output_attributes {
        // Skip echo-of-input outputs (already documented in the inputs section).
        if input_names.contains(output.name) {
            continue;
        }
        md.push_str(&format!("### `{}`\n\n", output.name));
        let type_display = type_code_to_display(output.type_code);
        md.push_str(&format!("- **Type:** {}\n", type_display));
        md.push_str("- **Read-only**\n\n");
        if !output.description.is_empty() {
            md.push_str(&format!("{}\n\n", output.description));
        }
    }

    Ok(md)
}

/// Determine the display string for a type in markdown docs.
#[allow(clippy::only_used_in_recursion)]
fn type_display_string_md<'a>(
    model: &'a SmithyModel,
    target: &str,
    field_name: &str,
    namespace: &str,
    type_overrides: &HashMap<&str, &str>,
    all_enums: &mut BTreeMap<String, EnumInfo>,
    struct_defs: &mut BTreeMap<String, Vec<(String, &'a carina_smithy::ShapeRef)>>,
) -> String {
    // Check type overrides
    if let Some(&override_type) = type_overrides.get(field_name) {
        return type_code_to_display(override_type);
    }

    // Check known enum overrides
    if let Some(values) = known_enum_overrides().get(field_name) {
        let type_name = field_name.to_string();
        let enum_info = EnumInfo {
            type_name: type_name.clone(),
            values: values.iter().map(|s| s.to_string()).collect(),
        };
        all_enums
            .entry(field_name.to_string())
            .or_insert_with(|| enum_info);
        return format!(
            "[Enum ({})](#{}-{})",
            type_name,
            field_name.to_snake_case(),
            type_name.to_lowercase()
        );
    }

    let kind = model.shape_kind(target);

    match kind {
        Some(ShapeKind::String) => {
            if let Some(inferred) = infer_string_type(field_name) {
                return type_code_to_display(&inferred);
            }
            "String".to_string()
        }
        Some(ShapeKind::Boolean) => "Bool".to_string(),
        Some(ShapeKind::Integer) | Some(ShapeKind::Long) => {
            let range = get_int_range(model, target, field_name);
            if let Some(r) = range {
                format!("Int({})", range_display_string(r.min, r.max))
            } else {
                "Int".to_string()
            }
        }
        Some(ShapeKind::Float) | Some(ShapeKind::Double) => "Float".to_string(),
        Some(ShapeKind::Enum) => {
            if let Some(values) = model.enum_values(target) {
                // Prefer the Smithy shape name (PascalCase, e.g. "LogGroupClass")
                // over the field name (often camelCase, e.g. "logGroupClass") so
                // the rendered enum heading reads naturally.
                let type_name =
                    pascalize_enum_type_name(SmithyModel::shape_name(target), field_name);
                let string_values: Vec<String> = values.into_iter().map(|(_, v)| v).collect();
                let enum_info = EnumInfo {
                    type_name: type_name.clone(),
                    values: string_values,
                };
                all_enums
                    .entry(field_name.to_string())
                    .or_insert_with(|| enum_info);
                format!(
                    "[Enum ({})](#{}-{})",
                    type_name,
                    field_name.to_snake_case(),
                    type_name.to_lowercase()
                )
            } else {
                "String".to_string()
            }
        }
        Some(ShapeKind::IntEnum) => "Int".to_string(),
        Some(ShapeKind::List) => {
            if let Some(carina_smithy::Shape::List(list_shape)) = model.get_shape(target) {
                let item_display = type_display_string_md(
                    model,
                    &list_shape.member.target,
                    field_name,
                    namespace,
                    type_overrides,
                    all_enums,
                    struct_defs,
                );
                format!("`List<{}>`", item_display)
            } else {
                "`List<String>`".to_string()
            }
        }
        Some(ShapeKind::Map) => "Map".to_string(),
        Some(ShapeKind::Structure) => {
            let shape_name = SmithyModel::shape_name(target);
            if shape_name == "TagList" || shape_name == "Tag" {
                return "Map".to_string();
            }
            if shape_name == "AttributeBooleanValue" {
                return "Bool".to_string();
            }
            if let Some(structure) = model.get_structure(target) {
                // Register struct definition for docs
                let fields: Vec<(String, &carina_smithy::ShapeRef)> = structure
                    .members
                    .iter()
                    .map(|(n, r)| (n.clone(), r))
                    .collect();
                struct_defs.entry(shape_name.to_string()).or_insert(fields);
                format!("[Struct({})](#{})", shape_name, shape_name.to_lowercase())
            } else {
                "String".to_string()
            }
        }
        _ => {
            if let Some(inferred) = infer_string_type(field_name) {
                type_code_to_display(&inferred)
            } else {
                "String".to_string()
            }
        }
    }
}

/// Convert a Rust type code string to a human-readable display name.
///
/// Display names use PascalCase consistently (e.g. `IamRoleArn`, not
/// `iam_role_arn`). Container types like `AttributeType::list(...)` are
/// rendered as `List<Inner>` so docs never leak Rust constructor syntax.
fn type_code_to_display(type_code: &str) -> String {
    // Container types: AttributeType::list(...) and AttributeType::map(...)
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
        "AttributeType::String" => "String".to_string(),
        "AttributeType::Bool" => "Bool".to_string(),
        "AttributeType::Int" => "Int".to_string(),
        s if s.contains("ipv4_cidr") => "Ipv4Cidr".to_string(),
        s if s.contains("ipv6_cidr") => "Ipv6Cidr".to_string(),
        s if s.contains("ipv4_address") => "Ipv4Address".to_string(),
        s if s.contains("ipv6_address") => "Ipv6Address".to_string(),
        s if s.contains("iam_role_arn") => "IamRoleArn".to_string(),
        s if s.contains("iam_role_id") => "IamRoleId".to_string(),
        s if s.contains("iam_policy_arn") => "IamPolicyArn".to_string(),
        s if s.contains("iam_policy_document") => "IamPolicyDocument".to_string(),
        s if s.contains("kms_key_arn") => "KmsKeyArn".to_string(),
        s if s.contains("kms_key_id") => "KmsKeyId".to_string(),
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
        m.insert("DeliverCrossAccountRole", "super::iam_role_arn()");
        m.insert("DeliverLogsPermissionArn", "super::iam_role_arn()");
        m.insert("PeerRoleArn", "super::iam_role_arn()");
        m.insert("PermissionsBoundary", "super::iam_policy_arn()");
        m.insert("ManagedPolicyArns", "super::iam_policy_arn()");
        m.insert("KmsKeyId", "super::kms_key_arn()");
        m.insert("KMSMasterKeyID", "super::kms_key_id()");
        m.insert("ReplicaKmsKeyID", "super::kms_key_id()");
        m.insert("KmsKeyArn", "super::kms_key_arn()");
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
        m.insert(
            "IpProtocol",
            vec!["tcp", "udp", "icmp", "icmpv6", "-1", "all"],
        );
        m.insert("HostnameType", vec!["ip-name", "resource-name"]);
        m
    });
    &OVERRIDES
}

/// Generate condition string and display string for integer range validation.
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
        "s3.BucketServerSideEncryptionConfiguration" => {
            "AWS::S3::BucketServerSideEncryptionConfiguration"
        }
        // No native CloudFormation type; synthesize for cf_type_name totality.
        "s3.BucketAcl" => "AWS::S3::BucketAcl",
        "s3.BucketOwnershipControls" => "AWS::S3::BucketOwnershipControls",
        "sts.CallerIdentity" => "AWS::STS::CallerIdentity",
        "organizations.Organization" => "AWS::Organizations::Organization",
        "organizations.Account" => "AWS::Organizations::Account",
        "route53.RecordSet" => "AWS::Route53::RecordSet",
        "iam.Role" => "AWS::IAM::Role",
        "logs.LogGroup" => "AWS::Logs::LogGroup",
        "identitystore.User" => "AWS::IdentityStore::User",
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
            generated.contains(r#"AttributeSchema::new("account_id", super::aws_account_id())"#),
            "account_id must use aws_account_id(): {generated}"
        );
        assert!(
            generated.contains(r#"AttributeSchema::new("arn", super::arn())"#),
            "arn must use arn(): {generated}"
        );
        assert!(
            generated.contains(r#"AttributeSchema::new("user_id", AttributeType::String)"#),
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

        assert!(
            generated
                .contains(r#"AttributeSchema::new("identity_store_id", AttributeType::String)"#)
        );
        assert!(
            generated.contains(".required()"),
            "identity_store_id is required"
        );
        assert!(
            generated.contains(r#"AttributeSchema::new("display_name", AttributeType::String)"#)
        );
        assert!(generated.contains(r#"AttributeSchema::new("emails", AttributeType::String)"#));
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

        let md = generate_markdown_data_source(&ds, &model).expect("md");

        assert!(md.contains("### `account_id`"), "{md}");
        assert!(md.contains("### `arn`"), "{md}");
        assert!(md.contains("### `user_id`"), "{md}");
    }

    #[test]
    fn generate_resource_uses_string_enum_for_namespaced_enums() {
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
            generated.contains("AttributeType::StringEnum {"),
            "enum-like strings should be emitted as StringEnum: {generated}"
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

        // No regression: optional fields keep the if-let-Some shape.
        assert!(
            code.contains("if let Some(v) = obj.description()"),
            "optional description must keep if-let-Some: {code}"
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
            code.contains("if let Some(addr) = obj.nat_gateway_addresses().first()"),
            "must walk nat_gateway_addresses().first(): {code}"
        );
        assert!(
            code.contains("addr.allocation_id()"),
            "must project allocation_id from the list element: {code}"
        );
        assert!(
            code.contains("attributes.insert(\"allocation_id\".to_string()"),
            "must insert allocation_id attribute: {code}"
        );
        // Regression guard: the regular pass must not also emit
        // `obj.allocation_id()` (NatGateway has no top-level allocation_id getter,
        // so that would be a compile error).
        assert!(
            !code.contains("if let Some(v) = obj.allocation_id()"),
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
            code.contains("if let Some(addr) = obj.attachments().first()"),
            "must walk attachments().first(): {code}"
        );
        assert!(
            code.contains("addr.vpc_id()"),
            "must project vpc_id from the list element: {code}"
        );
        assert!(
            code.contains("attributes.insert(\"vpc_id\".to_string()"),
            "must insert vpc_id attribute: {code}"
        );
        assert!(
            !code.contains("if let Some(v) = obj.vpc_id()"),
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
                "let list: Vec<Value> = ids.iter().map(|s| Value::String(s.to_string())).collect();"
            ),
            "must collect into Vec<Value>: {code}"
        );
        assert!(
            code.contains("attributes.insert(\"route_table_ids\".to_string(), Value::List(list));"),
            "must insert as Value::List: {code}"
        );

        assert!(
            code.contains("let ids = obj.subnet_ids();"),
            "must read .subnet_ids(): {code}"
        );
        assert!(
            code.contains("attributes.insert(\"subnet_ids\".to_string(), Value::List(list));"),
            "must insert subnet_ids as Value::List: {code}"
        );
    }

    #[test]
    fn extract_ec2_transit_gateway_attachment_emits_list_of_string_for_subnet_ids() {
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
            code.contains("attributes.insert(\"subnet_ids\".to_string(), Value::List(list));"),
            "must insert subnet_ids as Value::List: {code}"
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
            code.contains("let groups = obj.groups();"),
            "must read .groups() as the list source: {code}"
        );
        assert!(
            code.contains(".filter_map(|g| g.group_id().map(|id| Value::String(id.to_string())))"),
            "must filter_map child .group_id(): {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"security_group_ids\".to_string(), Value::List(list));"
            ),
            "must insert as security_group_ids Value::List: {code}"
        );
    }

    #[test]
    fn extract_ec2_transit_gateway_emits_struct_nested_options() {
        let res = resource_defs::ec2_resources()
            .into_iter()
            .find(|r| r.name == "ec2.TransitGateway")
            .expect("ec2.TransitGateway missing");
        let Some(code) = provider_code_for_single_resource(res, "ec2.json") else {
            return;
        };

        // Each child of obj.options() lands as its own top-level attribute.
        // Pin one numeric (amazon_side_asn) and one enum (dns_support) to
        // exercise both Int and Enum emission paths.
        assert!(
            code.contains("if let Some(opts) = obj.options()\n            && let Some(v) = opts.amazon_side_asn()"),
            "amazon_side_asn must walk obj.options(): {code}"
        );
        assert!(
            // amazon_side_asn is Smithy `Long`, so the SDK getter returns
            // i64 directly — no widening cast.
            code.contains("attributes.insert(\"amazon_side_asn\".to_string(), Value::Int(v));"),
            "amazon_side_asn must insert as Int: {code}"
        );
        assert!(
            code.contains(
                "if let Some(opts) = obj.options()\n            && let Some(v) = opts.dns_support()"
            ),
            "dns_support must walk obj.options(): {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"dns_support\".to_string(), Value::String(v.as_str().to_string()));"
            ),
            "dns_support must insert as enum-as-String: {code}"
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
            code.contains("if let Some(opts) = obj.requester_vpc_info()\n            && let Some(v) = opts.vpc_id()"),
            "requester_vpc_info.vpc_id must walk obj.requester_vpc_info(): {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"vpc_id\".to_string(), Value::String(v.to_string()));"
            ),
            "requester vpc_id inserts under \"vpc_id\": {code}"
        );

        // Accepter: child `vpc_id` is renamed to `peer_vpc_id` (the rename
        // is the whole reason this can't be a direct read-structure member
        // pull). Owner / region get the same rename treatment.
        assert!(
            code.contains("if let Some(opts) = obj.accepter_vpc_info()\n            && let Some(v) = opts.vpc_id()"),
            "accepter_vpc_info.vpc_id must walk obj.accepter_vpc_info(): {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"peer_vpc_id\".to_string(), Value::String(v.to_string()));"
            ),
            "accepter vpc_id inserts under \"peer_vpc_id\" (rename): {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"peer_owner_id\".to_string(), Value::String(v.to_string()));"
            ),
            "accepter owner_id inserts under \"peer_owner_id\": {code}"
        );
        assert!(
            code.contains(
                "attributes.insert(\"peer_region\".to_string(), Value::String(v.to_string()));"
            ),
            "accepter region inserts under \"peer_region\": {code}"
        );
    }

    #[test]
    fn extract_ec2_subnet_emits_struct_as_map_for_private_dns_options() {
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
            code.contains("if let Some(dns_opts) = obj.private_dns_name_options_on_launch()"),
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
            code.contains("fields.insert(\"hostname_type\".to_string(), Value::String(v.as_str().to_string()));"),
            "hostname_type inserts as enum-as-String into fields: {code}"
        );
        // enable_resource_name_dns_a_record → Bool
        assert!(
            code.contains("if let Some(v) = dns_opts.enable_resource_name_dns_a_record()"),
            "must walk inner enable_resource_name_dns_a_record: {code}"
        );
        assert!(
            code.contains(
                "fields.insert(\"enable_resource_name_dns_a_record\".to_string(), Value::Bool(v));"
            ),
            "enable_resource_name_dns_a_record inserts as Bool into fields: {code}"
        );
        // Empty-map guard
        assert!(
            code.contains("if !fields.is_empty()"),
            "must guard the outer insert on non-empty fields: {code}"
        );
        // Final insert under the DSL attr name
        assert!(
            code.contains("attributes.insert(\"private_dns_name_options_on_launch\".to_string(), Value::Map(fields));"),
            "must insert the collected map under \"private_dns_name_options_on_launch\": {code}"
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
            dsl_name: "ec2.Vpc".to_string(),
            service: "ec2".to_string(),
            file_stem: "vpc".to_string(),
            config_fn: "ec2_vpc_config".to_string(),
            is_data_source: false,
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
            dsl_name: "ec2.Vpc".to_string(),
            service: "ec2".to_string(),
            file_stem: "vpc".to_string(),
            config_fn: "ec2_vpc_config".to_string(),
            is_data_source: false,
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
            generated.contains("if resource_type == \"iam.Role\""),
            "orphaned iam.Role enum_alias_reverse should be included: {generated}"
        );
    }

    #[test]
    fn generate_mod_rs_registers_dual_kinds_for_s3_bucket() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let output_dir = tmp.path();
        let known = vec![
            GeneratedModule {
                dsl_name: "s3.Bucket".to_string(),
                service: "s3".to_string(),
                file_stem: "bucket".to_string(),
                config_fn: "s3_bucket_config".to_string(),
                is_data_source: false,
            },
            GeneratedModule {
                dsl_name: "s3.Bucket".to_string(),
                service: "s3".to_string(),
                file_stem: "bucket_data_source".to_string(),
                config_fn: "s3_bucket_data_source_config".to_string(),
                is_data_source: true,
            },
        ];
        let generated = generate_mod_rs(&known, output_dir);
        assert!(generated.contains("s3::bucket::s3_bucket_config()"));
        assert!(generated.contains("s3::bucket_data_source::s3_bucket_data_source_config()"));
        // Only one enum_alias_reverse arm for s3.Bucket (the Managed entry).
        let enum_arms: Vec<_> = generated
            .match_indices("if resource_type == \"s3.Bucket\"")
            .collect();
        assert_eq!(
            enum_arms.len(),
            1,
            "data source must not duplicate alias arm: {generated}"
        );
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
            !generated.contains("\"email\", AttributeType::String"),
            "organizations.account.email should NOT be AttributeType::String: {generated}"
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
}
