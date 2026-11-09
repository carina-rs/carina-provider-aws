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
use carina_core::schema::{
    AttributeType, Draft, FieldGroup, FieldGroupKind, ListElements, PatternMatch, PatternSpec,
    RawShape, TypeIdentity,
};
use carina_smithy::{Shape, ShapeKind, SmithyModel};
use clap::{Parser, ValueEnum};
use heck::{ToSnakeCase, ToUpperCamelCase};

use carina_aws_types::{AwsType, arn_regex, dsl_enum_value};
use carina_codegen_aws::description::Description;
use carina_codegen_aws::kms_references::{
    self, PathSegment, reference_type_code, validate_smithy_type,
};
use carina_codegen_aws::resource_defs::{self, ReplacementClass, ResourceDef};
use carina_docgen::example::{self, DocExample, ExampleKind};
use carina_docgen::{
    Bounds, Declaration, DeclarationKind, DocLink, DocgenError, Heading, HeadingPath,
    ListConstraints, MapConstraints, MarkdownTypePosition, PageBuilder, Resolved, ScalarDoc,
    TableCell, TypeDisplay, TypeDocHooks, TypeEdge, TypeRegistry, ValidationNote, list_display,
    map_display, render_attribute_type, render_field_groups, scalar_display,
};

mod tag_types;
use tag_types::{TagMapType, resource_tag_type};

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

    /// Output format: rust (default), provider, or markdown (for documentation)
    #[arg(long, value_enum, default_value = "rust")]
    format: OutputFormat,

    /// Directory containing generated-doc examples (`<service>_<page>/main.crn`); required for markdown.
    #[arg(long)]
    examples_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    #[value(alias = "md")]
    Markdown,
    Provider,
    Rust,
}

#[derive(Debug, PartialEq, Eq)]
enum OutputMode {
    Markdown { examples_dir: PathBuf },
    Provider,
    Rust,
}

#[derive(Debug)]
struct ResolvedArgs {
    model_dir: PathBuf,
    output_dir: PathBuf,
    resource: Option<String>,
    output_mode: OutputMode,
}

impl TryFrom<Args> for ResolvedArgs {
    type Error = anyhow::Error;

    fn try_from(args: Args) -> Result<Self> {
        let Args {
            model_dir,
            output_dir,
            resource,
            format,
            examples_dir,
        } = args;

        if let Some(examples_dir) = &examples_dir
            && !examples_dir.is_dir()
        {
            anyhow::bail!(
                "--examples-dir must be an existing directory: {}",
                examples_dir.display()
            );
        }

        let output_mode = match format {
            OutputFormat::Markdown => OutputMode::Markdown {
                examples_dir: examples_dir
                    .context("--examples-dir is required when --format is markdown")?,
            },
            OutputFormat::Provider => OutputMode::Provider,
            OutputFormat::Rust => OutputMode::Rust,
        };

        Ok(Self {
            model_dir,
            output_dir,
            resource,
            output_mode,
        })
    }
}

/// Resource-scoped value refinements and shared type identities.
#[derive(Debug, Clone, PartialEq)]
enum TypeOverride {
    /// Anonymous Int range; preserves the numeric carrier and wire packing.
    IntRange { min: i64, max: i64 },
    /// Anonymous String refinement; does not introduce an identifier identity.
    RefinedString {
        pattern: PatternSpec,
        min_length: u64,
        max_length: u64,
    },
    /// Anonymous String carrier with documented character bounds only.
    StringBounds { min: u64, max: u64 },
    /// Anonymous whole-string pattern with no documented numeric length cap.
    StringPattern(PatternSpec),
    /// Share an anonymous String constructor and derive documentation from it.
    AnonymousStringConstructor(&'static str),
    /// An established core scalar, including its existing validation identity.
    CoreType(&'static str),
    /// Refine a list without replacing its recursively resolved element type.
    ListLength { min: u64, max: u64 },
    /// Use the identity, validation, and allowed-value metadata of a shared row.
    SharedType(AwsType),
    /// Override only the enum identity, retaining Smithy values and DSL aliases.
    EnumIdentity(AwsType),
}

struct ValueTypeOverride {
    type_code: String,
    documentation: Option<String>,
    attribute_type: AttributeType,
}

/// Read the Smithy length trait for strings and collections through one path.
fn length_from_traits(traits: &carina_smithy::Traits) -> Bounds<u64> {
    traits.get("smithy.api#length").map(|length| {
        (
            length.get("min").and_then(|value| value.as_u64()),
            length.get("max").and_then(|value| value.as_u64()),
        )
    })
}

fn pattern_spec_code(pattern: &PatternSpec) -> String {
    let mode = match pattern.mode() {
        PatternMatch::Full => "carina_core::schema::PatternMatch::Full".to_string(),
        PatternMatch::Substring { reason } => format!(
            "carina_core::schema::PatternMatch::Substring {{ reason: carina_core::schema::PatternReason::new({:?}).expect(\"visible pattern reason\") }}",
            reason.as_str(),
        ),
    };
    format!(
        "carina_core::schema::PatternSpec::new({:?}, {mode})",
        pattern.as_str()
    )
}

/// Serialize core's portable declarations without introducing provider-specific
/// predicates or changing their scope, target, codes, or pattern semantics.
fn input_rule_code(rule: &carina_core::schema::InputRule) -> String {
    use carina_core::schema::{CharSet, InputPredicate, InputRuleScope, InputRuleTarget};

    let charset = |set: &CharSet| {
        format!(
            "carina_core::schema::CharSet::new({:?}).expect(\"valid input character set\")",
            set.ranges(),
        )
    };
    let token = |value: &carina_core::schema::NonEmptyString| {
        format!(
            "carina_core::schema::NonEmptyString::new({:?}).expect(\"nonempty input token\")",
            value.as_str(),
        )
    };
    let scope = match rule.scope() {
        InputRuleScope::CreateOnly => "CreateOnly",
        InputRuleScope::EveryWrite => "EveryWrite",
    };
    let target = match rule.target() {
        InputRuleTarget::Value(_) => "Value",
        InputRuleTarget::MapKeys(_) => "MapKeys",
    };
    let predicate = match rule.predicate() {
        InputPredicate::Length { min, max } => format!("Length {{ min: {min:?}, max: {max:?} }}"),
        InputPredicate::AllowedChars(set) => format!("AllowedChars({})", charset(set)),
        InputPredicate::FirstChar(set) => format!("FirstChar({})", charset(set)),
        InputPredicate::LastChar(set) => format!("LastChar({})", charset(set)),
        InputPredicate::NoSubstring(value) => format!("NoSubstring({})", token(value)),
        InputPredicate::NoPrefix(value) => format!("NoPrefix({})", token(value)),
        InputPredicate::ForbiddenPrefix {
            prefix,
            ignore_ascii_case,
        } => format!(
            "ForbiddenPrefix {{ prefix: {}, ignore_ascii_case: {ignore_ascii_case} }}",
            token(prefix),
        ),
        InputPredicate::NoSuffix(value) => format!("NoSuffix({})", token(value)),
        InputPredicate::NoAdjacent { left, right } => format!(
            "NoAdjacent {{ left: {}, right: {} }}",
            charset(left),
            charset(right),
        ),
        InputPredicate::NotIpv4 => "NotIpv4".to_string(),
        InputPredicate::Matches(pattern) => format!("Matches({})", pattern_spec_code(pattern)),
        InputPredicate::NotMatches(pattern) => {
            format!("NotMatches({})", pattern_spec_code(pattern))
        }
    };
    format!(
        "carina_core::schema::InputRule::new(\
         carina_core::schema::InputRuleScope::{scope}, \
         carina_core::schema::InputRuleTarget::{target}(\
         carina_core::schema::InputPath::new({:?}).expect(\"valid input path\")), \
         carina_core::schema::InputRuleCode::new({:?}).expect(\"valid input rule code\"), \
         carina_core::schema::InputPredicate::{predicate})",
        rule.path().as_str(),
        rule.code().as_str(),
    )
}

/// Anonymous strings share their Rust literal encoding and schema metadata.
fn string_refinement(pattern: Option<&PatternSpec>, length: Bounds<u64>) -> ValueTypeOverride {
    let pattern_code = pattern.map_or_else(
        || "None".to_string(),
        |pattern| format!("Some({})", pattern_spec_code(pattern)),
    );
    let type_code = if pattern.is_none() && length.is_none() {
        PLAIN_STRING_TYPE_CODE.to_string()
    } else {
        format!("AttributeType::refined_string(None, {pattern_code}, {length:?}, None)")
    };
    ValueTypeOverride {
        type_code,
        documentation: None,
        attribute_type: AttributeType::refined_string(None, pattern.cloned(), length, None),
    }
}

/// Ordinary maps and tag collections use the same constructor selection.
fn map_type_code(key: &str, value: &str, length: Bounds<u64>) -> String {
    if length.is_some() {
        format!("AttributeType::refined_map({key}, {value}, {length:?})")
    } else if key == PLAIN_STRING_TYPE_CODE {
        format!("AttributeType::map({value})")
    } else {
        format!("AttributeType::map_with_key({key}, {value})")
    }
}

/// Rust and documentation use the same exact resource/path lookup. Enum
/// identities retain their separate Smithy-shape lookup in enum_identity_override.
fn resource_value_override(resource: &str, property_path: &str) -> Option<ValueTypeOverride> {
    let (type_code, attribute_type, documentation) = match resource_type_overrides()
        .get(&(resource, property_path))?
    {
        TypeOverride::IntRange { min, max } => (
            format!("AttributeType::refined_int(None, Some((Some({min}), Some({max}))))"),
            AttributeType::refined_int(None, Some((Some(*min), Some(*max)))),
            None,
        ),
        TypeOverride::StringBounds { min, max } => {
            return Some(string_refinement(None, Some((Some(*min), Some(*max)))));
        }
        TypeOverride::RefinedString {
            pattern,
            min_length,
            max_length,
        } => {
            return Some(string_refinement(
                Some(pattern),
                Some((Some(*min_length), Some(*max_length))),
            ));
        }
        TypeOverride::StringPattern(pattern) => {
            return Some(string_refinement(Some(pattern), None));
        }
        TypeOverride::AnonymousStringConstructor(constructor)
        | TypeOverride::CoreType(constructor) => (
            constructor.to_string(),
            attribute_type_from_code(constructor).expect("registered type"),
            None,
        ),
        TypeOverride::ListLength { .. } | TypeOverride::EnumIdentity(_) => return None,
        TypeOverride::SharedType(row) => {
            // Supplemental AWS validation explanations are documentation metadata,
            // separate from the shared renderer's scalar and constraint wording.
            let mut notes = Vec::new();
            if let Some(values) = row.allowed_int_values() {
                notes.push(format!(
                    "allowed: {}",
                    values
                        .iter()
                        .map(i64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            let ec2_rule = match row {
                AwsType::Ec2AmazonSideAsn => Some("64512..=65534 or 4200000000..=4294967294"),
                AwsType::Ec2VpnGatewayAmazonSideAsn => Some(
                    "64512..=65534 or 4200000000..=4294967294; legacy Amazon ASNs: 7224, 9059, 10124, 17493",
                ),
                AwsType::Ec2VpcCidrBlock => {
                    Some("IPv4 /16..=/28; excludes 0/8, 127/8, 169.254/16, 224/4")
                }
                AwsType::Ec2SubnetIpv4CidrBlock => Some("IPv4 /16..=/28"),
                AwsType::Ec2SubnetIpv6CidrBlock => Some("IPv6 prefixes: 44, 48, 52, 56, 60, 64"),
                AwsType::Ec2TransitGatewayCidrBlock => {
                    Some("IPv4 /0..=/24 or IPv6 /0..=/64; excludes overlap with 169.254/16")
                }
                AwsType::Ec2SecurityGroupName => Some("must not start with sg-"),
                _ => None,
            };
            if let Some(rule) = ec2_rule {
                notes.push(rule.to_string());
            }
            if matches!(
                row,
                AwsType::AcmCertificateDomainName | AwsType::AcmCertificateSubjectAlternativeName
            ) {
                notes.push(carina_aws_types::ACM_CERTIFICATE_REQUEST_DOMAIN_RULE.to_string());
            }
            (
                row.constructor_path().to_string(),
                row.attribute_type(),
                (!notes.is_empty()).then(|| notes.join("; ")),
            )
        }
    };
    Some(ValueTypeOverride {
        type_code,
        documentation,
        attribute_type,
    })
}

/// Select synthetic extra types once for schemas and documentation, retaining
/// both the Rust constructor and its complete attribute type and validation note.
fn synthetic_extra_attribute_type(
    resource: &str,
    field_name: &str,
    type_overrides: &HashMap<&str, &str>,
) -> Result<ValueTypeOverride> {
    let snake_name = field_name.to_snake_case();
    let type_code = if let Some(code) = reference_type_code(resource, &snake_name) {
        code.to_string()
    } else if let Some(override_type) = resource_value_override(resource, &snake_name) {
        return Ok(override_type);
    } else if let Some(&override_type) = type_overrides.get(field_name) {
        override_type.to_string()
    } else if let Some(inferred) = infer_string_type(field_name) {
        inferred
    } else {
        PLAIN_STRING_TYPE_CODE.to_string()
    };
    Ok(ValueTypeOverride {
        attribute_type: attribute_type_from_code(&type_code)?,
        type_code,
        documentation: None,
    })
}

impl ValueTypeOverride {
    fn markdown(
        self,
        heading_path: &HeadingPath,
        namespace: &str,
        enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
        structs: &mut MarkdownTypes,
    ) -> Result<MarkdownType> {
        discover_markdown_attribute_type(
            &self.attribute_type,
            heading_path,
            namespace,
            enums,
            structs,
            self.documentation.as_deref().map(ValidationNote::new),
        )
    }
}

/// Preserve flattened string lists while refining their exact element path.
fn string_list_value_override(
    model: &SmithyModel,
    target: &str,
    resource: &str,
    property_path: &str,
    constructor: &str,
) -> Option<ValueTypeOverride> {
    let ordered = match constructor {
        "AttributeType::list(AttributeType::string())" => true,
        "AttributeType::unordered_list(AttributeType::string())" => false,
        _ => return None,
    };
    let element = resource_value_override(resource, &format!("{property_path}[]"))?;
    // The ordered Route 53 override flattens ResourceRecord{Value}; retain the
    // source list's min=1 instead of silently dropping it during flattening.
    // Leave the existing unordered-list contracts (ACM SANs) unchanged.
    if ordered {
        let length = if let Some(Shape::List(list)) = model.get_shape(target) {
            length_from_traits(&list.traits)
        } else {
            None
        };
        return Some(ValueTypeOverride {
            type_code: format!(
                "AttributeType::refined_list({}, true, {length:?}, carina_core::schema::ListElements::AllowDuplicates)",
                element.type_code
            ),
            documentation: element.documentation,
            attribute_type: AttributeType::refined_list(
                element.attribute_type,
                true,
                length,
                carina_core::schema::ListElements::AllowDuplicates,
            ),
        });
    }
    Some(ValueTypeOverride {
        type_code: format!("AttributeType::unordered_list({})", element.type_code),
        documentation: element.documentation,
        attribute_type: AttributeType::unordered_list(element.attribute_type),
    })
}

/// Information about a detected enum type
#[derive(Debug, Clone)]
struct EnumInfo {
    /// Type name in PascalCase (e.g., "InstanceTenancy")
    type_name: String,
    /// Valid enum values (e.g., ["default", "dedicated", "host"])
    values: Vec<String>,
    /// Shared catalog identity to use instead of the structural Smithy identity.
    identity_override: Option<AwsType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArnEmitChoice {
    PerKind(&'static ArnValidation),
    ServicePrefix(&'static str),
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArnValidation {
    service: &'static str,
    resource: &'static str,
    pattern: PatternSpec,
    #[cfg(test)]
    sample_arn_rest: &'static str,
}

static ARN_VALIDATIONS: LazyLock<[ArnValidation; 9]> = LazyLock::new(|| {
    [
        ArnValidation {
            service: "iam",
            resource: "Role",
            pattern: PatternSpec::new(arn_regex("iam::[^:]*:role/.+$"), PatternMatch::Full),
            #[cfg(test)]
            sample_arn_rest: "iam::123456789012:role/example-role",
        },
        ArnValidation {
            service: "iam",
            resource: "Policy",
            pattern: PatternSpec::new(arn_regex("iam::[^:]*:policy/.+$"), PatternMatch::Full),
            #[cfg(test)]
            sample_arn_rest: "iam::123456789012:policy/example-policy",
        },
        ArnValidation {
            service: "iam",
            resource: "OidcProvider",
            pattern: PatternSpec::new(
                arn_regex("iam::[^:]*:oidc-provider/.+$"),
                PatternMatch::Full,
            ),
            #[cfg(test)]
            sample_arn_rest: "iam::123456789012:oidc-provider/example.com",
        },
        ArnValidation {
            service: "kms",
            resource: "Key",
            pattern: PatternSpec::new(arn_regex("kms:[^:]*:[^:]*:key/.+$"), PatternMatch::Full),
            #[cfg(test)]
            sample_arn_rest: "kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789abc",
        },
        ArnValidation {
            service: "s3",
            resource: "Bucket",
            pattern: PatternSpec::new(arn_regex("s3:::.+$"), PatternMatch::Full),
            #[cfg(test)]
            sample_arn_rest: "s3:::example-bucket",
        },
        ArnValidation {
            service: "cloudfront",
            resource: "Distribution",
            pattern: PatternSpec::new(
                arn_regex("cloudfront::[^:]*:distribution/.+$"),
                PatternMatch::Full,
            ),
            #[cfg(test)]
            sample_arn_rest: "cloudfront::123456789012:distribution/EXAMPLE",
        },
        ArnValidation {
            service: "ecs",
            resource: "Cluster",
            pattern: PatternSpec::new(arn_regex("ecs:[^:]*:[^:]*:cluster/.+$"), PatternMatch::Full),
            #[cfg(test)]
            sample_arn_rest: "ecs:us-east-1:123456789012:cluster/example",
        },
        ArnValidation {
            service: "dynamodb",
            resource: "Table",
            pattern: PatternSpec::new(
                arn_regex("dynamodb:[^:]*:[^:]*:table/.+$"),
                PatternMatch::Full,
            ),
            #[cfg(test)]
            sample_arn_rest: "dynamodb:us-east-1:123456789012:table/example",
        },
        ArnValidation {
            service: "logs",
            resource: "LogGroup",
            pattern: PatternSpec::new(
                arn_regex("logs:[^:]*:[^:]*:log-group:.+$"),
                PatternMatch::Full,
            ),
            #[cfg(test)]
            sample_arn_rest: "logs:us-east-1:123456789012:log-group:example",
        },
    ]
});

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
        ArnEmitChoice::PerKind(entry) => format!("Some({})", pattern_spec_code(&entry.pattern)),
        ArnEmitChoice::ServicePrefix(service) => {
            format!(
                "Some({})",
                pattern_spec_code(&PatternSpec::new(
                    arn_regex(&format!("{service}:.*$")),
                    PatternMatch::Full
                ))
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
    /// Whether this writable attribute is part of the generated send input.
    is_sent: bool,
    /// Which create phase carries this value when it is sent.
    send_via: Option<resource_defs::SendVia>,
    /// Reason attached to a deliberate `NotSent` classification.
    send_reason: Option<String>,
    /// Rust value shape used by generated create/update inputs.
    send_type: SendFieldType,
    /// Wire integers are narrowed while decoding, before Sent guards exist.
    sdk_i32: bool,
    sdk_i32_paths: Vec<Vec<String>>,
    /// Whether the underlying Smithy member target is a plain string shape.
    /// This is captured before rendered type overrides are applied.
    is_plain_string: bool,
    /// Whether the field contributes to anonymous resource identity hashing
    is_identity: bool,
    /// Whether the AWS API populates this field asynchronously after Create
    /// (carina#3034). Validate-time check rejects chained references to
    /// such a field unless the user has declared a `wait` block.
    is_deferred_populate: bool,
    /// Description cleaned according to its Smithy or authored origin.
    description: Option<Description>,
    /// Enum info if this attribute is an enum
    enum_info: Option<EnumInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendFieldType {
    String,
    StringList,
    Tags,
    Enum,
    Bool,
    Int,
    Float,
    Value,
}

fn send_field_type(
    model: &SmithyModel,
    target: &str,
    type_code: &str,
    enum_info: Option<&EnumInfo>,
) -> SendFieldType {
    if type_code == "AttributeType::list(AttributeType::string())"
        || type_code == "AttributeType::unordered_list(AttributeType::string())"
        || type_code.starts_with("AttributeType::refined_list(AttributeType::refined_string(")
        || (matches!(model.get_shape(target), Some(Shape::List(list))
            if matches!(model.shape_kind(&list.member.target), Some(ShapeKind::String))))
    {
        return SendFieldType::StringList;
    }
    // Classify containers before looking for scalar helper names: generated
    // list/struct expressions can themselves contain an enum helper (for
    // example AvailabilityZoneAddresses contains availability_zone()).
    if type_code.contains("AttributeType::list(")
        || type_code.contains("AttributeType::unordered_list(")
        || type_code.contains("AttributeType::map(")
        || type_code.contains("AttributeType::struct_(")
        || type_code.contains("AttributeType::struct_with_field_groups(")
        || type_code.contains("AttributeType::union(")
        || type_code.contains("iam_policy_document()")
        || type_code.contains("sqs_redrive_policy()")
        || type_code.contains("sqs_redrive_allow_policy()")
        || type_code.contains("bucket_encryption_rules()")
        || type_code.contains("bucket_replication_rules()")
        || type_code.contains("bucket_lifecycle_rules()")
        || type_code.contains("bucket_object_lock_rule()")
        || type_code.contains("s3_index_document()")
        || type_code.contains("s3_error_document()")
        || type_code.contains("s3_redirect_all_requests_to()")
        || type_code.contains("bucket_cors_rules()")
        || type_code.contains("bucket_topic_configurations()")
        || type_code.contains("bucket_queue_configurations()")
        || type_code.contains("bucket_lambda_function_configurations()")
        || type_code.contains("bucket_event_bridge_configuration()")
        || type_code.contains("bucket_target_object_key_format()")
    {
        return SendFieldType::Value;
    }
    if enum_info.is_some()
        || attribute_type_from_code(type_code)
            .is_ok_and(|attr_type| matches!(attr_type.raw_shape(), RawShape::Enum { .. }))
        || type_code.contains("AttributeType::enum_(")
        || type_code.contains("aws_region()")
        || type_code.contains("availability_zone()")
        || type_code.contains("s3_object_lock_enabled()")
    {
        return SendFieldType::Enum;
    }
    if type_code == "AttributeType::bool()" {
        return SendFieldType::Bool;
    }
    if type_code == "AttributeType::int()" || type_code.starts_with("AttributeType::refined_int(") {
        return SendFieldType::Int;
    }
    if type_code == "AttributeType::float()" {
        return SendFieldType::Float;
    }
    match model.shape_kind(target) {
        Some(ShapeKind::String | ShapeKind::Timestamp) => SendFieldType::String,
        Some(ShapeKind::Boolean) => SendFieldType::Bool,
        Some(ShapeKind::Integer | ShapeKind::Long | ShapeKind::IntEnum) => SendFieldType::Int,
        Some(ShapeKind::Float | ShapeKind::Double) => SendFieldType::Float,
        Some(
            ShapeKind::Service
            | ShapeKind::Operation
            | ShapeKind::Structure
            | ShapeKind::Union
            | ShapeKind::Enum
            | ShapeKind::List
            | ShapeKind::Map
            | ShapeKind::Blob
            | ShapeKind::Resource
            | ShapeKind::Unit,
        )
        | None => SendFieldType::Value,
    }
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
            let string_override = match resource_type_overrides()
                .get(&(resource_type_name, provider_name_snake.as_str()))
            {
                Some(
                    TypeOverride::RefinedString { .. }
                    | TypeOverride::StringPattern(_)
                    | TypeOverride::AnonymousStringConstructor(_),
                ) => true,
                Some(TypeOverride::SharedType(row)) => {
                    matches!(row.attribute_type().raw_shape(), RawShape::String { .. })
                }
                _ => false,
            };
            let attr = attrs
                .iter()
                .find(|attr| attr.provider_name.to_snake_case() == provider_name_snake);
            let attr = if attrs.iter().any(|attr| {
                attr.is_identity && attr.provider_name.to_snake_case() != provider_name_snake
            }) {
                Err("on a resource with a composite identity")
            } else {
                match attr {
                    None => Err("missing"),
                    Some(attr) if attr.is_read_only => Err("read-only"),
                    Some(attr)
                        if !attr.is_plain_string
                            || (attr.type_code != PLAIN_STRING_TYPE_CODE && !string_override) =>
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

fn markdown_example_dir_name(service: &str, page_stem: &str) -> String {
    format!("{service}_{page_stem}")
}

fn ensure_examples_match_rendered_pages(
    examples_dir: &Path,
    rendered_example_dirs: &HashSet<String>,
) -> Result<()> {
    let entries = std::fs::read_dir(examples_dir)
        .with_context(|| format!("Failed to read {}", examples_dir.display()))?;
    let mut unmatched = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("Failed to read {}", examples_dir.display()))?;
        if !entry
            .file_type()
            .with_context(|| format!("Failed to inspect example path {}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if !rendered_example_dirs.contains(&name) {
            unmatched.push(name);
        }
    }

    unmatched.sort();
    if !unmatched.is_empty() {
        anyhow::bail!(
            "Example directories without rendered pages: {}",
            unmatched.join(", ")
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
    run(ResolvedArgs::try_from(Args::parse())?)
}

fn run(args: ResolvedArgs) -> Result<()> {
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
    // Cross-service models validate hand-written updates and are also inputs
    // to resend-safety generation for the corresponding guarded SDK clients.
    for res in &resources {
        for namespace in res
            .update_ops
            .iter()
            .filter_map(|operation| operation.fields.cross_service_namespace())
        {
            if models.contains_key(namespace) {
                continue;
            }
            let model = load_model(&args.model_dir, namespace)?;
            models.insert(namespace, model);
        }
    }
    for ds in &data_sources {
        if models.contains_key(ds.service_namespace) {
            continue;
        }
        let model = load_model(&args.model_dir, ds.service_namespace)?;
        models.insert(ds.service_namespace, model);
    }
    for res in &resources {
        validate_cross_service_update_operations(res, &models)?;
        if res.has_tags {
            resource_tag_type(res, &models[res.service_namespace])?;
        }
    }

    match &args.output_mode {
        OutputMode::Rust => {
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
        OutputMode::Provider => {
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

            let send_dispositions = all_resources
                .iter()
                .map(ResourceDef::send_dispositions)
                .collect::<Vec<_>>();
            let send_resources = all_resources
                .iter()
                .zip(&send_dispositions)
                .map(|(resource, dispositions)| (resource, dispositions.as_slice()))
                .collect::<Vec<_>>();
            let send_models = generate_send_models_code(&send_resources, &models)?;
            let send_models_path = args.output_dir.join("send_models_generated.rs");
            std::fs::write(&send_models_path, send_models)
                .with_context(|| format!("Failed to write {}", send_models_path.display()))?;
            eprintln!("Generated: {}", send_models_path.display());

            let resend_safety = carina_codegen_aws::resend_safety::generate_provider_module(
                &models,
                carina_codegen_aws::resend_safety::OVERRIDES,
            )?;
            let resend_safety_path = args.output_dir.join("resend_safety_generated.rs");
            std::fs::write(&resend_safety_path, resend_safety)
                .with_context(|| format!("Failed to write {}", resend_safety_path.display()))?;
            eprintln!("Generated: {}", resend_safety_path.display());
        }
        OutputMode::Markdown { examples_dir } => {
            run_markdown(
                &args.output_dir,
                examples_dir,
                &resources,
                &data_sources,
                &all_resources,
                &all_data_sources,
                &models,
            )?;
        }
    }

    Ok(())
}

fn run_markdown(
    output_dir: &Path,
    examples_dir: &Path,
    resources: &[&ResourceDef],
    data_sources: &[&resource_defs::DataSourceDef],
    all_resources: &[ResourceDef],
    all_data_sources: &[resource_defs::DataSourceDef],
    models: &HashMap<&str, SmithyModel>,
) -> Result<()> {
    for res in resources {
        let model = models.get(res.service_namespace).unwrap();
        let (service, resource) = split_service_resource(res.name);
        let file_name = markdown_file_name(resource, false);
        let page_stem = file_name.trim_end_matches(".md");
        let example = example::load(
            examples_dir,
            &markdown_example_dir_name(service, page_stem),
            "aws",
            res.name,
            ExampleKind::Resource,
        )?;
        let md = generate_markdown_resource(res, model, &example)
            .with_context(|| format!("generating docs for aws.{}", res.name))?;

        let service_dir = output_dir.join(service);
        std::fs::create_dir_all(&service_dir)?;
        let output_path = service_dir.join(file_name);
        std::fs::write(&output_path, &md)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;
        eprintln!("Generated: {}", output_path.display());
    }
    for ds in data_sources {
        let model = models.get(ds.service_namespace).unwrap();
        let (service, resource) = split_service_resource(ds.name);
        let file_name = markdown_file_name(resource, true);
        let page_stem = file_name.trim_end_matches(".md");
        let example = example::load(
            examples_dir,
            &markdown_example_dir_name(service, page_stem),
            "aws",
            ds.name,
            ExampleKind::DataSource,
        )?;
        let md = generate_markdown_data_source(ds, model, &example)
            .with_context(|| format!("generating docs for aws.{}", ds.name))?;

        let service_dir = output_dir.join(service);
        std::fs::create_dir_all(&service_dir)?;
        let output_path = service_dir.join(file_name);
        std::fs::write(&output_path, &md)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;
        eprintln!("Generated: {}", output_path.display());
    }
    let documentable_example_dirs = all_resources
        .iter()
        .map(|resource| (resource.name, false))
        .chain(
            all_data_sources
                .iter()
                .map(|data_source| (data_source.name, true)),
        )
        .map(|(name, data_source)| {
            let (service, resource) = split_service_resource(name);
            let file_name = markdown_file_name(resource, data_source);
            markdown_example_dir_name(service, file_name.trim_end_matches(".md"))
        })
        .collect();
    ensure_examples_match_rendered_pages(examples_dir, &documentable_example_dirs)?;

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

fn validate_cross_service_update_operations(
    res: &ResourceDef,
    models: &HashMap<&str, SmithyModel>,
) -> Result<()> {
    for update_op in &res.update_ops {
        let resource_defs::FieldLayout::CrossServiceFlat {
            service_namespace,
            fields,
        } = &update_op.fields
        else {
            continue;
        };
        let model = models.get(service_namespace).with_context(|| {
            format!(
                "Missing Smithy model {service_namespace} for {} cross-service update {}",
                res.name, update_op.operation
            )
        })?;
        let operation_id = format!("{service_namespace}#{}", update_op.operation);
        let input = model.operation_input(&operation_id).with_context(|| {
            format!(
                "Cannot find cross-service update input for {operation_id} used by {}",
                res.name
            )
        })?;
        for field in fields {
            anyhow::ensure!(
                input.members.contains_key(*field),
                "Cross-service update operation {operation_id} for {} is missing declared field {field}",
                res.name
            );
        }
    }
    Ok(())
}

struct CollectedResourceAttributes {
    attrs: Vec<AttrInfo>,
    all_enums: BTreeMap<String, EnumInfo>,
    description: Option<Description>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StructMemberKey {
    parent_shape_id: String,
    member_name: String,
}

#[derive(Debug)]
struct ResolvedNestedWriteOnly {
    field: &'static str,
    member: &'static str,
    key: StructMemberKey,
}

fn resolve_nested_write_only_fields(
    res: &ResourceDef,
    model: &SmithyModel,
    dispositions: &[resource_defs::ReadDisposition],
) -> Result<Vec<ResolvedNestedWriteOnly>> {
    use resource_defs::ReadDisposition;

    let nested_dispositions = dispositions
        .iter()
        .filter_map(|disposition| match disposition {
            ReadDisposition::NestedWriteOnly { field, member, .. } => Some((*field, *member)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if nested_dispositions.is_empty() {
        return Ok(Vec::new());
    }

    let conflicting_top_level = dispositions
        .iter()
        .filter_map(|disposition| match disposition {
            ReadDisposition::WriteOnly { field, .. } => Some((field.to_snake_case(), "WriteOnly")),
            ReadDisposition::OmitSchema { field, .. } => {
                Some((field.to_snake_case(), "OmitSchema"))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    let writable_structure = if let Some(schema_structure) = res.schema_structure {
        model
            .get_structure(&format!("{}#{schema_structure}", res.service_namespace))
            .with_context(|| {
                format!(
                    "Cannot find schema structure {}#{schema_structure}",
                    res.service_namespace
                )
            })?
    } else {
        model
            .operation_input(&format!("{}#{}", res.service_namespace, res.create_op))
            .with_context(|| {
                format!(
                    "Cannot find create input {}#{}",
                    res.service_namespace, res.create_op
                )
            })?
    };

    let mut resolved = Vec::with_capacity(nested_dispositions.len());
    let mut resolved_keys = HashMap::new();
    for (field, member) in nested_dispositions {
        if let Some(conflict) = conflicting_top_level.get(&field.to_snake_case()) {
            anyhow::bail!(
                "{} nested write-only field {}.{} conflicts with top-level {} disposition for {}",
                res.name,
                field,
                member,
                conflict,
                field
            );
        }

        let parent = writable_structure.members.get(field).with_context(|| {
            format!(
                "{} nested write-only field {}.{} has no writable parent member named exactly {}; use the Smithy member spelling",
                res.name, field, member, field
            )
        })?;
        let nested_structure = model.get_structure(&parent.target).with_context(|| {
            format!(
                "{} nested write-only parent {} must target a structure",
                res.name, field
            )
        })?;
        let (resolved_member, _) =
            nested_structure
                .members
                .get_key_value(member)
                .with_context(|| {
                    format!(
                        "{} nested write-only field {}.{} does not name an exact member in {}; use the Smithy member spelling",
                        res.name, field, member, parent.target
                    )
                })?;
        let key = StructMemberKey {
            parent_shape_id: parent.target.clone(),
            member_name: resolved_member.clone(),
        };
        if let Some((previous_field, previous_member)) =
            resolved_keys.insert(key.clone(), (field, member))
        {
            anyhow::bail!(
                "{} nested write-only fields {}.{} and {}.{} resolve to the same Smithy member {}.{}",
                res.name,
                previous_field,
                previous_member,
                field,
                member,
                key.parent_shape_id,
                key.member_name
            );
        }
        resolved.push(ResolvedNestedWriteOnly { field, member, key });
    }

    Ok(resolved)
}

/// Collect the exact top-level schema attributes before rendering either the
/// schema or its typed read model. Sharing this pass is the guard against the
/// two generated contracts drifting apart.
fn collect_resource_attributes_unvalidated(
    res: &ResourceDef,
    model: &SmithyModel,
    read_dispositions: &[resource_defs::ReadDisposition],
) -> Result<CollectedResourceAttributes> {
    let ns = res.service_namespace;
    let namespace = format!("aws.{}", res.name);
    let resolved_nested_write_only =
        resolve_nested_write_only_fields(res, model, read_dispositions)?;

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
    let identity_overrides: HashSet<String> = res
        .identity_overrides
        .iter()
        .map(|name| name.to_snake_case())
        .collect();

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

    // Nested write-only fields are keyed by the resolved Smithy parent shape
    // and exact member name. Emission counts make resolution self-checking:
    // a type override cannot silently bypass the StructField marker.
    let nested_write_only_struct_fields: HashSet<StructMemberKey> = resolved_nested_write_only
        .iter()
        .map(|resolved| resolved.key.clone())
        .collect();
    let mut nested_write_only_emissions: HashMap<StructMemberKey, usize> = HashMap::new();

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
        // Cross-service flat inputs are validated against their own model
        // before generation; this collector only borrows the resource model.
        if update_op.fields.cross_service_namespace().is_some() {
            continue;
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
            Some(Description::authored(desc))
        } else {
            SmithyModel::documentation(&member_ref.traits).map(Description::from_smithy_html)
        };

        validate_smithy_type(model, res.name, &snake_name, name, member_ref)?;
        let (type_code, enum_info) = resolve_type(
            &mut TypeResolutionContext {
                model,
                resource_name: res.name,
                namespace: &namespace,
                struct_path: Vec::new(),
                resolving_shapes: Vec::new(),
                property_path: snake_name.clone(),
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
                current_root_attr: Some(name.as_str()),
                deferred_populate_struct_fields: &deferred_populate_struct_fields,
                nested_write_only_struct_fields: &nested_write_only_struct_fields,
                nested_write_only_emissions: &mut nested_write_only_emissions,
                struct_field_type_overrides: &struct_field_type_overrides,
            },
            &member_ref.target,
            name,
        )?;

        let is_plain_string = is_plain_smithy_string(model, &member_ref.target);
        let send_type = send_field_type(model, &member_ref.target, &type_code, enum_info.as_ref());
        let sdk_i32 = send_type == SendFieldType::Int
            && matches!(
                model.shape_kind(&member_ref.target),
                Some(ShapeKind::Integer | ShapeKind::IntEnum)
            );
        let sdk_i32_paths = if send_type == SendFieldType::Value {
            nested_sdk_i32_paths(model, &member_ref.target, &type_code)
        } else {
            Vec::new()
        };
        let is_identity = identity_overrides.contains(&snake_name);
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
            is_sent: false,
            send_via: None,
            send_reason: None,
            send_type,
            sdk_i32,
            sdk_i32_paths,
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
        carina_codegen_aws::kms_references::validate_synthetic_type(model, res, extra)?;
        let type_code =
            synthetic_extra_attribute_type(res.name, extra.name, &type_overrides)?.type_code;
        let is_read_only = read_only_overrides.contains(extra.name);
        let is_create_only = !is_read_only
            && (!updatable_fields.contains(extra.name)
                || create_only_overrides.contains(extra.name));
        let is_required = required_overrides.contains(extra.name);
        let send_type = send_field_type(model, "smithy.api#String", &type_code, None);
        let sdk_i32_paths = if send_type == SendFieldType::Value {
            nested_sdk_i32_paths(model, "smithy.api#String", &type_code)
        } else {
            Vec::new()
        };
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
            is_sent: false,
            send_via: None,
            send_reason: None,
            send_type,
            sdk_i32: false,
            sdk_i32_paths,
            is_plain_string: false,
            is_identity: identity_overrides.contains(&extra.name.to_snake_case()),
            is_deferred_populate: deferred_populate_overrides.contains(extra.name),
            description: extra.description.map(Description::authored),
            enum_info: None,
        });
    }

    // Process read-only fields
    for (name, member_ref) in &read_only_fields {
        let snake_name = name.to_snake_case();
        let description =
            SmithyModel::documentation(&member_ref.traits).map(Description::from_smithy_html);

        validate_smithy_type(model, res.name, &snake_name, name, member_ref)?;
        let (type_code, enum_info) = resolve_type(
            &mut TypeResolutionContext {
                model,
                resource_name: res.name,
                namespace: &namespace,
                struct_path: Vec::new(),
                resolving_shapes: Vec::new(),
                property_path: snake_name.clone(),
                type_overrides: &type_overrides,
                enum_alias_map: &enum_alias_map,
                to_dsl_overrides: &to_dsl_overrides,
                all_enums: &mut all_enums,
                all_ranged_ints: &mut all_ranged_ints,
                current_root_attr: Some(name.as_str()),
                deferred_populate_struct_fields: &deferred_populate_struct_fields,
                nested_write_only_struct_fields: &nested_write_only_struct_fields,
                nested_write_only_emissions: &mut nested_write_only_emissions,
                struct_field_type_overrides: &struct_field_type_overrides,
            },
            &member_ref.target,
            name,
        )?;

        let is_plain_string = is_plain_smithy_string(model, &member_ref.target);
        let send_type = send_field_type(model, &member_ref.target, &type_code, enum_info.as_ref());
        let sdk_i32 = send_type == SendFieldType::Int
            && matches!(
                model.shape_kind(&member_ref.target),
                Some(ShapeKind::Integer | ShapeKind::IntEnum)
            );
        let sdk_i32_paths = if send_type == SendFieldType::Value {
            nested_sdk_i32_paths(model, &member_ref.target, &type_code)
        } else {
            Vec::new()
        };
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
            is_sent: false,
            send_via: None,
            send_reason: None,
            send_type,
            sdk_i32,
            sdk_i32_paths,
            is_plain_string,
            is_identity: identity_overrides.contains(&name.to_snake_case()),
            is_deferred_populate: deferred_populate_overrides.contains(name.as_str()),
            description,
            enum_info,
        });
    }

    for attr in &attrs {
        anyhow::ensure!(
            !attr.is_identity || attr.is_create_only,
            "{} identity attribute {} must be create-only",
            res.name,
            attr.provider_name
        );
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
        SmithyModel::documentation(&read_struct.traits).map(Description::from_smithy_html)
    } else {
        create_input
            .as_ref()
            .and_then(|input| SmithyModel::documentation(&input.traits))
            .map(Description::from_smithy_html)
    };

    for resolved in &resolved_nested_write_only {
        let emission_count = nested_write_only_emissions
            .get(&resolved.key)
            .copied()
            .unwrap_or_default();
        anyhow::ensure!(
            emission_count == 1,
            "{} nested write-only field {}.{} resolved to {}.{}, but generated {} matching StructField markers; expected exactly one (a type override or reused shape may have bypassed exact emission)",
            res.name,
            resolved.field,
            resolved.member,
            resolved.key.parent_shape_id,
            resolved.key.member_name,
            emission_count
        );
    }

    Ok(CollectedResourceAttributes {
        attrs,
        all_enums,
        description,
    })
}

fn collect_resource_attributes_with_read_dispositions(
    res: &ResourceDef,
    model: &SmithyModel,
    dispositions: &[resource_defs::ReadDisposition],
) -> Result<CollectedResourceAttributes> {
    let mut collected = collect_resource_attributes_unvalidated(res, model, dispositions)?;
    apply_read_contract(res, model, &mut collected.attrs, dispositions)?;
    collected.all_enums.retain(|provider_name, _| {
        collected
            .attrs
            .iter()
            .any(|attr| attr.provider_name == *provider_name)
    });
    Ok(collected)
}

fn collect_resource_attributes(
    res: &ResourceDef,
    model: &SmithyModel,
) -> Result<CollectedResourceAttributes> {
    let dispositions = res.read_dispositions();
    collect_resource_attributes_with_read_dispositions(res, model, &dispositions)
}

#[cfg(test)]
fn validate_send_contract(
    res: &ResourceDef,
    model: &SmithyModel,
    dispositions: &[resource_defs::SendDisposition],
) -> Result<()> {
    let read_dispositions = res.read_dispositions();
    let mut collected = collect_resource_attributes_unvalidated(res, model, &read_dispositions)?;
    apply_send_contract(res, &mut collected.attrs, dispositions).map(|_| ())
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AppliedSendContract {
    /// Synthetic tags are emitted outside `AttrInfo`; retain their deliberate
    /// omission reason here so it is not lost merely because tags are special.
    tags_not_sent_reason: Option<String>,
}

fn apply_send_contract(
    res: &ResourceDef,
    attrs: &mut [AttrInfo],
    dispositions: &[resource_defs::SendDisposition],
) -> Result<AppliedSendContract> {
    use resource_defs::SendDisposition;

    let mut applied = AppliedSendContract::default();
    let mut classified = HashSet::new();
    for disposition in dispositions {
        let field = disposition.field();
        let normalized = field.to_snake_case();
        if !classified.insert(normalized.clone()) {
            anyhow::bail!(
                "{} has duplicate/conflicting send dispositions for {}",
                res.name,
                field
            );
        }

        let attr = attrs
            .iter_mut()
            .find(|attr| attr.provider_name.to_snake_case() == normalized);
        if attr.is_none() && !(normalized == "tags" && res.has_tags) {
            anyhow::bail!(
                "{} send disposition names unknown schema attribute {}",
                res.name,
                field
            );
        }
        if attr.as_ref().is_some_and(|attr| attr.is_read_only) {
            anyhow::bail!(
                "{} send disposition names provider-populated/read-only attribute {}",
                res.name,
                field
            );
        }

        match disposition {
            SendDisposition::Sent { via, .. } => {
                if let Some(attr) = attr {
                    attr.is_sent = true;
                    attr.send_via = Some(*via);
                }
            }
            SendDisposition::NotSent { reason, .. } => {
                if reason.trim().is_empty() {
                    anyhow::bail!(
                        "{} NotSent attribute {} requires a non-empty reason",
                        res.name,
                        field
                    );
                }
                if let Some(attr) = attr {
                    attr.send_reason = Some((*reason).to_string());
                } else {
                    debug_assert_eq!(normalized, "tags");
                    applied.tags_not_sent_reason = Some((*reason).to_string());
                }
            }
        }
    }

    let unclassified = attrs
        .iter()
        .filter(|attr| {
            !attr.is_read_only && !classified.contains(&attr.provider_name.to_snake_case())
        })
        .map(|attr| attr.provider_name.as_str())
        .collect::<Vec<_>>();
    let mut unclassified = unclassified;
    if res.has_tags && !classified.contains("tags") {
        unclassified.push("Tags");
    }
    if !unclassified.is_empty() {
        anyhow::bail!(
            "{} writable schema attributes are unclassified for send: {}; add Sent or NotSent dispositions",
            res.name,
            unclassified.join(", ")
        );
    }

    Ok(applied)
}

#[cfg(test)]
fn validate_read_contract(
    res: &ResourceDef,
    model: &SmithyModel,
    dispositions: &[resource_defs::ReadDisposition],
) -> Result<()> {
    collect_resource_attributes_with_read_dispositions(res, model, dispositions).map(|_| ())
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

    let conflicting_top_level = dispositions
        .iter()
        .filter_map(|disposition| match disposition {
            ReadDisposition::WriteOnly { field, .. } => Some((field.to_snake_case(), "WriteOnly")),
            ReadDisposition::OmitSchema { field, .. } => {
                Some((field.to_snake_case(), "OmitSchema"))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut seen_nested = HashSet::new();
    for disposition in dispositions {
        let field = disposition.field();
        let normalized = field.to_snake_case();
        match disposition {
            ReadDisposition::NestedWriteOnly { member, .. } => {
                if !seen_nested.insert((field, *member)) {
                    anyhow::bail!(
                        "{} has duplicate/conflicting nested read dispositions for {}.{}",
                        res.name,
                        field,
                        member
                    );
                }
            }
            _ => {
                if !seen.insert(normalized.clone()) {
                    anyhow::bail!(
                        "{} has duplicate/conflicting read dispositions for {}",
                        res.name,
                        field
                    );
                }
            }
        }

        let Some(attr) = attrs.iter_mut().find(|attr| match disposition {
            ReadDisposition::NestedWriteOnly { .. } => attr.provider_name == field,
            _ => attr.provider_name.to_snake_case() == normalized,
        }) else {
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
            ReadDisposition::NestedWriteOnly { member, reason, .. } => {
                if let Some(conflict) = conflicting_top_level.get(&normalized) {
                    anyhow::bail!(
                        "{} nested write-only field {}.{} conflicts with top-level {} disposition for {}",
                        res.name,
                        field,
                        member,
                        conflict,
                        field
                    );
                }
                if reason.trim().is_empty() {
                    anyhow::bail!(
                        "{} nested write-only field {}.{} requires a non-empty reason",
                        res.name,
                        field,
                        member
                    );
                }
                if attr.is_read_only || !attr.is_create_only {
                    anyhow::bail!(
                        "{} nested write-only field {}.{} must belong to a writable, create-only attribute",
                        res.name,
                        field,
                        member
                    );
                }
                let explicitly_readable = dispositions.iter().any(|candidate| {
                    matches!(candidate, ReadDisposition::Read { .. })
                        && candidate.field().to_snake_case() == normalized
                });
                if !explicitly_readable && !has_inferred_read_source(res, model, attr) {
                    anyhow::bail!(
                        "{} nested write-only field {}.{} requires its parent attribute to be readable",
                        res.name,
                        field,
                        member
                    );
                }
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
        mut attrs,
        all_enums,
        description,
    } = collect_resource_attributes(res, model)?;
    // Schema generation is one of the two production send-contract seams;
    // provider generation independently enforces the same contract while
    // producing the typed send models. Inline `com.amazonaws.test` fixtures
    // exercise schema rendering in isolation and validate send classification
    // explicitly in the send-contract tests below.
    let applied_send_contract = if res.service_namespace != "com.amazonaws.test" {
        let send_dispositions = res.send_dispositions();
        Some(apply_send_contract(res, &mut attrs, &send_dispositions)?)
    } else {
        None
    };
    let read_back = res.read_back_overrides();
    read_back
        .validate(attrs.iter().map(|attr| attr.snake_name.as_str()))
        .map_err(anyhow::Error::msg)?;
    let ns = res.service_namespace;
    let is_data_source = res.create_op.is_empty();
    let namespace = format!("aws.{}", res.name);
    let tags_type_code = res
        .has_tags
        .then(|| resource_tag_type(res, model).map(|tags| tags.code()))
        .transpose()?;
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
    // Only schema `types::` constructors need the import; fully qualified
    // `carina_aws_types::` constructors do not.
    let needs_types = code_str
        .replace("carina_aws_types::", "")
        .contains("types::");
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
        .any(|a| a.type_code.contains("AttributeType::") || a.enum_info.is_some())
        || tags_type_code.is_some();
    if needs_attribute_type {
        schema_imports.insert(1, "AttributeType");
    }
    if needs_struct_field {
        schema_imports.push("StructField");
    }
    if code_str.contains("FieldGroup::") {
        schema_imports.push("FieldGroup");
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

    if res.has_tags {
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

    let requirements = read_back
        .await_attributes
        .iter()
        .map(|requirement| {
            let predicate = match requirement.predicate {
                resource_defs::ReadBackPredicate::AcmDnsDvo => "ReadBackPredicate::AcmDnsDvo",
            };
            format!(
                "ReadBackRequirement::new(\"{}\", {predicate})",
                requirement.attribute
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !read_back.await_attributes.is_empty() {
        code.push_str("use crate::schemas::config::{ReadBackPredicate, ReadBackRequirement};\n\n");
    }
    // Generate config function
    code.push_str(&format!(
        "/// Returns the schema config for {} (Smithy: {})\n\
         pub fn {}_config() -> AwsSchemaConfig {{\n\
         \x20   AwsSchemaConfig {{\n\
         \x20       aws_type_name: \"{}\",\n\
         \x20       resource_type_name: \"{}\",\n\
         \x20       has_tags: {},\n\
         \x20       await_on_read_back: vec![{}],\n\
         \x20       schema: ResourceSchema::new(\"{}\")\n",
        res.name,
        ns,
        mod_name,
        cf_type_name(res.name),
        res.name,
        res.has_tags,
        requirements,
        res.name,
    ));

    if let Some(description) = description {
        code.push_str(&format!(
            "\x20       .with_description(\"{}\")\n",
            description.rust_string_literal(Some(200))
        ));
    }

    if resource_defs::defaults_to_prevent_destroy(res.name) {
        code.push_str("        .default_prevent_destroy()\n");
    }

    // Mark data sources
    if is_data_source {
        code.push_str("\x20       .as_data_source()\n");
    }

    if let Some(config) = read_back.operation_config_rust() {
        code.push_str(&config);
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
            let identity_code = enum_identity_code(ei, &namespace);
            format!(
                "AttributeType::enum_(\n\
                 \x20   {},\n\
                 \x20   Some(vec![{}]),\n\
                 \x20   {},\n\
                 \x20   None,\n\
                 \x20   None,\n\
                 )",
                identity_code, values_str, dsl_aliases_code
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
        if attr.is_identity {
            attr_code.push_str("\n\x20               .identity()");
        }
        if attr.is_write_only {
            attr_code.push_str("\n\x20               .write_only()");
        }
        if attr.is_deferred_populate {
            attr_code.push_str("\n\x20               .deferred_populate()");
        }

        if let Some(ref desc) = attr.description {
            let suffix = if attr.is_read_only {
                " (read-only)"
            } else {
                ""
            };
            attr_code.push_str(&format!(
                "\n\x20               .with_description(\"{}{}\")",
                desc.rust_string_literal(Some(150)),
                suffix
            ));
        } else if attr.is_read_only {
            attr_code.push_str(&format!(
                "\n\x20               .with_description(\"{}\")",
                Description::authored("(read-only)").rust_string_literal(None)
            ));
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
    if let Some(tags_type_code) = tags_type_code {
        if let Some(reason) = applied_send_contract
            .as_ref()
            .and_then(|contract| contract.tags_not_sent_reason.as_deref())
        {
            code.push_str(&format!(
                "        // Send contract: {}\n",
                reason.replace('\n', " ")
            ));
        }
        code.push_str(&format!(
            "\x20       .attribute(\n\
             \x20           AttributeSchema::optional(\"tags\", {tags_type_code})\n\
             \x20               .with_description(\"{}\")\n\
             \x20               .with_provider_name(\"Tags\")\n\
             \x20               .build(),\n\
             \x20       )\n",
            Description::authored("The tags for the resource.").rust_string_literal(None)
        ));
    }

    for rule in &res.input_rules {
        code.push_str(&format!("        .input_rule({})\n", input_rule_code(rule)));
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
    /// Resource name used by resource-specific type overrides, e.g. `acm.Certificate`.
    resource_name: &'a str,
    /// Base resource namespace, e.g. `aws.acm.Certificate`.
    namespace: &'a str,
    /// Containing Smithy struct type names between the resource and the
    /// field currently being resolved.
    struct_path: Vec<String>,
    /// Full shape IDs on the active expansion path. Repeated sibling uses
    /// are allowed; an ancestor repeat is a recursive schema.
    resolving_shapes: Vec<String>,
    /// Full property path, including array markers, independent of Smithy type names.
    property_path: String,
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
    /// Resolved `(parent_shape_id, member_name)` keys whose user-input
    /// StructField is accepted by AWS but absent from every read response.
    /// The generated `.write_only()` marker lets core preserve and compare
    /// the accepted leaf without projecting it from provider state.
    nested_write_only_struct_fields: &'a HashSet<StructMemberKey>,
    /// Number of StructFields emitted for each resolved nested write-only key.
    /// Every configured disposition must finish with exactly one match.
    nested_write_only_emissions: &'a mut HashMap<StructMemberKey, usize>,
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

fn enum_identity_override(
    resource_name: &str,
    parent_struct: Option<&str>,
    field_name: &str,
) -> Option<AwsType> {
    let nested_property;
    let property_name = if let Some(parent_struct) = parent_struct {
        nested_property = format!("{parent_struct}.{field_name}");
        nested_property.as_str()
    } else {
        field_name
    };

    match resource_type_overrides().get(&(resource_name, property_name)) {
        Some(TypeOverride::EnumIdentity(identity)) => Some(*identity),
        _ => None,
    }
}

fn enum_identity_code(enum_info: &EnumInfo, default_namespace: &str) -> String {
    if let Some(aws_type) = enum_info.identity_override {
        return format!(
            "carina_aws_types::AwsType::{}.identity()",
            aws_type.rust_variant_name()
        );
    }

    let (namespace, type_name) =
        effective_enum_identity(None, default_namespace, &enum_info.type_name);
    format!(
        "carina_core::schema::enum_identity(\"{}\", Some(\"{}\"))",
        type_name, namespace
    )
}

fn effective_enum_identity(
    identity_override: Option<AwsType>,
    default_namespace: &str,
    default_type_name: &str,
) -> (String, String) {
    if let Some(aws_type) = identity_override {
        let identity = aws_type.identity();
        let namespace = identity
            .dotted_prefix()
            .expect("AWS catalog identities always have a provider or segments");
        (namespace, identity.kind)
    } else {
        (default_namespace.to_string(), default_type_name.to_string())
    }
}

/// The same struct representation drives schema and Markdown generation.
struct StructRepresentation<'a> {
    members: &'a BTreeMap<String, carina_smithy::ShapeRef>,
    required_fields: HashSet<&'a str>,
    field_groups: Vec<FieldGroup>,
}

impl<'a> StructRepresentation<'a> {
    fn from_structure(structure: &'a carina_smithy::StructureShape) -> Self {
        Self {
            members: &structure.members,
            required_fields: structure
                .members
                .iter()
                .filter_map(|(name, member)| {
                    SmithyModel::is_required(member).then_some(name.as_str())
                })
                .collect(),
            field_groups: vec![],
        }
    }
}

/// A singleton union requires its sole field. Larger unions require exactly
/// one optional field; an empty union has no valid value and cannot be emitted.
fn union_struct_representation<'a>(
    shape_id: &str,
    members: &'a BTreeMap<String, carina_smithy::ShapeRef>,
) -> Result<StructRepresentation<'a>> {
    let names: Vec<&str> = members.keys().map(String::as_str).collect();
    let (required_fields, field_groups) = match names.as_slice() {
        [] => anyhow::bail!("Smithy union {shape_id} has no members"),
        [member] => (HashSet::from([*member]), vec![]),
        [first, second, remaining @ ..] => (
            HashSet::new(),
            vec![FieldGroup::exactly_one(
                [*first, *second]
                    .into_iter()
                    .chain(remaining.iter().copied())
                    .map(|name| name.to_snake_case()),
            )?],
        ),
    };
    Ok(StructRepresentation {
        members,
        required_fields,
        field_groups,
    })
}

fn field_group_code(group: &FieldGroup) -> String {
    let constructor = match group.kind() {
        FieldGroupKind::ExactlyOne => "exactly_one",
        FieldGroupKind::AtLeastOne => "at_least_one",
        FieldGroupKind::AtMostOne => "at_most_one",
    };
    let fields = group
        .fields()
        .iter()
        .map(|field| format!("\"{field}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("FieldGroup::{constructor}([{fields}]).expect(\"generated field group must be valid\")")
}

type AttributeTypeConstructor = fn() -> AttributeType;

/// Resolve constructor overrides to the same concrete types used by schemas.
/// This registry contains constructors, never field names or group rules.
fn attribute_type_from_code(code: &str) -> Result<AttributeType> {
    for (prefix, constructor) in [
        (
            "AttributeType::list(",
            AttributeType::list as fn(AttributeType) -> AttributeType,
        ),
        (
            "AttributeType::unordered_list(",
            AttributeType::unordered_list,
        ),
        ("AttributeType::map(", AttributeType::map),
    ] {
        if let Some(inner) = code
            .strip_prefix(prefix)
            .and_then(|inner| inner.strip_suffix(')'))
        {
            return Ok(constructor(attribute_type_from_code(inner)?));
        }
    }
    if let Some(inner) = code
        .strip_prefix("AttributeType::union(vec![")
        .and_then(|inner| inner.strip_suffix("])"))
    {
        return Ok(AttributeType::union(
            split_top_level_commas(inner)
                .into_iter()
                .map(|member| attribute_type_from_code(member.trim()))
                .collect::<Result<Vec<_>>>()?,
        ));
    }
    if code == "super::super::iam::role::arn()" {
        return Ok(AwsType::IamRoleArn.attribute_type());
    }
    let name = code.rsplit("::").next().unwrap_or(code);
    if let Some(aws_type) = AwsType::ALL
        .iter()
        .find(|ty| ty.constructor_path().rsplit("::").next() == Some(name))
    {
        return Ok(aws_type.attribute_type());
    }
    macro_rules! constructors {
        ($($constructor:ident),+ $(,)?) => {
            [$( (concat!(stringify!($constructor), "()"), carina_aws_types::$constructor as AttributeTypeConstructor) ),+]
        };
    }
    let constructors = constructors![
        route53_dns_name,
        iam_path,
        iam_path_prefix,
        iam_policy_document,
        sqs_redrive_policy,
        sqs_redrive_allow_policy,
        bucket_encryption_rules,
        bucket_replication_rules,
        bucket_lifecycle_rules,
        bucket_object_lock_rule,
        s3_index_document,
        s3_error_document,
        s3_redirect_all_requests_to,
        bucket_cors_rules,
        bucket_topic_configurations,
        bucket_queue_configurations,
        bucket_lambda_function_configurations,
        bucket_event_bridge_configuration,
        bucket_target_object_key_format,
        identitystore_user_name,
        kms_key_arn_reference,
        nat_gateway_availability_zone_addresses,
        s3_object_lock_enabled,
        s3_versioning_status,
        s3_mfa_delete,
        s3_object_ownership,
        gateway_id,
    ];
    if let Some(entry) = constructors.iter().find(|entry| entry.0 == name) {
        return Ok(entry.1());
    }
    let builtins: &[(&str, AttributeTypeConstructor)] = &[
        ("string()", AttributeType::string),
        ("bool()", AttributeType::bool),
        ("int()", AttributeType::int),
        ("float()", AttributeType::float),
        ("duration()", AttributeType::duration),
        ("ipv4_cidr()", carina_core::schema::types::ipv4_cidr),
        ("ipv6_cidr()", carina_core::schema::types::ipv6_cidr),
        ("ipv4_address()", carina_core::schema::types::ipv4_address),
        ("ipv6_address()", carina_core::schema::types::ipv6_address),
        ("email()", carina_core::schema::types::email),
    ];
    builtins
        .iter()
        .find(|entry| entry.0 == name)
        .map(|entry| entry.1())
        .with_context(|| format!("No AttributeType constructor registered for {code}"))
}

fn enum_type_code(ctx: &TypeResolutionContext<'_>, field_name: &str, info: &EnumInfo) -> String {
    let namespace = enum_namespace(ctx);
    let snake_name = field_name.to_snake_case();
    let dsl_aliases_code = if let Some(code) = ctx.to_dsl_overrides.get(snake_name.as_str()) {
        code.to_string()
    } else {
        build_dsl_aliases_code(&info.values, ctx.enum_alias_map.get(snake_name.as_str()))
    };
    let values = info
        .values
        .iter()
        .map(|value| format!("\"{value}\".to_string()"))
        .collect::<Vec<_>>()
        .join(", ");
    let identity = enum_identity_code(info, &namespace);
    format!(
        "AttributeType::enum_(\n\
         \x20   {},\n\
         \x20   Some(vec![{}]),\n\
         \x20   {},\n\
         \x20   None,\n\
         \x20   None,\n\
         )",
        identity, values, dsl_aliases_code
    )
}

/// Resolve Smithy schema code and enum metadata.
/// Also populates collectors for enums and ranged ints discovered during resolution.
fn resolve_type(
    ctx: &mut TypeResolutionContext<'_>,
    target: &str,
    field_name: &str,
) -> Result<(String, Option<EnumInfo>)> {
    if let Some(code) = reference_type_code(ctx.resource_name, &ctx.property_path) {
        return Ok((code.to_string(), None));
    }
    if let Some(override_type) = resource_value_override(ctx.resource_name, &ctx.property_path) {
        return Ok((
            bound_int_override(ctx.model, target, field_name, override_type).type_code,
            None,
        ));
    }
    // Check type overrides first
    if let Some(&override_type) = ctx.type_overrides.get(field_name) {
        if let Some(list) = string_list_value_override(
            ctx.model,
            target,
            ctx.resource_name,
            &ctx.property_path,
            override_type,
        ) {
            return Ok((list.type_code, None));
        }
        return Ok((
            bounded_int_constructor_code(ctx.model, target, field_name, override_type)?,
            None,
        ));
    }

    // Check known enum overrides
    if let Some(values) = known_enum_overrides().get(field_name) {
        let type_name = field_name.to_string();
        let enum_info = EnumInfo {
            type_name,
            values: values.iter().map(|s| s.to_string()).collect(),
            identity_override: enum_identity_override(
                ctx.resource_name,
                ctx.struct_path.last().map(String::as_str),
                field_name,
            ),
        };
        ctx.all_enums
            .entry(field_name.to_string())
            .or_insert_with(|| enum_info.clone());
        return Ok(("/* enum */".to_string(), Some(enum_info)));
    }

    if let Some(start) = ctx
        .resolving_shapes
        .iter()
        .position(|shape| shape == target)
    {
        let cycle = ctx.resolving_shapes[start..]
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(target))
            .collect::<Vec<_>>()
            .join(" -> ");
        anyhow::bail!("Recursive Smithy shape cycle: {cycle}");
    }
    ctx.resolving_shapes.push(target.to_string());
    let result = resolve_type_shape(ctx, target, field_name);
    ctx.resolving_shapes.pop();
    result
}

/// Expand a model shape after overrides, through the shared recursion guard.
fn resolve_type_shape(
    ctx: &mut TypeResolutionContext<'_>,
    target: &str,
    field_name: &str,
) -> Result<(String, Option<EnumInfo>)> {
    let kind = ctx.model.shape_kind(target);

    Ok(match kind {
        Some(ShapeKind::String) => {
            // Check name-based type inference (handles CIDR, IP, AZ, ARN, resource IDs, etc.)
            if let Some(inferred) = infer_string_type(field_name) {
                return Ok((inferred, None));
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
                    identity_override: enum_identity_override(
                        ctx.resource_name,
                        ctx.struct_path.last().map(String::as_str),
                        field_name,
                    ),
                };
                ctx.all_enums
                    .entry(field_name.to_string())
                    .or_insert_with(|| enum_info.clone());
                return Ok(("/* enum */".to_string(), Some(enum_info)));
            }
            (PLAIN_STRING_TYPE_CODE.to_string(), None)
        }
        Some(ShapeKind::IntEnum) => ("AttributeType::int()".to_string(), None),
        Some(ShapeKind::List) => {
            // Get list member type
            if let Some(carina_smithy::Shape::List(list_shape)) = ctx.model.get_shape(target) {
                let previous_path = ctx.property_path.clone();
                ctx.property_path =
                    kms_references::property_path(&previous_path, PathSegment::ListElement);
                let (item_type, enum_info) =
                    resolve_type(ctx, &list_shape.member.target, field_name)?;
                ctx.property_path = previous_path;
                let item_type = enum_info
                    .as_ref()
                    .map(|info| enum_type_code(ctx, field_name, info))
                    .unwrap_or(item_type);
                let code = match resource_type_overrides()
                    .get(&(ctx.resource_name, ctx.property_path.as_str()))
                {
                    Some(TypeOverride::ListLength { min, max }) => format!(
                        "AttributeType::refined_list({item_type}, true, Some((Some({min}), Some({max}))), carina_core::schema::ListElements::AllowDuplicates)"
                    ),
                    _ => format!("AttributeType::list({item_type})"),
                };
                (code, None)
            } else {
                (
                    format!("AttributeType::list({PLAIN_STRING_TYPE_CODE})"),
                    None,
                )
            }
        }
        Some(ShapeKind::Map) => {
            if let Some(Shape::Map(map_shape)) = ctx.model.get_shape(target) {
                let previous_path = ctx.property_path.clone();
                ctx.property_path =
                    kms_references::property_path(&previous_path, PathSegment::MapValue);
                let (value_type, enum_info) =
                    resolve_type(ctx, &map_shape.value.target, field_name)?;
                ctx.property_path = previous_path;
                let value_type = enum_info
                    .as_ref()
                    .map(|info| enum_type_code(ctx, field_name, info))
                    .unwrap_or(value_type);
                let code = map_type_code(
                    PLAIN_STRING_TYPE_CODE,
                    &value_type,
                    length_from_traits(&map_shape.traits),
                );
                (code, None)
            } else {
                (
                    format!("AttributeType::map({PLAIN_STRING_TYPE_CODE})"),
                    None,
                )
            }
        }
        Some(ShapeKind::Structure) => {
            // Check if it's a TagList-like structure
            let shape_name = SmithyModel::shape_name(target);
            if (shape_name == "TagList" || shape_name == "Tag")
                && let Some(tags) = TagMapType::from_shape(ctx.model, target)?
            {
                return Ok((tags.code(), None));
            }

            // Unwrap EC2 AttributeBooleanValue wrapper → plain Bool
            if shape_name == "AttributeBooleanValue" {
                return Ok(("AttributeType::bool()".to_string(), None));
            }

            // Generate struct type for nested structures
            if let Some(structure) = ctx.model.get_structure(target) {
                let struct_code = generate_struct_type(
                    ctx,
                    target,
                    StructRepresentation::from_structure(structure),
                )?;
                return Ok((struct_code, None));
            }
            (PLAIN_STRING_TYPE_CODE.to_string(), None)
        }
        Some(ShapeKind::Union) => {
            let Some(Shape::Union(union)) = ctx.model.get_shape(target) else {
                anyhow::bail!("Cannot find Smithy union {target}");
            };
            let representation = union_struct_representation(target, &union.members)?;
            (generate_struct_type(ctx, target, representation)?, None)
        }
        Some(
            ShapeKind::Blob
            | ShapeKind::Timestamp
            | ShapeKind::Service
            | ShapeKind::Operation
            | ShapeKind::Resource
            | ShapeKind::Unit,
        )
        | None => {
            // Fallback: try name-based heuristics
            if let Some(inferred) = infer_string_type(field_name) {
                (inferred, None)
            } else {
                (PLAIN_STRING_TYPE_CODE.to_string(), None)
            }
        }
    })
}

/// Generate Rust code for an AttributeType::Struct.
fn generate_struct_type(
    ctx: &mut TypeResolutionContext<'_>,
    struct_shape_id: &str,
    structure: StructRepresentation<'_>,
) -> Result<String> {
    let struct_name = SmithyModel::shape_name(struct_shape_id);
    ctx.struct_path.push(struct_name.to_string());
    let mut fields: Vec<String> = Vec::new();
    for (field_name, member_ref) in structure.members {
        let snake_name = field_name.to_snake_case();
        let is_required = structure.required_fields.contains(field_name.as_str());

        // Check `(parent_struct, member)` type override first — distinguishes
        // a member name shared across structs (e.g. `HostedZoneId` in
        // `AliasTarget` vs `RecordSet`). Falls through to the default
        // resolve_type path when no per-parent override is registered.
        let override_type = ctx
            .struct_field_type_overrides
            .get(&(struct_name, field_name.as_str()))
            .copied();

        let previous_path = ctx.property_path.clone();
        ctx.property_path =
            kms_references::property_path(&previous_path, PathSegment::Field(&snake_name));
        let (field_type, enum_info) =
            if let Some(code) = reference_type_code(ctx.resource_name, &ctx.property_path) {
                (code.to_string(), None)
            } else if let Some(override_type) =
                resource_value_override(ctx.resource_name, &ctx.property_path)
            {
                (
                    bound_int_override(ctx.model, &member_ref.target, field_name, override_type)
                        .type_code,
                    None,
                )
            } else if let Some(t) = override_type {
                (
                    bounded_int_constructor_code(ctx.model, &member_ref.target, field_name, t)?,
                    None,
                )
            } else {
                resolve_type(ctx, &member_ref.target, field_name)?
            };

        ctx.property_path = previous_path;

        // If enum detected, use shared schema enum type.
        let field_type = enum_info
            .as_ref()
            .map(|info| enum_type_code(ctx, field_name, info))
            .unwrap_or(field_type);

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
        let nested_write_only_key = StructMemberKey {
            parent_shape_id: struct_shape_id.to_string(),
            member_name: field_name.clone(),
        };
        if ctx
            .nested_write_only_struct_fields
            .contains(&nested_write_only_key)
        {
            field_code.push_str(".write_only()");
            *ctx.nested_write_only_emissions
                .entry(nested_write_only_key)
                .or_default() += 1;
        }
        if let Some(desc) = SmithyModel::documentation(&member_ref.traits) {
            field_code.push_str(&format!(
                ".with_description(\"{}\")",
                Description::from_smithy_html(desc).rust_string_literal(Some(150))
            ));
        }
        field_code.push_str(&format!(".with_provider_name(\"{}\")", field_name));
        field_code.push_str(".build()");
        fields.push(field_code);
    }

    let fields_str = fields.join(",\n                    ");
    ctx.struct_path.pop();
    let field_groups = &structure.field_groups;
    let groups = if field_groups.is_empty() {
        String::new()
    } else {
        format!(
            "                    vec![{}],\n",
            field_groups
                .iter()
                .map(field_group_code)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let constructor = if field_groups.is_empty() {
        "struct_"
    } else {
        "struct_with_field_groups"
    };
    Ok(format!(
        "AttributeType::{}(\n\
         \x20                   \"{}\".to_string(),\n\
         \x20                   vec![\n\
         \x20                   {}\n\
         \x20                   ],\n\
         {}\x20               )",
        constructor, struct_name, fields_str, groups
    ))
}

/// Intersect wire width, Smithy traits, and documented field bounds.
/// https://smithy.io/2.0/spec/simple-types.html#integer
fn get_int_range(model: &SmithyModel, target: &str, field_name: &str) -> Option<IntRange> {
    let mut bounds = Vec::new();
    if model.shape_kind(target) == Some(ShapeKind::Integer) {
        bounds.push(IntRange {
            min: Some(i64::from(i32::MIN)),
            max: Some(i64::from(i32::MAX)),
        });
    }
    let traits = match model.get_shape(target) {
        Some(Shape::Integer(shape)) => Some(&shape.traits),
        Some(Shape::Long(shape)) => Some(&shape.traits),
        _ => None,
    };
    if let Some(range) = traits.and_then(|traits| traits.get("smithy.api#range")) {
        bounds.push(IntRange {
            min: range.get("min").and_then(|value| value.as_i64()),
            max: range.get("max").and_then(|value| value.as_i64()),
        });
    }
    if let Some(&(min, max)) = known_int_range_overrides().get(field_name) {
        bounds.push(IntRange {
            min: Some(min),
            max: Some(max),
        });
    }
    bounds.into_iter().reduce(|left, right| IntRange {
        min: left.min.into_iter().chain(right.min).max(),
        max: left.max.into_iter().chain(right.max).min(),
    })
}

/// Overrides can refine or name an integer, but cannot widen its wire carrier.
fn bound_int_override(
    model: &SmithyModel,
    target: &str,
    field_name: &str,
    mut value: ValueTypeOverride,
) -> ValueTypeOverride {
    if !matches!(
        model.shape_kind(target),
        Some(ShapeKind::Integer | ShapeKind::Long)
    ) {
        return value;
    }
    let Some(bounds) = get_int_range(model, target, field_name) else {
        return value;
    };
    let RawShape::Int {
        identity, range, ..
    } = value.attribute_type.raw_shape()
    else {
        return value;
    };
    // A closed Int set already enforces the carrier when every member fits.
    // Preserve its constructor, callback and allowed-value documentation.
    if let Some(identity) = identity
        && let Some(values) = AwsType::ALL
            .iter()
            .find(|row| row.identity() == *identity)
            .and_then(|row| row.allowed_int_values())
        && values.iter().all(|value| {
            bounds.min.is_none_or(|minimum| *value >= minimum)
                && bounds.max.is_none_or(|maximum| *value <= maximum)
        })
    {
        return value;
    }
    let bounded =
        carina_aws_types::with_int_range(value.attribute_type.clone(), bounds.min, bounds.max);
    if let RawShape::Int {
        range: bounded_range,
        ..
    } = bounded.raw_shape()
        && range == bounded_range
    {
        return value;
    }

    value.type_code = format!(
        "carina_aws_types::with_int_range({}, {:?}, {:?})",
        value.type_code, bounds.min, bounds.max
    );
    value.attribute_type = bounded;
    value
}

fn bounded_int_constructor_code(
    model: &SmithyModel,
    target: &str,
    field_name: &str,
    code: &str,
) -> Result<String> {
    if !matches!(
        model.shape_kind(target),
        Some(ShapeKind::Integer | ShapeKind::Long)
    ) {
        return Ok(code.to_string());
    }
    Ok(bound_int_override(
        model,
        target,
        field_name,
        ValueTypeOverride {
            type_code: code.to_string(),
            documentation: None,
            attribute_type: attribute_type_from_code(code)?,
        },
    )
    .type_code)
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
         \x20   AbsoluteBindingAddress, ConcreteValue, DataSource, NoPlaceholder, ResourceId, SnapshotState, State, Value,\n\
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
        let update_input = send_input_type_name(res.name, "Update");
        let update_parts = send_parts_type_name(res.name, "Update");

        if res.has_tags {
            // Tag-enabled noop update: apply tags then read back
            code.push_str(&format!(
                "\x20   /// Update {}: apply tag changes and read back (generated)\n\
                 \x20   pub(crate) async fn {}(\n\
                 \x20       &self,\n\
                 \x20       id: ResourceId,\n\
                 \x20       identifier: &AppliedIdentifier,\n\
                 \x20       from: &SnapshotState,\n\
                 \x20       to: &crate::helpers::UpdateResource,\n\
                 \x20   ) -> ProviderResult<ProviderResult<ReadOutcome>> {{\n\
                 \x20       let input = crate::send_models_generated::{}::decode(to)?;\n\
                 \x20       let crate::send_models_generated::{} {{ tags }} = input.into_parts();\n\
                 \x20       let tags = tags.into_inner();\n\
                 \x20       self.apply_ec2_tags(&id, identifier.as_str(), tags, Some(&from.attributes))\n\
                 \x20           .await?;\n\
                 \x20       Ok(self.{}(&id, identifier).await)\n\
                 \x20   }}\n\n",
                res.name, method_name, update_input, update_parts, read_method,
            ));
        } else {
            code.push_str(&format!(
                "\x20   /// Update {} (no-op, just read back current state) (generated)\n\
                 \x20   pub(crate) async fn {}(\n\
                 \x20       &self,\n\
                 \x20       id: ResourceId,\n\
                 \x20       identifier: &AppliedIdentifier,\n\
                 \x20       to: &crate::helpers::UpdateResource,\n\
                 \x20   ) -> ProviderResult<ProviderResult<ReadOutcome>> {{\n\
                 \x20       let input = crate::send_models_generated::{}::decode(to)?;\n\
                 \x20       let crate::send_models_generated::{} = input.into_parts();\n\
                 \x20       Ok(self.{}(&id, identifier).await)\n\
                 \x20   }}\n\n",
                res.name, method_name, update_input, update_parts, read_method,
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
             \x20       resource: &DataSource<impl Clone + Send + Sync + 'static, NoPlaceholder, AbsoluteBindingAddress, impl Clone + Send + Sync + 'static>,\n\
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
         \x20   resource: &'a DataSource<impl Clone + Send + Sync + 'static, NoPlaceholder, AbsoluteBindingAddress, impl Clone + Send + Sync + 'static>,\n\
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

#[derive(Clone)]
struct SendInputField {
    snake_name: String,
    provider_name: String,
    capture_names: &'static [&'static str],
    via: resource_defs::SendVia,
    is_required: bool,
    is_create_only: bool,
    send_type: SendFieldType,
    sdk_i32: bool,
    sdk_i32_paths: Vec<Vec<String>>,
}

fn send_input_type_name(resource_name: &str, operation: &str) -> String {
    let (service, resource) = split_service_resource(resource_name);
    format!(
        "{}{}{}Input",
        service.to_upper_camel_case(),
        resource,
        operation
    )
}

fn send_parts_type_name(resource_name: &str, operation: &str) -> String {
    send_input_type_name(resource_name, operation).replace("Input", "Parts")
}

/// Prefer authored DSL paths; otherwise walk the Smithy structure that codegen
/// renders. Lists and maps use explicit markers so Long siblings stay untouched.
fn nested_sdk_i32_paths(model: &SmithyModel, target: &str, constructor: &str) -> Vec<Vec<String>> {
    fn authored(ty: &AttributeType, path: &mut Vec<String>, paths: &mut Vec<Vec<String>>) {
        match ty.raw_shape() {
            RawShape::Int {
                identity, range, ..
            } => {
                let bounded = range.is_some_and(|(min, max)| {
                    min.is_some_and(|min| min >= i64::from(i32::MIN))
                        && max.is_some_and(|max| max <= i64::from(i32::MAX))
                });
                let closed = identity
                    .and_then(|identity| {
                        AwsType::ALL.iter().find(|row| row.identity() == *identity)
                    })
                    .and_then(|row| row.allowed_int_values())
                    .is_some_and(|values| values.iter().all(|value| i32::try_from(*value).is_ok()));
                if bounded || closed {
                    paths.push(path.clone());
                }
            }
            RawShape::Struct { fields, .. } => {
                for field in fields {
                    path.push(field.name().to_string());
                    authored(field.field_type(), path, paths);
                    path.pop();
                }
            }
            RawShape::List { element_type, .. } => {
                path.push("[]".to_string());
                authored(element_type, path, paths);
                path.pop();
            }
            RawShape::Map { value, .. } => {
                path.push("{}".to_string());
                authored(value, path, paths);
                path.pop();
            }
            RawShape::Union(members) => {
                for member in members {
                    authored(member, path, paths);
                }
            }
            _ => {}
        }
    }
    fn smithy(
        model: &SmithyModel,
        target: &str,
        path: &mut Vec<String>,
        paths: &mut Vec<Vec<String>>,
        visiting: &mut HashSet<String>,
    ) {
        if matches!(
            model.shape_kind(target),
            Some(ShapeKind::Integer | ShapeKind::IntEnum)
        ) {
            paths.push(path.clone());
            return;
        }
        if !visiting.insert(target.to_string()) {
            return;
        }
        match model.get_shape(target) {
            Some(Shape::Structure(shape)) => {
                for (name, member) in &shape.members {
                    path.push(name.to_snake_case());
                    smithy(model, &member.target, path, paths, visiting);
                    path.pop();
                }
            }
            Some(Shape::Union(shape)) => {
                for (name, member) in &shape.members {
                    path.push(name.to_snake_case());
                    smithy(model, &member.target, path, paths, visiting);
                    path.pop();
                }
            }
            Some(Shape::List(shape)) => {
                path.push("[]".to_string());
                smithy(model, &shape.member.target, path, paths, visiting);
                path.pop();
            }
            Some(Shape::Map(shape)) => {
                path.push("{}".to_string());
                smithy(model, &shape.value.target, path, paths, visiting);
                path.pop();
            }
            _ => {}
        }
        visiting.remove(target);
    }
    let mut paths = Vec::new();
    if let Ok(ty) = attribute_type_from_code(constructor) {
        authored(&ty, &mut Vec::new(), &mut paths);
    } else {
        smithy(
            model,
            target,
            &mut Vec::new(),
            &mut paths,
            &mut HashSet::new(),
        );
    }
    paths.sort();
    paths.dedup();
    paths
}

fn send_field_rust_type(field: &SendInputField) -> String {
    let inner = match field.send_type {
        SendFieldType::String | SendFieldType::Enum => "String",
        SendFieldType::StringList => "crate::send_model::StringList",
        SendFieldType::Tags => "crate::send_model::Tags",
        SendFieldType::Bool => "bool",
        SendFieldType::Int => {
            if field.sdk_i32 {
                "i32"
            } else {
                "i64"
            }
        }
        SendFieldType::Float => "f64",
        SendFieldType::Value => "Value",
    };
    if field.is_required {
        inner.to_string()
    } else {
        format!("Option<{inner}>")
    }
}

fn send_decode_expression(field: &SendInputField) -> String {
    let requiredness = if field.is_required {
        "required"
    } else {
        "optional"
    };
    let kind = match field.send_type {
        SendFieldType::String => "string",
        SendFieldType::StringList => "string_list",
        SendFieldType::Tags => "tags",
        SendFieldType::Enum => "enum",
        SendFieldType::Bool => "bool",
        SendFieldType::Int => {
            if field.sdk_i32 {
                "i32"
            } else {
                "int"
            }
        }
        SendFieldType::Float => "float",
        SendFieldType::Value => "value",
    };
    let schema_arg = if field.send_type == SendFieldType::Enum {
        "_schema, "
    } else {
        ""
    };
    let decoded = format!(
        "crate::send_model::{requiredness}_{kind}(_resource, {schema_arg}\"{}\")",
        field.snake_name
    );
    if field.sdk_i32_paths.is_empty() {
        return decoded;
    }
    let paths = field
        .sdk_i32_paths
        .iter()
        .map(|path| format!("&{path:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let value_argument = if field.is_required { "&value" } else { "value" };
    let check = format!(
        "crate::send_model::check_i32_value(_resource, {:?}, {value_argument}, &[{paths}])?;",
        field.snake_name
    );
    if field.is_required {
        format!("{decoded}.and_then(|value| {{ {check} Ok(value) }})")
    } else {
        format!(
            "{decoded}.and_then(|value| {{ if let Some(value) = &value {{ {check} }} Ok(value) }})"
        )
    }
}

fn emit_send_input(
    code: &mut String,
    res: &ResourceDef,
    operation: &str,
    fields: &[SendInputField],
) {
    let type_name = send_input_type_name(res.name, operation);
    let parts_name = send_parts_type_name(res.name, operation);
    let is_create = operation == "Create";
    let field_type = |field: &SendInputField| {
        let value_type = send_field_rust_type(field);
        if is_create || field.is_required {
            value_type
        } else {
            format!("crate::helpers::AttributeChange<{value_type}>")
        }
    };
    let (service, resource) = split_service_resource(res.name);
    let resource_module = pascal_to_snake(resource);
    let config_fn = format!("{}_config", module_name(res.name));
    code.push_str(&format!(
        "/// Exhaustive typed {} input for `{}`.\n",
        operation.to_lowercase(),
        res.name
    ));
    if fields.is_empty() {
        code.push_str(&format!("pub(crate) struct {type_name};\n\n"));
    } else {
        code.push_str(&format!("pub(crate) struct {type_name} {{\n"));
        for field in fields {
            code.push_str(&format!(
                "    {}: {},\n",
                escape_rust_keyword(&field.snake_name),
                field_type(field)
            ));
        }
        code.push_str("}\n\n");
    }

    if fields.is_empty() {
        code.push_str(&format!("pub(crate) struct {parts_name};\n\n"));
    } else {
        code.push_str(&format!("pub(crate) struct {parts_name} {{\n"));
        for field in fields {
            code.push_str(&format!(
                "    pub(crate) {}: crate::send_model::Sent<{}>,\n",
                escape_rust_keyword(&field.snake_name),
                field_type(field)
            ));
        }
        code.push_str("}\n\n");
    }

    code.push_str(&format!("impl {type_name} {{\n"));
    code.push_str("    pub(crate) fn decode(\n");
    if is_create {
        code.push_str("        _resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,\n");
    } else {
        code.push_str("        _resource: &crate::helpers::UpdateResource,\n");
    }
    let needs_schema = fields
        .iter()
        .any(|field| field.send_type == SendFieldType::Enum);
    if needs_schema {
        code.push_str("        _schema: &carina_core::schema::CompiledResourceSchema,\n");
    }
    code.push_str("    ) -> ProviderResult<Self> {\n");
    if !needs_schema {
        code.push_str(&format!(
            "        let _schema = &crate::schemas::generated::{service}::{resource_module}::{config_fn}().schema;\n"
        ));
    }
    if fields.is_empty() {
        code.push_str("        Ok(Self)\n");
    } else {
        code.push_str("        Ok(Self {\n");
        for field in fields {
            code.push_str(&format!(
                "            {}: {},\n",
                escape_rust_keyword(&field.snake_name),
                if is_create {
                    format!("{}?", send_decode_expression(field))
                } else {
                    let decoder = send_decode_expression(field);
                    let decode = if field.is_required {
                        "decode_required"
                    } else {
                        "decode_change"
                    };
                    format!(
                        "crate::send_model::{decode}(_resource, {:?}, |_resource| {decoder})?",
                        field.snake_name
                    )
                }
            ));
        }
        code.push_str("        })\n");
    }
    code.push_str("    }\n\n");

    if is_create {
        code.push_str("    pub(crate) fn sent(&self) -> CreateSent {\n");
        code.push_str("        let mut sent = BTreeSet::new();\n");
        for field in fields {
            if field.via != resource_defs::SendVia::CreateRequest {
                continue;
            }
            if field.is_required {
                code.push_str(&format!(
                    "        sent.insert(\"{}\".to_string());\n",
                    field.snake_name
                ));
            } else {
                let ident = escape_rust_keyword(&field.snake_name);
                code.push_str(&format!(
                    "        if self.{ident}.is_some() {{ sent.insert(\"{}\".to_string()); }}\n",
                    field.snake_name
                ));
            }
        }
        code.push_str("        CreateSent::only(sent)\n");
        code.push_str("    }\n\n");
    }

    code.push_str(&format!(
        "    pub(crate) fn into_parts(self) -> {parts_name} {{\n"
    ));
    if fields.is_empty() {
        code.push_str("        let Self = self;\n");
        code.push_str(&format!("        {parts_name}\n"));
    } else {
        code.push_str("        let Self {\n");
        for field in fields {
            code.push_str(&format!(
                "            {},\n",
                escape_rust_keyword(&field.snake_name)
            ));
        }
        code.push_str("        } = self;\n");
        code.push_str(&format!("        {parts_name} {{\n"));
        for field in fields {
            let ident = escape_rust_keyword(&field.snake_name);
            code.push_str(&format!(
                "            {ident}: crate::send_model::Sent::new(\"{}.{}.{}\", {ident}),\n",
                res.name,
                operation.to_lowercase(),
                field.snake_name,
            ));
        }
        code.push_str("        }\n");
    }
    code.push_str("    }\n");
    code.push_str("}\n\n");
}

/// A service that updates struct members individually must match this closed
/// model-derived field set when declaring resets. A new Smithy member then
/// makes that service's reset match non-exhaustive at compile time.
fn emit_update_field_enum(
    code: &mut String,
    model: &SmithyModel,
    shape_id: &str,
    name: &str,
) -> Result<()> {
    let shape = model
        .get_structure(shape_id)
        .with_context(|| format!("missing nested update shape {shape_id}"))?;
    code.push_str(&format!(
        "#[derive(Clone, Copy)]\npub(crate) enum {name} {{\n"
    ));
    for field in shape.members.keys() {
        code.push_str(&format!("    {},\n", field.to_upper_camel_case()));
    }
    code.push_str(&format!(
        "}}\nimpl {name} {{\n    pub(crate) const ALL: &'static [Self] = &[\n"
    ));
    for field in shape.members.keys() {
        code.push_str(&format!("        Self::{},\n", field.to_upper_camel_case()));
    }
    code.push_str("    ];\n    pub(crate) fn name(self) -> &'static str {\n        match self {\n");
    for field in shape.members.keys() {
        code.push_str(&format!(
            "            Self::{} => {:?},\n",
            field.to_upper_camel_case(),
            field.to_snake_case()
        ));
    }
    code.push_str("        }\n    }\n}\n\n");
    Ok(())
}

fn generate_send_models_code(
    resources: &[(&ResourceDef, &[resource_defs::SendDisposition])],
    models: &HashMap<&str, SmithyModel>,
) -> Result<String> {
    let mut code = String::from(
        "//! Auto-generated exhaustive managed-resource send inputs.\n\
         //!\n\
         //! DO NOT EDIT MANUALLY - regenerate with:\n\
         //!   ./scripts/generate-provider.sh\n\n\
         use std::collections::BTreeSet;\n\n\
         use carina_core::provider::{ProviderResult, Sent as CreateSent};\n\
         use carina_core::resource::{AbsoluteBindingAddress, NoPlaceholder, Resource, Value};\n\n",
    );
    let mut capture_contracts = Vec::new();

    for (res, dispositions) in resources {
        let model = models.get(res.service_namespace).with_context(|| {
            format!(
                "Missing Smithy model {} for {}",
                res.service_namespace, res.name
            )
        })?;
        for declaration in res.nested_update_field_enums {
            emit_update_field_enum(
                &mut code,
                model,
                &format!("{}#{}", res.service_namespace, declaration.structure),
                declaration.rust_name,
            )?;
        }
        let read_dispositions = res.read_dispositions();
        let mut collected =
            collect_resource_attributes_unvalidated(res, model, &read_dispositions)?;
        apply_read_contract(res, model, &mut collected.attrs, &read_dispositions)?;
        apply_send_contract(res, &mut collected.attrs, dispositions)?;

        let mut create_fields = collected
            .attrs
            .iter()
            .filter(|attr| !attr.is_read_only && attr.is_sent)
            .map(|attr| SendInputField {
                snake_name: attr.snake_name.clone(),
                provider_name: attr.provider_name.clone(),
                capture_names: dispositions
                    .iter()
                    .find(|disposition| {
                        disposition.field().to_snake_case() == attr.provider_name.to_snake_case()
                    })
                    .map(|disposition| disposition.capture_names())
                    .unwrap_or_default(),
                via: attr.send_via.expect("Sent attributes have a send phase"),
                is_required: attr.is_required,
                is_create_only: attr.is_create_only,
                send_type: attr.send_type,
                sdk_i32: attr.sdk_i32,
                sdk_i32_paths: attr.sdk_i32_paths.clone(),
            })
            .collect::<Vec<_>>();
        let tags_sent = res.has_tags
            && dispositions.iter().any(|disposition| {
                disposition.field().to_snake_case() == "tags"
                    && matches!(disposition, resource_defs::SendDisposition::Sent { .. })
            });
        if tags_sent {
            let disposition = dispositions
                .iter()
                .find(|disposition| disposition.field().to_snake_case() == "tags")
                .expect("classified tags");
            create_fields.push(SendInputField {
                snake_name: "tags".to_string(),
                provider_name: "Tags".to_string(),
                capture_names: disposition.capture_names(),
                via: disposition.via().expect("Sent tags have a send phase"),
                is_required: false,
                is_create_only: false,
                send_type: SendFieldType::Tags,
                sdk_i32: false,
                sdk_i32_paths: Vec::new(),
            });
        }
        create_fields.sort_by(|left, right| left.snake_name.cmp(&right.snake_name));
        let update_fields = create_fields
            .iter()
            .filter(|field| !field.is_create_only)
            .cloned()
            .collect::<Vec<_>>();

        emit_send_input(&mut code, res, "Create", &create_fields);
        emit_send_input(&mut code, res, "Update", &update_fields);
        let render_attributes = |fields: &[SendInputField]| {
            fields
                .iter()
                .map(|field| {
                    let capture_names = field
                        .capture_names
                        .iter()
                        .map(|name| format!("\"{name}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "SendCaptureAttribute {{ name: \"{}\", provider_name: \"{}\", capture_names: &[{}] }}",
                        field.snake_name, field.provider_name, capture_names
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let create_attributes = render_attributes(&create_fields);
        let update_attributes = render_attributes(&update_fields);
        capture_contracts.push(format!(
            "    SendCaptureContract {{ resource_type: \"{}\", create_attributes: &[{}], update_attributes: &[{}] }},\n",
            res.name, create_attributes, update_attributes
        ));
    }

    code.push_str(
        "#[cfg(test)]\n\
         pub(crate) struct SendCaptureAttribute {\n\
             pub(crate) name: &'static str,\n\
             pub(crate) provider_name: &'static str,\n\
             pub(crate) capture_names: &'static [&'static str],\n\
         }\n\n\
         #[cfg(test)]\n\
         pub(crate) struct SendCaptureContract {\n\
             pub(crate) resource_type: &'static str,\n\
             pub(crate) create_attributes: &'static [SendCaptureAttribute],\n\
             pub(crate) update_attributes: &'static [SendCaptureAttribute],\n\
         }\n\n\
         #[cfg(test)]\n\
         pub(crate) fn capture_contracts() -> &'static [SendCaptureContract] {\n\
             &[\n",
    );
    for contract in capture_contracts {
        code.push_str(&contract);
    }
    code.push_str("    ]\n}\n");

    Ok(code)
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

/// Get the SDK facade path from a service namespace.
/// e.g., "com.amazonaws.s3" -> "carina_aws_types::sdk::s3"
fn sdk_crate_name(service_namespace: &str) -> String {
    let service = service_namespace
        .strip_prefix("com.amazonaws.")
        .unwrap_or(service_namespace);
    format!("carina_aws_types::sdk::{}", service)
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
    AttributeType(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MarkdownEnumKey {
    namespace: String,
    heading_path: HeadingPath,
    source: MarkdownEnumSource,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum MarkdownStructSource {
    AttributeType(String),
    // The heading scope and typed path identify a union without inspecting its
    // contents. Tree IDs distinguish separately discovered override trees.
    AttributeUnion { tree: usize, path: HeadingPath },
    Smithy(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MarkdownStructKey {
    namespace: String,
    heading_path: HeadingPath,
    source: MarkdownStructSource,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum MarkdownTarget {
    Enum(MarkdownEnumKey),
    Struct(MarkdownStructKey),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum MarkdownNodeId {
    Override { tree: usize, address: usize },
    Smithy(MarkdownTarget),
}

#[derive(Default)]
struct MarkdownTypes {
    definitions: BTreeMap<MarkdownStructKey, MarkdownStruct>,
    // One registry for every type render on this page, including Smithy links
    // and fields of separately queued override declarations.
    registry: TypeRegistry<MarkdownNodeId, MarkdownTarget>,
    // Override trees are temporary allocations. Namespace their node addresses
    // so later trees cannot inherit registrations after allocator address reuse.
    next_tree: usize,
}

impl std::ops::Deref for MarkdownTypes {
    type Target = BTreeMap<MarkdownStructKey, MarkdownStruct>;

    fn deref(&self) -> &Self::Target {
        &self.definitions
    }
}

impl std::ops::DerefMut for MarkdownTypes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.definitions
    }
}

type MarkdownType = TypeDisplay<MarkdownTarget>;

#[derive(Debug)]
struct MarkdownEnum {
    prop_name: String,
    info: EnumInfo,
    dsl_aliases: Vec<(String, String)>,
}

#[derive(Debug)]
struct MarkdownStruct {
    union_members: Option<Vec<MarkdownType>>,
    fields: Vec<MarkdownStructField>,
    // Captured from the shared union mapping or the hand-written type's raw shape.
    field_groups: Vec<FieldGroup>,
}

#[derive(Debug)]
struct MarkdownStructField {
    snake_name: String,
    field_type: MarkdownType,
    is_required: bool,
    description: Option<Description>,
}

#[derive(Debug)]
struct MarkdownAttrInfo {
    snake_name: String,
    attr_type: MarkdownType,
    is_required: bool,
    description: Option<Description>,
}

fn declared_scalar_display_name(identity: &carina_core::schema::TypeIdentity) -> String {
    AwsType::ALL
        .iter()
        .find(|ty| ty.identity() == *identity)
        .map(|ty| type_code_to_display(ty.constructor_path()))
        .unwrap_or_else(|| identity.kind.clone())
}

fn discover_markdown_override(
    code: &str,
    heading_path: &HeadingPath,
    namespace: &str,
    enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    structs: &mut MarkdownTypes,
) -> Result<MarkdownType> {
    let attr_type = attribute_type_from_code(code)?;
    discover_markdown_attribute_type(&attr_type, heading_path, namespace, enums, structs, None)
}

#[derive(Clone)]
struct MarkdownScope {
    heading_path: HeadingPath,
    namespace: String,
}

struct MarkdownHooks<'store, 'a> {
    tree: usize,
    enums: &'store mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    structs: &'store mut BTreeMap<MarkdownStructKey, MarkdownStruct>,
    pending: Vec<(MarkdownStructKey, MarkdownScope, DeclarationKind<'a, Draft>)>,
}

impl<'a> TypeDocHooks<'a, Draft> for MarkdownHooks<'_, 'a> {
    type NodeId = MarkdownNodeId;
    type Target = MarkdownTarget;
    type Scope = MarkdownScope;

    fn resolve(&self, ty: &'a AttributeType) -> Result<Resolved<'a, Draft, MarkdownNodeId>> {
        if let RawShape::Ref(name) = ty.raw_shape() {
            anyhow::bail!("cannot document unresolved schema type {name}");
        }
        Ok(Resolved {
            id: MarkdownNodeId::Override {
                tree: self.tree,
                address: ty as *const AttributeType as usize,
            },
            ty,
        })
    }

    fn scalar_name(&self, identity: &TypeIdentity) -> String {
        declared_scalar_display_name(identity)
    }

    fn child_scope(&self, parent: &MarkdownScope, edge: TypeEdge<'_>) -> MarkdownScope {
        let mut scope = parent.clone();
        scope.heading_path = scope.heading_path.child(edge);
        scope
    }

    fn register(
        &mut self,
        scope: &MarkdownScope,
        declaration: Declaration<'a, Draft, MarkdownNodeId>,
    ) -> Result<DocLink<MarkdownTarget>> {
        let heading_path = &scope.heading_path;
        match declaration.kind {
            DeclarationKind::Enum {
                identity,
                values,
                dsl_aliases,
                ..
            } => {
                let namespace = identity
                    .dotted_prefix()
                    .unwrap_or_else(|| scope.namespace.clone());
                let key = register_markdown_enum(
                    MarkdownEnumSource::AttributeType(identity.to_string()),
                    heading_path.last_field().unwrap_or(&identity.kind),
                    &identity.kind,
                    heading_path,
                    &namespace,
                    values.to_vec(),
                    self.enums,
                );
                self.enums
                    .get_mut(&key)
                    .expect("registered enum")
                    .dsl_aliases = dsl_aliases.to_vec();
                Ok(DocLink {
                    target: MarkdownTarget::Enum(key),
                    label: identity.kind.clone(),
                })
            }
            DeclarationKind::Struct {
                name,
                fields,
                field_groups,
            } => {
                let mut child = scope.clone();
                child.namespace = format!("{}.{name}", scope.namespace);
                let key = MarkdownStructKey {
                    namespace: child.namespace.clone(),
                    heading_path: heading_path.clone(),
                    source: MarkdownStructSource::AttributeType(name.to_string()),
                };
                if !self.structs.contains_key(&key) {
                    self.structs.insert(
                        key.clone(),
                        MarkdownStruct {
                            fields: Vec::new(),
                            field_groups: field_groups.to_vec(),
                            union_members: None,
                        },
                    );
                    self.pending.push((
                        key.clone(),
                        child,
                        DeclarationKind::Struct {
                            name,
                            fields,
                            field_groups,
                        },
                    ));
                }
                Ok(DocLink {
                    target: MarkdownTarget::Struct(key),
                    label: name.to_string(),
                })
            }
            DeclarationKind::Union { members } => {
                let label = heading_path
                    .last_field()
                    .map(|name| name.to_upper_camel_case())
                    .unwrap_or_else(|| "Value".into());
                let key = MarkdownStructKey {
                    namespace: scope.namespace.clone(),
                    heading_path: heading_path.clone(),
                    source: MarkdownStructSource::AttributeUnion {
                        tree: self.tree,
                        path: heading_path.clone(),
                    },
                };
                if !self.structs.contains_key(&key) {
                    self.structs.insert(
                        key.clone(),
                        MarkdownStruct {
                            fields: Vec::new(),
                            field_groups: Vec::new(),
                            union_members: Some(Vec::new()),
                        },
                    );
                    self.pending.push((
                        key.clone(),
                        scope.clone(),
                        DeclarationKind::Union { members },
                    ));
                }
                Ok(DocLink {
                    target: MarkdownTarget::Struct(key),
                    label,
                })
            }
            DeclarationKind::Recursive => {
                anyhow::bail!("AWS attribute overrides cannot contain unresolved recursive aliases")
            }
        }
    }
}

/// AWS owns declaration layout and scope; all shape traversal and type wording
/// live in carina-docgen. Targets are reserved before queued children are read.
fn discover_markdown_attribute_type(
    attr_type: &AttributeType,
    heading_path: &HeadingPath,
    namespace: &str,
    enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    structs: &mut MarkdownTypes,
    note: Option<ValidationNote<'_>>,
) -> Result<MarkdownType> {
    let tree = structs.next_tree;
    structs.next_tree += 1;
    let MarkdownTypes {
        definitions,
        registry,
        ..
    } = structs;
    let mut hooks = MarkdownHooks {
        tree,
        enums,
        structs: definitions,
        pending: Vec::new(),
    };
    let scope = MarkdownScope {
        heading_path: heading_path.clone(),
        namespace: namespace.to_string(),
    };
    let display = render_attribute_type(attr_type, &scope, &mut hooks, registry, note)?;
    while let Some((key, scope, kind)) = hooks.pending.pop() {
        match kind {
            DeclarationKind::Struct { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        let child = hooks.child_scope(&scope, TypeEdge::Field(field.name()));
                        Ok(MarkdownStructField {
                            snake_name: field.name().to_string(),
                            field_type: render_attribute_type(
                                field.field_type(),
                                &child,
                                &mut hooks,
                                registry,
                                None,
                            )?,
                            is_required: field.parts().declared_input_mode.is_required(),
                            description: field.description().map(Description::authored),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                hooks.structs.get_mut(&key).expect("reserved struct").fields = fields;
            }
            DeclarationKind::Union { members } => {
                let members = members
                    .iter()
                    .enumerate()
                    .map(|(index, member)| {
                        let child = hooks.child_scope(&scope, TypeEdge::Member(index));
                        render_attribute_type(member, &child, &mut hooks, registry, None)
                    })
                    .collect::<Result<Vec<_>>>()?;
                hooks
                    .structs
                    .get_mut(&key)
                    .expect("reserved union")
                    .union_members = Some(members);
            }
            DeclarationKind::Enum { .. } | DeclarationKind::Recursive => {
                unreachable!("only compound definitions are queued")
            }
        }
    }
    Ok(display)
}

/// Generate markdown documentation for a single resource.
fn generate_markdown_resource(
    res: &ResourceDef,
    model: &SmithyModel,
    example: &DocExample,
) -> Result<String> {
    let example = example.text_for("aws", res.name, ExampleKind::Resource)?;
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
    let mut struct_defs = MarkdownTypes::default();
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
            Some(Description::authored(desc))
        } else {
            SmithyModel::documentation(&member_ref.traits).map(Description::from_smithy_html)
        };
        validate_smithy_type(model, res.name, &snake_name, name, member_ref)?;
        let attr_type = discover_markdown_type(
            model,
            res.name,
            &member_ref.target,
            name,
            &HeadingPath::root(&snake_name),
            &snake_name,
            &namespace,
            &type_overrides,
            &struct_field_type_overrides,
            None,
            &mut all_enums,
            &mut struct_defs,
            &mut discovering_structs,
        )?;

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
        carina_codegen_aws::kms_references::validate_synthetic_type(model, res, extra)?;
        let attr_type = synthetic_extra_attribute_type(res.name, extra.name, &type_overrides)?
            .markdown(
                &HeadingPath::root(&snake_name),
                &namespace,
                &mut all_enums,
                &mut struct_defs,
            )?;
        let entry = MarkdownAttrInfo {
            snake_name: snake_name.clone(),
            attr_type,
            is_required: required_overrides.contains(extra.name)
                && !read_only_overrides.contains(extra.name),
            description: extra.description.map(Description::authored),
        };
        if read_only_overrides.contains(extra.name) {
            read_only_attrs.push(entry);
        } else {
            writable_attrs.push(entry);
        }
    }

    for (name, member_ref) in &read_only_fields {
        let snake_name = name.to_snake_case();
        let description =
            SmithyModel::documentation(&member_ref.traits).map(Description::from_smithy_html);
        validate_smithy_type(model, res.name, &snake_name, name, member_ref)?;
        let attr_type = if let Some(override_type) = resource_value_override(res.name, &snake_name)
        {
            override_type.markdown(
                &HeadingPath::root(&snake_name),
                &namespace,
                &mut all_enums,
                &mut struct_defs,
            )?
        } else if let Some(constructor) = (name == "Arn")
            .then(|| {
                resource_identity_parts(res.name)
                    .and_then(|(service, resource)| shared_arn_constructor(service, resource))
            })
            .flatten()
        {
            discover_markdown_override(
                &format!("super::{constructor}()"),
                &HeadingPath::root(&snake_name),
                &namespace,
                &mut all_enums,
                &mut struct_defs,
            )?
        } else {
            discover_markdown_type(
                model,
                res.name,
                &member_ref.target,
                name,
                &HeadingPath::root(&snake_name),
                &snake_name,
                &namespace,
                &type_overrides,
                &struct_field_type_overrides,
                None,
                &mut all_enums,
                &mut struct_defs,
                &mut discovering_structs,
            )?
        };

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
            .filter(|key| !matches!(key.source, MarkdownEnumSource::AttributeType(..)))
            .map(|key| &key.heading_path)
            .chain(
                struct_defs
                    .keys()
                    .filter(|key| matches!(key.source, MarkdownStructSource::Smithy(_)))
                    .map(|key| &key.heading_path),
            ),
    )
    .with_context(|| format!("validating Markdown heading paths for aws.{}", res.name))?;

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
        let description = Description::from_smithy_html(desc);
        page.push_str(&format!("{}\n\n", description.markdown()));
    }

    if resource_defs::defaults_to_prevent_destroy(res.name) {
        page.push_str(
            "This resource is protected from destruction by default. To destroy it, explicitly set `prevent_destroy = false`.\n\n",
        );
    }

    let read_back = res.read_back_overrides();
    if let Some(documentation) = read_back.budget_markdown() {
        page.push_str(&documentation);
    }
    if !read_back.await_attributes.is_empty() {
        page.push_str("After create or update, the host polls for ");
        for (index, requirement) in read_back.await_attributes.iter().enumerate() {
            if index > 0 {
                page.push_str(", ");
            }
            page.inline(TableCell::code(requirement.attribute));
        }
        page.push_str(" when DNS validation is requested.\n\n");
    }

    for entry in carina_codegen_aws::kms_references::CLASSIFICATIONS
        .iter()
        .filter(|entry| {
            entry.resource == res.name
                && matches!(
                    entry.classification,
                    carina_codegen_aws::kms_references::Classification::Reference { .. }
                )
        })
    {
        page.inline(TableCell::code(entry.path));
        if matches!(
            entry.classification,
            carina_codegen_aws::kms_references::Classification::Reference { arn_only: true }
        ) {
            page.push_str(" uses `KmsKeyReference` and accepts key ARNs only. Key ARNs need no lookup. See [KMS reference planning permissions](https://carina-rs.dev/guides/aws-credentials/#kms-reference-planning-permissions).\n\n");
            continue;
        }
        page.push_str(" uses `KmsKeyReference`; during planning, key IDs, alias names, and alias ARNs may need `kms:DescribeKey` for the provider's effective identity and authorization in the key policy, only when the authored value differs from the value the service returns. Key ARNs never need a lookup; resolving an AWS-managed alias can initialize its key during planning (see [KMS reference planning permissions](https://carina-rs.dev/guides/aws-credentials/#kms-reference-planning-permissions)).\n\n");
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
            page.push_str(&format!("{}\n\n", desc.markdown()));
        }
    }

    // Tags
    if res.has_tags {
        page.heading(Heading::code(3, "tags"))?;
        page.push_str("- **Type:** ");
        discover_markdown_attribute_type(
            &resource_tag_type(res, model)?.attribute_type(),
            &HeadingPath::root("tags"),
            &namespace,
            &mut all_enums,
            &mut struct_defs,
            None,
        )?
        .write_to(&mut page);
        page.push_str("\n");
        page.push_str("- **Required:** No\n\n");
        page.push_str(&format!(
            "{}\n\n",
            Description::authored("The tags for the resource.").markdown()
        ));
    }

    write_markdown_type_definitions(&mut page, &all_enums, &struct_defs, &enum_alias_map)?;

    // Attribute Reference (read-only)
    if !read_only_attrs.is_empty() {
        page.heading(Heading::plain(2, "Attribute Reference"))?;
        for attr in &read_only_attrs {
            let description = attr.description.as_ref().map(Description::markdown);
            page.attribute_reference_entry(
                &attr.snake_name,
                attr.attr_type.table_cell(),
                description.as_deref(),
            )?;
        }
    }

    page.finish()
        .with_context(|| format!("generated invalid Markdown for aws.{}", res.name))
}

fn write_markdown_type_definitions(
    page: &mut PageBuilder<MarkdownTarget>,
    all_enums: &BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    struct_defs: &BTreeMap<MarkdownStructKey, MarkdownStruct>,
    enum_alias_map: &HashMap<&str, Vec<(&str, &str)>>,
) -> Result<()> {
    let union_heading_paths = markdown_union_heading_paths(all_enums, struct_defs)?;
    let enum_heading_texts = markdown_enum_heading_texts(all_enums);
    let struct_heading_texts = markdown_struct_heading_texts(struct_defs, &union_heading_paths);
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
                if let Some(entry) = enum_doc.dsl_aliases.iter().find(|entry| entry.0 == value) {
                    return entry.1.clone();
                }
                dsl_enum_value(value)
            };

            page.target_heading(
                MarkdownTarget::Enum(key.clone()),
                Heading::plain(3, heading_text),
            )?;
            let rows = enum_info.values.iter().map(|value| {
                let dsl_value = dsl_for(value);
                let dsl_id = format!("{}.{}.{}", key.namespace, enum_info.type_name, dsl_value);
                [TableCell::code(value), TableCell::code(dsl_id)]
            });
            page.table(["Value", "DSL Identifier"], rows)?;
            page.push_str("\n");

            let first_value = enum_info.values.first().map(|s| s.as_str()).unwrap_or("");
            let first_dsl = dsl_for(first_value);
            let mut shorthand = TableCell::text("Shorthand formats: ");
            shorthand.push_code(&first_dsl);
            shorthand.push_text(" or ");
            shorthand.push_code(format!("{}.{first_dsl}", enum_info.type_name));
            page.inline(shorthand);
            page.push_str("\n\n");
        }
    }

    // Struct Definitions section
    if !struct_heading_texts.is_empty() {
        page.heading(Heading::plain(2, "Struct Definitions"))?;
        for (key, heading_text) in &struct_heading_texts {
            let struct_doc = struct_defs
                .get(key)
                .expect("every struct heading must refer to a discovered struct");
            page.target_heading(
                MarkdownTarget::Struct(key.clone()),
                Heading::plain(3, heading_text),
            )?;
            render_field_groups(page, &struct_doc.field_groups);
            let rows = struct_doc.fields.iter().map(|field| {
                [
                    TableCell::code(&field.snake_name),
                    field.field_type.table_cell(),
                    TableCell::text(if field.is_required { "Yes" } else { "No" }),
                    TableCell::text(
                        field
                            .description
                            .as_ref()
                            .map(Description::markdown)
                            .unwrap_or_default(),
                    ),
                ]
            });
            page.table(["Field", "Type", "Required", "Description"], rows)?;
            page.push_str("\n");
        }
    }

    // Union definitions are intentionally minimal and use their own headings,
    // so adding a union cannot rename any existing struct or enum anchor.
    let mut unions = struct_defs
        .iter()
        .filter(|(_, doc)| doc.union_members.is_some())
        .peekable();
    if unions.peek().is_some() {
        page.heading(Heading::plain(2, "Union Definitions"))?;
        let mut used_slugs = HashSet::new();
        for (key, doc) in unions {
            let heading = claim_unique_heading(
                format!("{} union", key.heading_path),
                &key.namespace,
                &mut used_slugs,
            );
            page.target_heading(
                MarkdownTarget::Struct(key.clone()),
                Heading::plain(3, heading),
            )?;
            for member in doc.union_members.as_ref().expect("union members") {
                page.push_str("- ");
                member.write_to(page);
                page.push_str("\n");
            }
            page.push_str("\n");
        }
    }

    Ok(())
}

fn write_markdown_example(page: &mut PageBuilder<MarkdownTarget>, example: &str) -> Result<()> {
    page.heading(Heading::plain(2, "Example"))?;
    page.push_str("```crn\n");
    page.push_str(example);
    if !example.ends_with('\n') {
        page.push_str("\n");
    }
    page.push_str("```\n\n");
    Ok(())
}

fn markdown_enum_heading_texts(
    enums: &BTreeMap<MarkdownEnumKey, MarkdownEnum>,
) -> Vec<(MarkdownEnumKey, String)> {
    let candidates: Vec<_> = enums
        .iter()
        .map(|(key, enum_doc)| {
            let base = format!("{} ({})", key.heading_path, enum_doc.info.type_name);
            let identity = match &key.source {
                MarkdownEnumSource::Smithy { target, type_name } => {
                    format!("{target} {type_name}")
                }
                MarkdownEnumSource::KnownOverride(field) => format!("known override {field}"),
                MarkdownEnumSource::AttributeType(identity) => identity.clone(),
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
    union_heading_paths: &HashSet<HeadingPath>,
) -> Vec<(MarkdownStructKey, String)> {
    let mut used_slugs = HashSet::new();
    let mut headings: Vec<_> = structs
        .iter()
        .filter(|(_, doc)| doc.union_members.is_none())
        .map(|(key, _)| key)
        .map(|key| {
            let path = key.heading_path.to_string();
            let base = if union_heading_paths.contains(&key.heading_path.visible()) {
                format!("{path} ({})", markdown_struct_type_name(key))
            } else {
                path
            };
            let target = match &key.source {
                MarkdownStructSource::AttributeType(name) => format!("AttributeType:{name}"),
                MarkdownStructSource::Smithy(target) => target.clone(),
                MarkdownStructSource::AttributeUnion { .. } => {
                    unreachable!("unions have separate headings")
                }
            };
            let source = format!("{} {target}", key.namespace);
            let heading = claim_unique_heading(base, &source, &mut used_slugs);
            (key.clone(), heading)
        })
        .collect();
    sort_markdown_headings(&mut headings);
    headings
}

fn markdown_struct_type_name(key: &MarkdownStructKey) -> &str {
    match &key.source {
        MarkdownStructSource::AttributeType(name) => name,
        MarkdownStructSource::Smithy(target) => SmithyModel::shape_name(target),
        MarkdownStructSource::AttributeUnion { .. } => {
            unreachable!("anonymous unions have separate headings")
        }
    }
}

/// Member indices distinguish declarations, never their visible headings.
/// Named members sharing a visible path must be distinguished by type name.
fn markdown_union_heading_paths(
    enums: &BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    structs: &BTreeMap<MarkdownStructKey, MarkdownStruct>,
) -> Result<HashSet<HeadingPath>> {
    let declarations = enums
        .iter()
        .map(|(key, doc)| (&key.heading_path, doc.info.type_name.as_str()))
        .chain(
            structs
                .iter()
                .filter(|(_, doc)| doc.union_members.is_none())
                .map(|(key, _)| (&key.heading_path, markdown_struct_type_name(key))),
        );
    let mut by_path: HashMap<HeadingPath, Vec<(&HeadingPath, &str)>> = HashMap::new();
    for (path, type_name) in declarations {
        by_path
            .entry(path.visible())
            .or_default()
            .push((path, type_name));
    }
    let mut shared_paths = HashSet::new();
    for (visible_path, declarations) in by_path {
        if declarations.len() < 2 || !declarations.iter().any(|(path, _)| path.has_union_member()) {
            continue;
        }
        let mut names = HashSet::new();
        for (path, type_name) in declarations {
            if !names.insert(type_name) {
                return Err(DocgenError::AmbiguousUnionHeading {
                    path: path.clone(),
                    type_name: type_name.into(),
                }
                .into());
            }
        }
        shared_paths.insert(visible_path);
    }
    Ok(shared_paths)
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
        description: Description,
        required: bool,
        read_only: bool,
    }
    carina_codegen_aws::kms_references::validate_data_source(ds)?;
    let mut ds_attrs: Vec<DsAttr> = Vec::new();
    for input in &ds.inputs {
        let type_str = if let Some(override_type) = resource_value_override(ds.name, input.name) {
            override_type.type_code
        } else if let Some(t) = input.type_override {
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
            description: Description::authored(input.description),
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
                    type_str: resource_value_override(ds.name, output.name)
                        .map(|value| value.type_code)
                        .unwrap_or_else(|| output.type_code.to_string()),
                    description: Description::authored(output.description),
                    required: false,
                    read_only: true,
                });
            }
        }
        resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => {
            for output in aggregated_outputs {
                let item_type = resource_value_override(ds.name, &format!("{}[]", output.name))
                    .map(|override_type| override_type.type_code)
                    .unwrap_or_else(|| output.item_type.to_string());
                ds_attrs.push(DsAttr {
                    name: output.name.to_string(),
                    provider_name: String::new(),
                    type_str: format!("AttributeType::unordered_list({item_type})"),
                    description: Description::authored(output.description),
                    required: false,
                    read_only: true,
                });
            }
        }
    }

    // Determine needed imports based on actual type strings used.
    // Match managed schemas: fully qualified shared helpers need no `types` import.
    let needs_types = ds_attrs.iter().any(|a| {
        a.type_str
            .replace("carina_aws_types::", "")
            .contains("types::")
    });
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
         \x20       await_on_read_back: vec![],\n\
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
            constructor,
            attr.name,
            type_str,
            attr.description.rust_string_literal(None),
            attr.provider_name,
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
    example: &DocExample,
) -> Result<String> {
    carina_codegen_aws::kms_references::validate_data_source(ds)?;
    let example = example.text_for("aws", ds.name, ExampleKind::DataSource)?;
    struct MarkdownDataSourceOutput<'a> {
        name: &'a str,
        attr_type: MarkdownType,
        description: Description,
    }

    let mut all_enums = BTreeMap::new();
    let mut struct_defs = MarkdownTypes::default();
    let namespace = format!("aws.{}", ds.name);
    let input_names: HashSet<&str> = ds.inputs.iter().map(|input| input.name).collect();
    let outputs: Vec<MarkdownDataSourceOutput<'_>> = match &ds.shape {
        resource_defs::DataSourceShape::Single { output_attributes } => output_attributes
            .iter()
            .filter(|output| !input_names.contains(output.name))
            .map(|output| {
                Ok(MarkdownDataSourceOutput {
                    name: output.name,
                    attr_type: if let Some(value) = resource_value_override(ds.name, output.name) {
                        value.markdown(
                            &HeadingPath::root(output.name),
                            &namespace,
                            &mut all_enums,
                            &mut struct_defs,
                        )?
                    } else {
                        discover_markdown_override(
                            output.type_code,
                            &HeadingPath::root(output.name),
                            &namespace,
                            &mut all_enums,
                            &mut struct_defs,
                        )?
                    },
                    description: Description::authored(output.description),
                })
            })
            .collect::<Result<Vec<_>>>()?,
        resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => aggregated_outputs
            .iter()
            .map(|output| {
                Ok(MarkdownDataSourceOutput {
                    name: output.name,
                    attr_type: {
                        let (item, note) = if let Some(override_type) =
                            resource_value_override(ds.name, &format!("{}[]", output.name))
                        {
                            (
                                discover_markdown_attribute_type(
                                    &override_type.attribute_type,
                                    &HeadingPath::root(output.name).child(TypeEdge::Item),
                                    &namespace,
                                    &mut all_enums,
                                    &mut struct_defs,
                                    None,
                                )?,
                                override_type.documentation,
                            )
                        } else {
                            (
                                discover_markdown_override(
                                    output.item_type,
                                    &HeadingPath::root(output.name).child(TypeEdge::Item),
                                    &namespace,
                                    &mut all_enums,
                                    &mut struct_defs,
                                )?,
                                None,
                            )
                        };
                        list_display(
                            item,
                            false,
                            ListConstraints {
                                length: None,
                                elements: ListElements::AllowDuplicates,
                            },
                            note.as_deref().map(ValidationNote::new),
                        )
                    },
                    description: Description::authored(output.description),
                })
            })
            .collect::<Result<Vec<_>>>()?,
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
            if let Some(override_type) = resource_value_override(ds.name, input.name) {
                page.push_str("- **Type:** ");
                override_type
                    .markdown(
                        &HeadingPath::root(input.name),
                        &namespace,
                        &mut all_enums,
                        &mut struct_defs,
                    )?
                    .write_to(&mut page);
                page.push_str("\n\n");
            } else if let Some(code) = input.type_override {
                let ty = discover_markdown_override(
                    code,
                    &HeadingPath::root(input.name),
                    &namespace,
                    &mut all_enums,
                    &mut struct_defs,
                )?;
                page.push_str("- **Type:** ");
                ty.write_to(&mut page);
                page.push_str("\n\n");
            }

            let description = Description::authored(input.description);
            let markdown = description.markdown();
            if !markdown.is_empty() {
                page.push_str(&format!("{}\n\n", markdown));
            }
        }
    }

    // Output attributes section
    page.heading(Heading::plain(2, "Attributes"))?;
    for output in outputs {
        let description = output.description.markdown();
        page.attribute_reference_entry(
            output.name,
            output.attr_type.table_cell(),
            Some(&description),
        )?;
    }

    write_markdown_type_definitions(&mut page, &all_enums, &struct_defs, &HashMap::new())?;
    page.finish()
        .with_context(|| format!("generated invalid Markdown for aws.{}", ds.name))
}

fn validate_markdown_heading_paths<'a>(
    model: &SmithyModel,
    root_targets: &HashMap<String, String>,
    heading_paths: impl IntoIterator<Item = &'a HeadingPath>,
) -> Result<()> {
    for path in heading_paths {
        let Some((MarkdownTypePosition::Field(root), nested)) = path.segments().split_first()
        else {
            unreachable!("HeadingPath starts at a schema field");
        };
        let mut target = root_targets
            .get(root.as_str())
            .with_context(|| format!("Markdown heading path `{path}` has no root field"))?
            .clone();
        for segment in nested {
            target = match segment {
                MarkdownTypePosition::Field(field) => {
                    markdown_nested_field_target(model, &target, field.as_str())
                }
                MarkdownTypePosition::Item => match model.get_shape(&target) {
                    Some(Shape::List(list)) => Ok(list.member.target.clone()),
                    _ => anyhow::bail!(
                        "Markdown heading path `{path}` has a list hop at non-list `{target}`"
                    ),
                },
                MarkdownTypePosition::Value => match model.get_shape(&target) {
                    Some(Shape::Map(map)) => Ok(map.value.target.clone()),
                    _ => anyhow::bail!(
                        "Markdown heading path `{path}` has a map-value hop at non-map `{target}`"
                    ),
                },
                MarkdownTypePosition::Key => match model.get_shape(&target) {
                    Some(Shape::Map(map)) => Ok(map.key.target.clone()),
                    _ => anyhow::bail!(
                        "Markdown heading path `{path}` has a map-key hop at non-map `{target}`"
                    ),
                },
                MarkdownTypePosition::Member(index) => anyhow::bail!(
                    "Markdown heading path `{path}` has an unexpected union-member hop ({index})"
                ),
            }
            .with_context(|| {
                format!("Markdown heading path `{path}` has no {segment:?} below `{target}`")
            })?;
        }
    }
    Ok(())
}

fn markdown_nested_field_target(
    model: &SmithyModel,
    target: &str,
    field_name: &str,
) -> Result<String> {
    let members = match model.get_shape(target) {
        Some(Shape::Structure(structure)) => &structure.members,
        Some(Shape::Union(union)) => &union.members,
        Some(
            Shape::Service(..)
            | Shape::Operation(..)
            | Shape::List(..)
            | Shape::Map(..)
            | Shape::Enum(..)
            | Shape::IntEnum(..)
            | Shape::String(..)
            | Shape::Boolean(..)
            | Shape::Integer(..)
            | Shape::Long(..)
            | Shape::Float(..)
            | Shape::Double(..)
            | Shape::Blob(..)
            | Shape::Timestamp(..)
            | Shape::Document(..)
            | Shape::Resource(..),
        ) => anyhow::bail!("Smithy shape `{target}` is not a structure or union"),
        None => anyhow::bail!("Smithy shape `{target}` does not exist"),
    };
    members
        .iter()
        .find_map(|(name, member)| {
            (name.to_snake_case() == field_name).then(|| member.target.clone())
        })
        .with_context(|| format!("Smithy struct or union `{target}` has no field `{field_name}`"))
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
    resource_name: &str,
    target: &str,
    field_name: &str,
    heading_path: &HeadingPath,
    property_path: &str,
    enum_namespace: &str,
    type_overrides: &HashMap<&str, &str>,
    struct_field_type_overrides: &HashMap<(&str, &str), &str>,
    parent_struct: Option<&str>,
    all_enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
    struct_defs: &mut MarkdownTypes,
    discovering_structs: &mut HashMap<String, MarkdownStructKey>,
) -> Result<MarkdownType> {
    if let Some(code) = reference_type_code(resource_name, property_path) {
        return discover_markdown_override(
            code,
            heading_path,
            enum_namespace,
            all_enums,
            struct_defs,
        );
    }
    if let Some(override_type) = resource_value_override(resource_name, property_path) {
        return bound_int_override(model, target, field_name, override_type).markdown(
            heading_path,
            enum_namespace,
            all_enums,
            struct_defs,
        );
    }
    // Per-(parent_struct, member) override wins over per-member and Smithy
    // default. Distinguishes a member name shared across structs (aws#302).
    if let Some(parent) = parent_struct
        && let Some(&override_type) = struct_field_type_overrides.get(&(parent, field_name))
    {
        let value = bound_int_override(
            model,
            target,
            field_name,
            ValueTypeOverride {
                type_code: override_type.to_string(),
                documentation: None,
                attribute_type: attribute_type_from_code(override_type)?,
            },
        );
        return discover_markdown_attribute_type(
            &value.attribute_type,
            heading_path,
            enum_namespace,
            all_enums,
            struct_defs,
            None,
        );
    }
    // Check type overrides
    if let Some(&override_type) = type_overrides.get(field_name) {
        if let Some(list) =
            string_list_value_override(model, target, resource_name, property_path, override_type)
        {
            return list.markdown(heading_path, enum_namespace, all_enums, struct_defs);
        }
        let value = bound_int_override(
            model,
            target,
            field_name,
            ValueTypeOverride {
                type_code: override_type.to_string(),
                documentation: None,
                attribute_type: attribute_type_from_code(override_type)?,
            },
        );
        return discover_markdown_attribute_type(
            &value.attribute_type,
            heading_path,
            enum_namespace,
            all_enums,
            struct_defs,
            None,
        );
    }

    // Check known enum overrides
    if let Some(values) = known_enum_overrides().get(field_name) {
        let identity_override = enum_identity_override(resource_name, parent_struct, field_name);
        let (enum_namespace, type_name) =
            effective_enum_identity(identity_override, enum_namespace, field_name);
        let key = register_markdown_enum(
            MarkdownEnumSource::KnownOverride(field_name.to_string()),
            field_name,
            &type_name,
            heading_path,
            &enum_namespace,
            values.iter().map(|value| value.to_string()).collect(),
            all_enums,
        );
        return render_markdown_declaration(
            &AttributeType::enum_(
                TypeIdentity::bare(&type_name),
                Some(all_enums[&key].info.values.clone()),
                Vec::new(),
                None,
                None,
            ),
            DocLink {
                target: MarkdownTarget::Enum(key),
                label: type_name,
            },
            struct_defs,
        );
    }

    let kind = model.shape_kind(target);

    Ok(match kind {
        Some(ShapeKind::String) => {
            if let Some(inferred) = infer_string_type(field_name) {
                return discover_markdown_override(
                    &inferred,
                    heading_path,
                    enum_namespace,
                    all_enums,
                    struct_defs,
                );
            }
            scalar_display(
                ScalarDoc::String {
                    pattern: None,
                    length: None,
                    validate: false,
                    to_dsl: None,
                },
                None,
            )
        }
        Some(ShapeKind::Boolean) => scalar_display(ScalarDoc::Bool, None),
        Some(ShapeKind::Integer) | Some(ShapeKind::Long) => {
            let range = get_int_range(model, target, field_name);
            if let Some(r) = range {
                scalar_display(
                    ScalarDoc::Int {
                        range: Some((r.min, r.max)),
                        validate: false,
                    },
                    None,
                )
            } else {
                scalar_display(
                    ScalarDoc::Int {
                        range: None,
                        validate: false,
                    },
                    None,
                )
            }
        }
        Some(ShapeKind::Float) | Some(ShapeKind::Double) => scalar_display(
            ScalarDoc::Float {
                range: None,
                validate: false,
            },
            None,
        ),
        Some(ShapeKind::Enum) => {
            if let Some(values) = model.enum_values(target) {
                // Prefer the Smithy shape name (PascalCase, e.g. "LogGroupClass")
                // over the field name (often camelCase, e.g. "logGroupClass") so
                // the rendered enum heading reads naturally.
                let type_name =
                    pascalize_enum_type_name(SmithyModel::shape_name(target), field_name);
                let identity_override =
                    enum_identity_override(resource_name, parent_struct, field_name);
                let (enum_namespace, type_name) =
                    effective_enum_identity(identity_override, enum_namespace, &type_name);
                let key = register_markdown_enum(
                    MarkdownEnumSource::Smithy {
                        target: target.to_string(),
                        type_name: type_name.clone(),
                    },
                    field_name,
                    &type_name,
                    heading_path,
                    &enum_namespace,
                    values.into_iter().map(|(_, value)| value).collect(),
                    all_enums,
                );
                render_markdown_declaration(
                    &AttributeType::enum_(
                        TypeIdentity::bare(&type_name),
                        Some(all_enums[&key].info.values.clone()),
                        Vec::new(),
                        None,
                        None,
                    ),
                    DocLink {
                        target: MarkdownTarget::Enum(key),
                        label: type_name,
                    },
                    struct_defs,
                )?
            } else {
                scalar_display(
                    ScalarDoc::String {
                        pattern: None,
                        length: None,
                        validate: false,
                        to_dsl: None,
                    },
                    None,
                )
            }
        }
        Some(ShapeKind::IntEnum) => scalar_display(
            ScalarDoc::Int {
                range: None,
                validate: false,
            },
            None,
        ),
        Some(ShapeKind::List) => {
            if let Some(carina_smithy::Shape::List(list_shape)) = model.get_shape(target) {
                let item = discover_markdown_type(
                    model,
                    resource_name,
                    &list_shape.member.target,
                    field_name,
                    &heading_path.child(TypeEdge::Item),
                    &kms_references::property_path(property_path, PathSegment::ListElement),
                    enum_namespace,
                    type_overrides,
                    struct_field_type_overrides,
                    parent_struct,
                    all_enums,
                    struct_defs,
                    discovering_structs,
                )?;
                let length = match resource_type_overrides().get(&(resource_name, property_path)) {
                    Some(TypeOverride::ListLength { min, max }) => Some((Some(*min), Some(*max))),
                    _ => None,
                };
                list_display(
                    item,
                    true,
                    ListConstraints {
                        length,
                        elements: ListElements::AllowDuplicates,
                    },
                    None,
                )
            } else {
                list_display(
                    scalar_display(
                        ScalarDoc::String {
                            pattern: None,
                            length: None,
                            validate: false,
                            to_dsl: None,
                        },
                        None,
                    ),
                    true,
                    ListConstraints {
                        length: None,
                        elements: ListElements::AllowDuplicates,
                    },
                    None,
                )
            }
        }
        Some(ShapeKind::Map) => {
            let value = if let Some(Shape::Map(map_shape)) = model.get_shape(target) {
                discover_markdown_type(
                    model,
                    resource_name,
                    &map_shape.value.target,
                    field_name,
                    &heading_path.child(TypeEdge::Value),
                    &kms_references::property_path(property_path, PathSegment::MapValue),
                    enum_namespace,
                    type_overrides,
                    struct_field_type_overrides,
                    parent_struct,
                    all_enums,
                    struct_defs,
                    discovering_structs,
                )?
            } else {
                scalar_display(
                    ScalarDoc::String {
                        pattern: None,
                        length: None,
                        validate: false,
                        to_dsl: None,
                    },
                    None,
                )
            };
            let length = if let Some(Shape::Map(shape)) = model.get_shape(target) {
                length_from_traits(&shape.traits)
            } else {
                None
            };
            map_display(
                scalar_display(
                    ScalarDoc::String {
                        pattern: None,
                        length: None,
                        validate: false,
                        to_dsl: None,
                    },
                    None,
                ),
                value,
                MapConstraints { length },
                None,
            )
        }
        Some(ShapeKind::Structure | ShapeKind::Union) => {
            let shape_name = SmithyModel::shape_name(target).to_string();
            if kind == Some(ShapeKind::Structure)
                && (shape_name == "TagList" || shape_name == "Tag")
                && let Some(tags) = TagMapType::from_shape(model, target)?
            {
                return discover_markdown_attribute_type(
                    &tags.attribute_type(),
                    heading_path,
                    enum_namespace,
                    all_enums,
                    struct_defs,
                    None,
                );
            }
            if kind == Some(ShapeKind::Structure) && shape_name == "AttributeBooleanValue" {
                return Ok(scalar_display(ScalarDoc::Bool, None));
            }
            let structure = if let Some(structure) = model.get_structure(target) {
                StructRepresentation::from_structure(structure)
            } else if let Some(Shape::Union(union)) = model.get_shape(target) {
                union_struct_representation(target, &union.members)?
            } else {
                anyhow::bail!("Cannot find Smithy struct or union {target}");
            };
            if let Some(active_key) = discovering_structs.get(target) {
                return render_markdown_declaration(
                    &AttributeType::struct_(&shape_name, Vec::new()),
                    DocLink {
                        target: MarkdownTarget::Struct(active_key.clone()),
                        label: shape_name,
                    },
                    struct_defs,
                );
            }

            let child_enum_namespace = format!("{enum_namespace}.{shape_name}");
            let key = MarkdownStructKey {
                namespace: child_enum_namespace.clone(),
                heading_path: heading_path.clone(),
                source: MarkdownStructSource::Smithy(target.to_string()),
            };
            if !struct_defs.contains_key(&key) {
                discovering_structs.insert(target.to_string(), key.clone());
                let fields = structure
                    .members
                    .iter()
                    .map(|(name, member_ref)| {
                        let child_heading_path =
                            heading_path.child(TypeEdge::Field(&name.to_snake_case()));
                        let field_type = discover_markdown_type(
                            model,
                            resource_name,
                            &member_ref.target,
                            name,
                            &child_heading_path,
                            &kms_references::property_path(
                                property_path,
                                PathSegment::Field(&name.to_snake_case()),
                            ),
                            &child_enum_namespace,
                            type_overrides,
                            struct_field_type_overrides,
                            Some(&shape_name),
                            all_enums,
                            struct_defs,
                            discovering_structs,
                        )?;
                        let description = SmithyModel::documentation(&member_ref.traits)
                            .map(Description::from_smithy_html);
                        Ok(MarkdownStructField {
                            snake_name: name.to_snake_case(),
                            field_type,
                            is_required: structure.required_fields.contains(name.as_str()),
                            description,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                discovering_structs.remove(target);
                struct_defs.insert(
                    key.clone(),
                    MarkdownStruct {
                        union_members: None,
                        fields,
                        field_groups: structure.field_groups,
                    },
                );
            }
            render_markdown_declaration(
                &AttributeType::struct_(&shape_name, Vec::new()),
                DocLink {
                    target: MarkdownTarget::Struct(key),
                    label: shape_name,
                },
                struct_defs,
            )?
        }
        Some(
            ShapeKind::Blob
            | ShapeKind::Timestamp
            | ShapeKind::Service
            | ShapeKind::Operation
            | ShapeKind::Resource
            | ShapeKind::Unit,
        )
        | None => {
            if let Some(inferred) = infer_string_type(field_name) {
                discover_markdown_override(
                    &inferred,
                    heading_path,
                    enum_namespace,
                    all_enums,
                    struct_defs,
                )?
            } else {
                scalar_display(
                    ScalarDoc::String {
                        pattern: None,
                        length: None,
                        validate: false,
                        to_dsl: None,
                    },
                    None,
                )
            }
        }
    })
}

struct MarkdownLinkHooks {
    link: DocLink<MarkdownTarget>,
}

impl<'a> TypeDocHooks<'a, Draft> for MarkdownLinkHooks {
    type NodeId = MarkdownNodeId;
    type Target = MarkdownTarget;
    type Scope = ();

    fn resolve(&self, ty: &'a AttributeType) -> Result<Resolved<'a, Draft, MarkdownNodeId>> {
        Ok(Resolved {
            id: MarkdownNodeId::Smithy(self.link.target.clone()),
            ty,
        })
    }

    fn scalar_name(&self, identity: &TypeIdentity) -> String {
        declared_scalar_display_name(identity)
    }
    fn child_scope(&self, _parent: &(), _edge: TypeEdge<'_>) {}

    fn register(
        &mut self,
        _scope: &(),
        _declaration: Declaration<'a, Draft, MarkdownNodeId>,
    ) -> Result<DocLink<MarkdownTarget>> {
        Ok(self.link.clone())
    }
}

fn render_markdown_declaration(
    ty: &AttributeType,
    link: DocLink<MarkdownTarget>,
    types: &mut MarkdownTypes,
) -> Result<MarkdownType> {
    render_attribute_type(
        ty,
        &(),
        &mut MarkdownLinkHooks { link },
        &mut types.registry,
        None,
    )
}

fn register_markdown_enum(
    source: MarkdownEnumSource,
    prop_name: &str,
    type_name: &str,
    heading_path: &HeadingPath,
    namespace: &str,
    values: Vec<String>,
    all_enums: &mut BTreeMap<MarkdownEnumKey, MarkdownEnum>,
) -> MarkdownEnumKey {
    let key = MarkdownEnumKey {
        namespace: namespace.to_string(),
        heading_path: heading_path.clone(),
        source,
    };
    all_enums
        .entry(key.clone())
        .or_insert_with(|| MarkdownEnum {
            prop_name: prop_name.to_string(),
            dsl_aliases: vec![],
            info: EnumInfo {
                type_name: type_name.to_string(),
                values,
                identity_override: None,
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

/// Convert a scalar Rust type code string to a literal display name.
///
/// Display names use PascalCase consistently (e.g. `IamRoleArn`, not
/// `iam_role_arn`). Collection and named-type wording belongs to carina-docgen.
fn type_code_to_display(type_code: &str) -> String {
    match type_code {
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
        "carina_aws_types::sqs_queue_arn()" => "SqsQueueArn".to_string(),
        s if s.contains("route53_hosted_zone_id") => "Route53HostedZoneId".to_string(),
        "super::kms_key_arn()" | "super::super::kms::key::arn()" => "KmsKeyArn".to_string(),
        "carina_aws_types::kms_key_reference()" | "carina_aws_types::kms_key_arn_reference()" => {
            "KmsKeyReference".to_string()
        }
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
        "carina_aws_types::organizations_account_arn()" => "OrganizationsAccountArn".to_string(),
        "carina_aws_types::organizations_organization_arn()" => {
            "OrganizationsOrganizationArn".to_string()
        }
        s if s.contains("arn()") => "Arn".to_string(),
        s if s.contains("aws_account_id") => "AwsAccountId".to_string(),
        s if s.contains("aws_resource_id") => "AwsResourceId".to_string(),
        s if s.contains("availability_zone_id") => "AvailabilityZoneId".to_string(),
        s if s.contains("availability_zone") => "ZoneName".to_string(),
        "carina_aws_types::organizations_account_email()" => {
            "OrganizationsAccountEmail".to_string()
        }
        s if s.contains("email") => "Email".to_string(),
        _ => snake_to_pascal_type(
            type_code
                .trim_start_matches("carina_aws_types::")
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
/// Value overrides use (resource name, full DSL property path), including []/{}
/// markers. EnumIdentity entries use their existing Smithy shape/member keys.
/// Use this when a property needs resource-specific type treatment that differs
/// from global overrides or pattern-based inference.
// acm.json DomainNameString: remove (?!-) by requiring each label to start
// with an alphanumeric. Interior labels are 1..63, the final label is 2..63.
// Unlike core's wildcard_domain_name(), ACM's pattern excludes a terminal dot.
const ACM_DOMAIN_NAME_PATTERN: &str = carina_aws_types::ACM_CERTIFICATE_DOMAIN_NAME_PATTERN;

fn resource_type_overrides() -> &'static HashMap<(&'static str, &'static str), TypeOverride> {
    static OVERRIDES: LazyLock<HashMap<(&'static str, &'static str), TypeOverride>> =
        LazyLock::new(|| {
            HashMap::from([
                // Share Route 53's anonymous DNS carrier, as ACM validation
                // records do, so CloudFront/Route 53/module references retain
                // compatible grammar and bounds in both managed and read views.
                // https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html
                // Keep region as String in both Bucket views: aws.Region
                // canonicalizes read values and changes their recorded state.
                (
                    ("s3.Bucket", "bucket_domain_name"),
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    ("s3.Bucket", "bucket_regional_domain_name"),
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    ("route53.RecordSet", "name"),
                    // ResourceRecordSet$Name -> DNSName (0..1024); the guide
                    // requires nonempty labels, decoded byte limits, and the
                    // record-specific wildcard rules. Share ACM's DNS carrier.
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    ("route53.RecordSet", "alias_target.dns_name"),
                    // AliasTarget$DNSName -> DNSName. CloudFront, ALB (also
                    // dualstack), S3 websites and another record's DNS name:
                    // https://docs.aws.amazon.com/Route53/latest/APIReference/API_AliasTarget.html
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    ("route53.RecordSet", "resource_records[]"),
                    // ResourceRecord$Value -> RData (0..4000 characters).
                    // Format is conditional on Type, validated provider-locally.
                    // https://docs.aws.amazon.com/Route53/latest/APIReference/API_ResourceRecord.html
                    TypeOverride::StringBounds { min: 0, max: 4000 },
                ),
                (
                    ("route53.RecordSet", "hosted_zone_id"),
                    // ChangeResourceRecordSetsRequest$HostedZoneId ->
                    // ResourceId (max 32), same as AliasTarget$HostedZoneId.
                    // Reuse the existing AWS-only zone identity.
                    // https://docs.aws.amazon.com/Route53/latest/APIReference/API_ChangeResourceRecordSets.html
                    TypeOverride::SharedType(AwsType::Route53HostedZoneId),
                ),
                (
                    ("ec2.Eip", "address"),
                    // ec2.json AllocateAddressRequest$Address -> PublicIpAddress.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_AllocateAddress.html
                    TypeOverride::CoreType("types::ipv4_address()"),
                ),
                (
                    ("ec2.FlowLog", "log_format"),
                    // CreateFlowLogsRequest$LogFormat: at least one ${field-id},
                    // separated by spaces. Keep the catalog open to new fields
                    // and preserve the exact order and string sent to AWS.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateFlowLogs.html
                    // https://docs.aws.amazon.com/vpc/latest/userguide/flow-log-records.html
                    // https://docs.aws.amazon.com/vpc/latest/tgw/tgw-flow-logs.html
                    TypeOverride::StringPattern(PatternSpec::new(
                        r"^\$\{[a-z0-9]+(?:-[a-z0-9]+)*\}(?: +\$\{[a-z0-9]+(?:-[a-z0-9]+)*\})*\z",
                        PatternMatch::Full,
                    )),
                ),
                (
                    ("ec2.FlowLog", "max_aggregation_interval"),
                    // CreateFlowLogsRequest$MaxAggregationInterval: {60,600}.
                    // The transit-only 60 rule lives in normalizer.rs.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateFlowLogs.html
                    TypeOverride::SharedType(AwsType::Ec2FlowLogMaxAggregationInterval),
                ),
                (
                    ("ec2.FlowLog", "log_group_name"),
                    // CreateFlowLogsRequest$LogGroupName can select an existing
                    // group. Apply cloudwatchlogs.json LogGroupName's grammar,
                    // without imposing CreateLogGroup's create-only aws/ rule.
                    // https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_CreateLogGroup.html
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(
                            r"^[.\-_/#A-Za-z0-9]{1,512}\z",
                            PatternMatch::Full,
                        ),
                        min_length: 1,
                        max_length: 512,
                    },
                ),
                (
                    ("ec2.FlowLog", "tag_field_specifications"),
                    // ec2.json TagFieldSpecificationListRequest, length 1..3.
                    TypeOverride::ListLength { min: 1, max: 3 },
                ),
                (
                    ("ec2.FlowLog", "tag_field_specifications[].tag_keys"),
                    // TagFieldSpecificationRequest$TagKeys -> TagKeyList.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TagFieldSpecificationRequest.html
                    TypeOverride::ListLength { min: 1, max: 2 },
                ),
                (
                    ("ec2.FlowLog", "tag_field_specifications[].tag_keys[]"),
                    // TagKeyList$member -> TagKey: length 1..128, no pattern.
                    // (?s) preserves the entire Smithy string alphabet.
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(r"(?s)^.{1,128}\z", PatternMatch::Full),
                        min_length: 1,
                        max_length: 128,
                    },
                ),
                (
                    ("ec2.SecurityGroup", "group_name"),
                    // CreateSecurityGroupRequest$GroupName (no Smithy traits).
                    // The callback enforces the reserved sg- prefix as well as
                    // the documented alphabet and length through the SDK seam.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateSecurityGroup.html
                    TypeOverride::SharedType(AwsType::Ec2SecurityGroupName),
                ),
                (
                    ("ec2.Subnet", "cidr_block"),
                    // CreateSubnetRequest$CidrBlock. Keep the CIDR parser and
                    // host-bit acceptance; only this resource gets /16..=/28.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
                    TypeOverride::SharedType(AwsType::Ec2SubnetIpv4CidrBlock),
                ),
                (
                    ("ec2.Subnet", "ipv6_cidr_block"),
                    // CreateSubnetRequest$Ipv6CidrBlock: /44..=/64, step four.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
                    TypeOverride::SharedType(AwsType::Ec2SubnetIpv6CidrBlock),
                ),
                (
                    ("ec2.Subnet", "ipv6_netmask_length"),
                    // CreateSubnetRequest$Ipv6NetmaskLength uses the same set.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
                    TypeOverride::SharedType(AwsType::Ec2SubnetIpv6NetmaskLength),
                ),
                (
                    ("ec2.Subnet", "ipv4_netmask_length"),
                    // CreateSubnetRequest$Ipv4NetmaskLength -> NetmaskLength.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/subnet-sizing.html
                    TypeOverride::IntRange { min: 16, max: 28 },
                ),
                (
                    ("ec2.Subnet", "enable_lni_at_device_index"),
                    // ModifySubnetAttributeRequest$EnableLniAtDeviceIndex:
                    // eth0 cannot be local; the Smithy Integer carrier is i32.
                    // No instance-dependent maximum is inferred.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_ModifySubnetAttribute.html
                    TypeOverride::IntRange {
                        min: 1,
                        max: i32::MAX as i64,
                    },
                ),
                (
                    ("ec2.TransitGateway", "options.amazon_side_asn"),
                    // TransitGatewayRequestOptions$AmazonSideAsn: two private
                    // ASN ranges, not their bounding interval.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TransitGatewayRequestOptions.html
                    // https://docs.aws.amazon.com/vpc/latest/tgw/create-tgw.html
                    TypeOverride::SharedType(AwsType::Ec2AmazonSideAsn),
                ),
                (
                    (
                        "ec2.TransitGateway",
                        "options.transit_gateway_cidr_blocks[]",
                    ),
                    // TransitGatewayRequestOptions$TransitGatewayCidrBlocks:
                    // fix the existing IPv4-only inference while bounding both
                    // prefixes. /23 is larger than /24, as the guide explains.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TransitGatewayRequestOptions.html
                    // https://docs.aws.amazon.com/vpc/latest/tgw/create-tgw.html
                    TypeOverride::SharedType(AwsType::Ec2TransitGatewayCidrBlock),
                ),
                (
                    ("ec2.Vpc", "cidr_block"),
                    // CreateVpcRequest$CidrBlock, with the documented excluded
                    // ranges. Public IPv4 CIDRs and noncanonical host bits work.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/vpc-cidr-blocks.html
                    TypeOverride::SharedType(AwsType::Ec2VpcCidrBlock),
                ),
                (
                    ("ec2.Vpc", "ipv4_netmask_length"),
                    // CreateVpcRequest$Ipv4NetmaskLength -> NetmaskLength.
                    // https://docs.aws.amazon.com/vpc/latest/userguide/vpc-cidr-blocks.html
                    TypeOverride::IntRange { min: 16, max: 28 },
                ),
                (
                    ("ec2.VpnGateway", "amazon_side_asn"),
                    // Smithy/CreateVpnGateway prose describes private ranges,
                    // but the VPN FAQ also documents legacy Amazon ASNs:
                    // 9059 Dublin, 17493 Singapore, 10124 Tokyo, 7224 elsewhere.
                    // Accept the complete set for input/read compatibility;
                    // a scalar cannot decide the region-specific pairing.
                    // https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateVpnGateway.html
                    // https://aws.amazon.com/vpn/faqs/
                    TypeOverride::SharedType(AwsType::Ec2VpnGatewayAmazonSideAsn),
                ),
                (
                    ("acm.Certificate", "domain_name"),
                    // RequestCertificateRequest$DomainName -> DomainNameString.
                    // The member's Common Name prose narrows the shape's 253
                    // limit to 64 octets; the ASCII pattern makes bytes = chars.
                    // https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html
                    // Request-only Punycode: positions 3–4 of a label may be
                    // -- only for xn--; that A-label must pass IDNA validation.
                    // https://docs.aws.amazon.com/acm/latest/userguide/acm-certificate-characteristics.html
                    TypeOverride::SharedType(AwsType::AcmCertificateDomainName),
                ),
                (
                    ("acm.Certificate", "subject_alternative_names[]"),
                    // RequestCertificateRequest$SubjectAlternativeNames ->
                    // DomainList$member -> DomainNameString, 1..253 octets.
                    // The same RequestCertificate docs cap each label at 63.
                    // Apply the same request-only IDNA validator to each SAN:
                    // https://docs.aws.amazon.com/acm/latest/userguide/acm-certificate-characteristics.html
                    TypeOverride::SharedType(AwsType::AcmCertificateSubjectAlternativeName),
                ),
                (
                    ("acm.Certificate", "domain_validation_options[].domain_name"),
                    // DomainValidation$DomainName -> DomainNameString. Read
                    // domains retain the full 253 cap, without the request cap.
                    // https://docs.aws.amazon.com/acm/latest/APIReference/API_DomainValidation.html
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(ACM_DOMAIN_NAME_PATTERN, PatternMatch::Full),
                        min_length: 1,
                        max_length: 253,
                    },
                ),
                (
                    (
                        "acm.Certificate",
                        "domain_validation_options[].validation_domain",
                    ),
                    // DomainValidation$ValidationDomain -> DomainNameString.
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(ACM_DOMAIN_NAME_PATTERN, PatternMatch::Full),
                        min_length: 1,
                        max_length: 253,
                    },
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].domain_name",
                    ),
                    // RenewalSummary reuses the same DomainValidation shape.
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(ACM_DOMAIN_NAME_PATTERN, PatternMatch::Full),
                        min_length: 1,
                        max_length: 253,
                    },
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].validation_domain",
                    ),
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(ACM_DOMAIN_NAME_PATTERN, PatternMatch::Full),
                        min_length: 1,
                        max_length: 253,
                    },
                ),
                (
                    (
                        "acm.Certificate",
                        "domain_validation_options[].resource_record.name",
                    ),
                    // ACM supplies the DNS name and CNAME target to publish:
                    // https://docs.aws.amazon.com/acm/latest/APIReference/API_ResourceRecord.html
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    (
                        "acm.Certificate",
                        "domain_validation_options[].resource_record.value",
                    ),
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].resource_record.name",
                    ),
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].resource_record.value",
                    ),
                    TypeOverride::AnonymousStringConstructor(
                        "carina_aws_types::route53_dns_name()",
                    ),
                ),
                (
                    (
                        "acm.Certificate",
                        "domain_validation_options[].http_redirect.redirect_from",
                    ),
                    TypeOverride::SharedType(AwsType::AcmCertificateValidationUrl),
                ),
                (
                    (
                        "acm.Certificate",
                        "domain_validation_options[].http_redirect.redirect_to",
                    ),
                    TypeOverride::SharedType(AwsType::AcmCertificateValidationUrl),
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].http_redirect.redirect_from",
                    ),
                    TypeOverride::SharedType(AwsType::AcmCertificateValidationUrl),
                ),
                (
                    (
                        "acm.Certificate",
                        "renewal_summary.domain_validation_options[].http_redirect.redirect_to",
                    ),
                    TypeOverride::SharedType(AwsType::AcmCertificateValidationUrl),
                ),
                (
                    ("acm.Certificate", "certificate_arn"),
                    TypeOverride::SharedType(AwsType::AcmCertificateArn),
                ),
                (
                    ("acm.Certificate", "key_algorithm"),
                    // RequestCertificate's KeyAlgorithm prose excludes the
                    // other four Smithy enum values: imported certificates
                    // only. resource_defs::acm_resources requests certificates;
                    // ACM reads canonicalize known hyphenated spellings back
                    // to these underscore wire values, preserving future ones.
                    // https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html
                    TypeOverride::SharedType(AwsType::AcmCertificateKeyAlgorithm),
                ),
                // RenewalSummary$UpdatedAt -> TStamp stays String: its SDK
                // DateTime.to_string() spelling is not S3LifecycleDate's fixed
                // UTC seconds-only grammar. Do not narrow an output by guess.
                (
                    ("iam.Role", "role_name"),
                    // carina-provider-aws/tests/fixtures/smithy/iam.json:
                    // CreateRoleRequest$RoleName -> roleNameType, length 1..64,
                    // ^[\w+=,.@-]+$. All role-name input operations use this shape.
                    // Share the named type with Roles.names[] and AWSCC.
                    // carina-test-004, 2026-11-03 (IAM): CreateRole/GetRole/
                    // ListRoles preserve RoleName; "bad name" and 65 chars fail.
                    TypeOverride::SharedType(AwsType::IamRoleName),
                ),
                (
                    ("iam.Role", "path"),
                    // carina-provider-aws/tests/fixtures/smithy/iam.json:
                    // CreateRoleRequest$Path and Role$Path -> pathType,
                    // length 1..512; ^(\u002F)|(\u002F[\u0021-\u007E]+\u002F)$.
                    // Group the alternatives under whole-string anchors.
                    // carina-test-004, 2026-11-03 (IAM): CreateRole/GetRole/
                    // ListRoles preserve /carina-588/probe/; /no-trailing fails.
                    // '-' succeeds: use the model range, not the error prose.
                    TypeOverride::AnonymousStringConstructor("carina_aws_types::iam_path()"),
                ),
                (
                    ("iam.Roles", "path_prefix"),
                    // carina-provider-aws/tests/fixtures/smithy/iam.json:
                    // ListRolesRequest$PathPrefix -> pathPrefixType,
                    // length 1..512; ^\u002F[\u0021-\u007F]*$.
                    // Unlike pathType, this includes DEL (U+007F) and does not
                    // require a trailing slash. Preserve the exact model range.
                    // carina-test-004, 2026-11-03 (IAM): ListRoles rejects
                    // "carina" because the leading slash is missing.
                    TypeOverride::AnonymousStringConstructor("carina_aws_types::iam_path_prefix()"),
                ),
                (
                    ("iam.Roles", "names[]"),
                    // Smithy ListRolesResponse$Roles -> roleListType ->
                    // Role$RoleName -> roleNameType, in the same iam.json.
                    TypeOverride::SharedType(AwsType::IamRoleName),
                ),
                (
                    ("organizations.Account", "role_name"),
                    // carina-provider-aws/tests/fixtures/smithy/organizations.json:
                    // CreateAccountRequest$RoleName -> RoleName, length 0..64,
                    // ^[\w+=,.@-]{1,64}$. The pattern excludes empty strings,
                    // so this has the same effective 1..64 length and character
                    // set as iam.json's roleNameType. It names the IAM role
                    // created in the new account; reuse that role's identity.
                    // https://docs.aws.amazon.com/organizations/latest/APIReference/API_CreateAccount.html
                    TypeOverride::SharedType(AwsType::IamRoleName),
                ),
                (
                    ("organizations.Account", "account_name"),
                    // In the same organizations.json:
                    // CreateAccountRequest$AccountName -> CreateAccountName,
                    // length 1..50; ^[\u0020-\u007E]+$.
                    // carina-provider-aws/tests/fixtures/smithy/account.json:
                    // PutAccountNameRequest$AccountName -> AccountName,
                    // length 1..50; ^[ -;=?-~]+$. The provider sends the same
                    // desired value to CreateAccount and PutAccountName, so
                    // intersect their constraints: printable ASCII except < >.
                    // https://docs.aws.amazon.com/accounts/latest/APIReference/API_PutAccountName.html
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(r"^[ -;=?-~]+\z", PatternMatch::Full),
                        min_length: 1,
                        max_length: 50,
                    },
                ),
                (
                    ("organizations.Account", "email"),
                    // organizations.json: CreateAccountRequest$Email and
                    // Account$Email -> Email, length 6..64. The shared catalog
                    // type adds these bounds to core's public validate_email;
                    // core's cross-service Email identity stays unchanged.
                    TypeOverride::SharedType(AwsType::OrganizationsAccountEmail),
                ),
                (
                    ("organizations.Organization", "master_account_email"),
                    // Organization$MasterAccountEmail targets the same Email
                    // shape and 6..64 bounds. Use the output member's model,
                    // without CreateAccount's stricter creation-only prose.
                    // https://docs.aws.amazon.com/organizations/latest/APIReference/API_Organization.html#organizations-Type-Organization-MasterAccountEmail
                    TypeOverride::SharedType(AwsType::OrganizationsAccountEmail),
                ),
                (
                    ("organizations.Account", "id"),
                    // organizations.json: Account$Id -> AccountId, length
                    // 0..12 and ^\d{12}$. The pattern forces exactly 12 ASCII
                    // digits (ECMA-262), matching the shared aws.AccountId.
                    TypeOverride::SharedType(AwsType::AccountId),
                ),
                (
                    ("organizations.Organization", "id"),
                    // organizations.json: Organization$Id -> OrganizationId,
                    // ^o-[a-z0-9]{10,32}$. No length trait; the prefix and
                    // quantifier give an effective total length of 12..34.
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(r"^o-[a-z0-9]{10,32}\z", PatternMatch::Full),
                        min_length: 12,
                        max_length: 34,
                    },
                ),
                (
                    ("organizations.Account", "arn"),
                    // Account$Arn -> AccountArn; keep the Smithy resource
                    // grammar and use the shared Partition set (#638).
                    TypeOverride::SharedType(AwsType::OrganizationsAccountArn),
                ),
                (
                    ("organizations.Organization", "arn"),
                    // Organization$Arn -> OrganizationArn, in the same model.
                    TypeOverride::SharedType(AwsType::OrganizationsOrganizationArn),
                ),
                (
                    ("organizations.Organization", "master_account_arn"),
                    // Organization$MasterAccountArn -> AccountArn: exactly
                    // the same target as Account$Arn, not a generic ARN.
                    TypeOverride::SharedType(AwsType::OrganizationsAccountArn),
                ),
                (
                    ("organizations.Organization", "master_account_id"),
                    // Organization$MasterAccountId -> the same AccountId
                    // shape as Account$Id. Preserve its existing shared type
                    // and include the catalog bounds in generated docs.
                    TypeOverride::SharedType(AwsType::AccountId),
                ),
                // Account$JoinedTimestamp -> Timestamp stays String pending
                // a real-AWS read and a decision about SDK DateTime formatting.
                (
                    ("logs.LogGroup", "log_group_name"),
                    // Smithy com.amazonaws.cloudwatchlogs#LogGroupName:
                    // length 1..512 and pattern ^[.\-_/#A-Za-z0-9]+$.
                    // CreateLogGroup also forbids the prefix aws/:
                    // https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_CreateLogGroup.html
                    // Leave that create-only rule to AWS: custom scalar validators
                    // require an identity and are not portable anonymous refinements.
                    // Match AWSCC's exact anonymous pattern and length metadata
                    // so name references are assignable in both directions:
                    // carina-provider-awscc/src/schemas/generated/logs/log_group.rs
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(
                            r"^[.\-_/#A-Za-z0-9]{1,512}\z",
                            PatternMatch::Full,
                        ),
                        min_length: 1,
                        max_length: 512,
                    },
                ),
                (
                    ("logs.LogGroup", "retention_in_days"),
                    // PutRetentionPolicyRequest$retentionInDays -> Days:
                    // https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutRetentionPolicy.html
                    // Share AWSCC's established RetentionInDays identity and
                    // validated Int shape through the common catalog.
                    TypeOverride::SharedType(AwsType::LogsLogGroupRetentionInDays),
                ),
                (
                    ("sqs.Queue", "queue_name"),
                    // Cached sqs.json CreateQueueRequest$QueueName has no
                    // pattern/length traits; its documentation supplies these
                    // limits. The FIFO suffix counts within the total 80:
                    // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
                    // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-fifo-queue-message-identifiers.html
                    // Protocol 23's scoped input rules cannot express cross-field
                    // conditions. Leave the fifo_queue <-> .fifo condition to AWS;
                    // do not add a provider-local runtime validator.
                    TypeOverride::RefinedString {
                        pattern: PatternSpec::new(
                            r"^(?:[A-Za-z0-9_-]{1,80}|[A-Za-z0-9_-]{1,75}\.fifo)\z",
                            PatternMatch::Full,
                        ),
                        min_length: 1,
                        max_length: 80,
                    },
                ),
                // QueueAttributeMap values are wire Strings. The model's
                // CreateQueueRequest$Attributes prose documents these Int
                // ranges; retain the hand-written decimal-string packing.
                // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
                (
                    ("sqs.Queue", "visibility_timeout"),
                    TypeOverride::IntRange { min: 0, max: 43200 },
                ),
                (
                    ("sqs.Queue", "message_retention_period"),
                    TypeOverride::IntRange {
                        min: 60,
                        max: 1209600,
                    },
                ),
                (
                    ("sqs.Queue", "delay_seconds"),
                    TypeOverride::IntRange { min: 0, max: 900 },
                ),
                (
                    ("sqs.Queue", "maximum_message_size"),
                    TypeOverride::IntRange {
                        min: 1024,
                        max: 1048576,
                    },
                ),
                (
                    ("sqs.Queue", "receive_message_wait_time_seconds"),
                    TypeOverride::IntRange { min: 0, max: 20 },
                ),
                (
                    ("sqs.Queue", "kms_data_key_reuse_period_seconds"),
                    TypeOverride::IntRange {
                        min: 60,
                        max: 86400,
                    },
                ),
                // Same model member and API source: closed values, with the
                // catalog owning their identities and API <-> DSL aliases.
                (
                    ("sqs.Queue", "deduplication_scope"),
                    TypeOverride::SharedType(AwsType::SqsQueueDeduplicationScope),
                ),
                (
                    ("sqs.Queue", "fifo_throughput_limit"),
                    TypeOverride::SharedType(AwsType::SqsQueueFifoThroughputLimit),
                ),
                (
                    ("sqs.Queue", "queue_arn"),
                    TypeOverride::SharedType(AwsType::SqsQueueArn),
                ),
                (
                    ("route53.RecordSet", "Type"),
                    TypeOverride::EnumIdentity(AwsType::DnsRecordType),
                ),
                (
                    ("acm.Certificate", "ResourceRecord.Type"),
                    // com.amazonaws.acm#RecordType in acm.json is CNAME-only;
                    // it cannot share Route 53's full DNS enum definition.
                    TypeOverride::EnumIdentity(AwsType::AcmCertificateResourceRecordType),
                ),
            ])
        });
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

#[cfg(test)]
mod tests {

    #[test]
    fn resource_input_rules_are_emitted_generically() {
        use carina_core::schema::{
            InputPath, InputPredicate, InputRule, InputRuleCode, InputRuleScope,
        };

        let model = string_fixture_model(&[("Description", true)], &[("Description", true)]);
        let mut resource = provider_fixture_resource("iam.Role", "", &["Description"], vec![]);
        let baseline = generate_resource(&resource, &model).unwrap();
        assert!(!baseline.contains(".input_rule("));
        resource.input_rules.push(InputRule::value(
            InputRuleScope::EveryWrite,
            InputPath::new("description").unwrap(),
            InputRuleCode::new("test.description.pattern").unwrap(),
            InputPredicate::NotMatches(PatternSpec::new(r"^path\\secret$", PatternMatch::Full)),
        ));
        let generated = generate_resource(&resource, &model).unwrap();
        assert_eq!(generated.matches(".input_rule(").count(), 1, "{generated}");
        for expected in [
            "InputRuleScope::EveryWrite",
            "InputRuleTarget::Value",
            "InputPath::new(\"description\")",
            "InputRuleCode::new(\"test.description.pattern\")",
            "InputPredicate::NotMatches",
            r#"PatternSpec::new("^path\\\\secret$", carina_core::schema::PatternMatch::Full)"#,
        ] {
            assert!(generated.contains(expected), "{expected}: {generated}");
        }
    }

    #[test]
    fn map_length_tag_strings_preserve_literal_backslashes() {
        let model = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "test#Tags":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"test#Value"}},
                "test#Value":{"type":"string","traits":{"smithy.api#pattern":"^path\\\\[a-z]+$","smithy.api#length":{"min":2,"max":40}}}
            }
        }"##).unwrap();
        let tags = TagMapType::from_shape(&model, "test#Tags")
            .unwrap()
            .unwrap();
        let pattern = r"^path\\[a-z]+$";
        assert!(
            tags.code()
                .contains(&format!("Some(carina_core::schema::PatternSpec::new({pattern:?}, carina_core::schema::PatternMatch::Substring {{ reason: carina_core::schema::PatternReason::new(\"Smithy tag patterns retain their authored search semantics.\").expect(\"visible pattern reason\") }}))")),
            "{}",
            tags.code()
        );
        let schema = carina_core::schema::ResourceSchema::new("test.BackslashTags")
            .attribute(
                carina_core::schema::AttributeSchema::optional("tags", tags.attribute_type())
                    .build(),
            )
            .validate()
            .unwrap();
        let value: carina_core::resource::Value =
            carina_core::resource::Value::Concrete(carina_core::resource::ConcreteValue::Map(
                [(
                    "Name".to_string(),
                    carina_core::resource::Value::Concrete(
                        carina_core::resource::ConcreteValue::String(r"path\value".into()),
                    ),
                )]
                .into_iter()
                .collect(),
            ));
        schema
            .effective_attribute("tags")
            .unwrap()
            .validate(&value)
            .expect("a literal backslash must survive the model-to-schema path");
    }

    #[test]
    fn map_length_complete_service_models_require_a_tag_source() {
        let resource = resource_defs::logs_resources().remove(0);
        let model = carina_smithy::parse(
            r##"{
            "smithy":"2.0", "shapes": {
                "test#Service":{"type":"service","version":"2026-11-06"}
            }
        }"##,
        )
        .unwrap();
        let error = resource_tag_type(&resource, &model)
            .expect_err("complete models must not fall back to an unbounded map");
        assert!(
            error
                .to_string()
                .contains("logs.LogGroup has no tag source"),
            "{error}"
        );
    }

    #[test]
    fn map_length_codegen_preserves_modeled_non_tag_map_bounds() {
        let model = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "com.amazonaws.cloudwatchlogs#EntityAttributes":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":0,"max":10}}},
                "com.amazonaws.cloudwatchlogs#EntityKeyAttributes":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":2,"max":4}}},
                "com.amazonaws.iam#mapStringReplacementValueEntry":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":0,"max":30}}},
                "com.amazonaws.identitystore#Extensions":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":1,"max":10}}}
            }
        }"##).unwrap();
        for (namespace, shape, minimum, maximum) in [
            ("com.amazonaws.cloudwatchlogs", "EntityAttributes", 0, 10),
            ("com.amazonaws.cloudwatchlogs", "EntityKeyAttributes", 2, 4),
            ("com.amazonaws.iam", "mapStringReplacementValueEntry", 0, 30),
            ("com.amazonaws.identitystore", "Extensions", 1, 10),
        ] {
            let code = map_length_fixture_type(&model, &format!("{namespace}#{shape}"));
            assert!(
                code.starts_with("AttributeType::refined_map(AttributeType::string(), "),
                "{shape}: {code}"
            );
            assert!(
                code.ends_with(&format!(", Some((Some({minimum}), Some({maximum}))))")),
                "{shape}: {code}"
            );
        }
    }

    fn map_length_fixture_type(model: &SmithyModel, target: &str) -> String {
        resolve_type(
            &mut TypeResolutionContext {
                model,
                resource_name: "test.MapLength",
                namespace: "aws.test.MapLength",
                struct_path: Vec::new(),
                resolving_shapes: Vec::new(),
                property_path: "entries".to_string(),
                type_overrides: &HashMap::new(),
                enum_alias_map: &HashMap::new(),
                to_dsl_overrides: &HashMap::new(),
                all_enums: &mut BTreeMap::new(),
                all_ranged_ints: &mut BTreeMap::new(),
                current_root_attr: None,
                deferred_populate_struct_fields: &HashSet::new(),
                nested_write_only_struct_fields: &HashSet::new(),
                nested_write_only_emissions: &mut HashMap::new(),
                struct_field_type_overrides: &HashMap::new(),
            },
            target,
            "Entries",
        )
        .expect("resolve map fixture")
        .0
    }

    fn map_length_tag_fixture(resource_name: &str) -> (ResourceDef, SmithyModel) {
        let (resource, mut model, member_name, target) = match resource_name {
            "logs.LogGroup" => {
                let (resource, model) = logs_log_group_value_fixture();
                (resource, model, "tags", "com.amazonaws.cloudwatchlogs#Tags")
            }
            "iam.Role" => {
                let (mut resource, model) = iam_role_value_fixture();
                resource.has_tags = true;
                (resource, model, "Tags", "com.amazonaws.iam#tagListType")
            }
            "acm.Certificate" => (
                resource_defs::acm_resources().remove(0),
                markdown_read_shape_override_model(),
                "Tags",
                "com.amazonaws.acm#TagList",
            ),
            "ec2.Vpc" => (
                resource_defs::ec2_resources()
                    .into_iter()
                    .find(|resource| resource.name == "ec2.Vpc")
                    .unwrap(),
                carina_smithy::parse(r##"{
                    "smithy":"2.0", "shapes": {
                        "com.amazonaws.ec2#CreateVpc":{"type":"operation","input":{"target":"com.amazonaws.ec2#CreateVpcRequest"}},
                        "com.amazonaws.ec2#CreateVpcRequest":{"type":"structure","members":{"CidrBlock":{"target":"smithy.api#String"},"InstanceTenancy":{"target":"smithy.api#String"},"Ipv4IpamPoolId":{"target":"smithy.api#String"},"Ipv4NetmaskLength":{"target":"smithy.api#Integer"}}},
                        "com.amazonaws.ec2#ModifyVpcAttribute":{"type":"operation","input":{"target":"com.amazonaws.ec2#ModifyVpcAttributeRequest"}},
                        "com.amazonaws.ec2#ModifyVpcAttributeRequest":{"type":"structure","members":{"EnableDnsHostnames":{"target":"smithy.api#Boolean"},"EnableDnsSupport":{"target":"smithy.api#Boolean"}}},
                        "com.amazonaws.ec2#Vpc":{"type":"structure","members":{"CidrBlock":{"target":"smithy.api#String"},"InstanceTenancy":{"target":"smithy.api#String"},"VpcId":{"target":"smithy.api#String"},"Tags":{"target":"com.amazonaws.ec2#TagList"}}}
                    }
                }"##).unwrap(),
                "TagSpecifications",
                "com.amazonaws.ec2#TagSpecificationList",
            ),
            name => panic!("unknown tag fixture resource {name}"),
        };
        let tags = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "com.amazonaws.cloudwatchlogs#Tags":{"type":"map","key":{"target":"com.amazonaws.cloudwatchlogs#TagKey"},"value":{"target":"com.amazonaws.cloudwatchlogs#TagValue"},"traits":{"smithy.api#length":{"min":1,"max":50}}},
                "com.amazonaws.cloudwatchlogs#TagKey":{"type":"string","traits":{"smithy.api#length":{"min":1,"max":128},"smithy.api#pattern":"^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]+)$"}},
                "com.amazonaws.cloudwatchlogs#TagValue":{"type":"string","traits":{"smithy.api#length":{"min":0,"max":256},"smithy.api#pattern":"^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$"}},
                "com.amazonaws.iam#tagListType":{"type":"list","member":{"target":"com.amazonaws.iam#Tag"},"traits":{"smithy.api#length":{"min":0,"max":50}}},
                "com.amazonaws.iam#Tag":{"type":"structure","members":{"Key":{"target":"com.amazonaws.iam#tagKeyType"},"Value":{"target":"com.amazonaws.iam#tagValueType"}}},
                "com.amazonaws.iam#tagKeyType":{"type":"string","traits":{"smithy.api#length":{"min":1,"max":128},"smithy.api#pattern":"^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]+$"}},
                "com.amazonaws.iam#tagValueType":{"type":"string","traits":{"smithy.api#length":{"min":0,"max":256},"smithy.api#pattern":"^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*$"}},
                "com.amazonaws.acm#TagList":{"type":"list","member":{"target":"com.amazonaws.acm#Tag"},"traits":{"smithy.api#length":{"min":1,"max":50}}},
                "com.amazonaws.acm#Tag":{"type":"structure","members":{"Key":{"target":"com.amazonaws.acm#TagKey"},"Value":{"target":"com.amazonaws.acm#TagValue"}}},
                "com.amazonaws.acm#TagKey":{"type":"string","traits":{"smithy.api#length":{"min":1,"max":128},"smithy.api#pattern":"^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$"}},
                "com.amazonaws.acm#TagValue":{"type":"string","traits":{"smithy.api#length":{"max":256},"smithy.api#pattern":"^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$"}},
                "com.amazonaws.ec2#TagSpecificationList":{"type":"list","member":{"target":"com.amazonaws.ec2#TagSpecification"}},
                "com.amazonaws.ec2#TagSpecification":{"type":"structure","members":{"Tags":{"target":"com.amazonaws.ec2#TagList"}}},
                "com.amazonaws.ec2#TagList":{"type":"list","member":{"target":"com.amazonaws.ec2#Tag"}},
                "com.amazonaws.ec2#Tag":{"type":"structure","members":{"Key":{"target":"smithy.api#String"},"Value":{"target":"smithy.api#String"}}}
            }
        }"##).unwrap();
        model.shapes.extend(tags.shapes);
        let operation_id = format!("{}#{}", resource.service_namespace, resource.create_op);
        let Some(Shape::Operation(operation)) = model.get_shape(&operation_id) else {
            panic!("missing fixture operation {operation_id}");
        };
        let input_id = operation.input.as_ref().unwrap().target.clone();
        let Some(Shape::Structure(input)) = model.shapes.get_mut(&input_id) else {
            panic!("missing fixture input {input_id}");
        };
        input.members.insert(
            member_name.to_string(),
            carina_smithy::ShapeRef {
                target: target.to_string(),
                traits: Default::default(),
            },
        );
        (resource, model)
    }

    #[test]
    fn map_length_codegen_preserves_bounded_and_unbounded_maps() {
        let model = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "test#Bounded":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#Boolean"},"traits":{"smithy.api#length":{"min":1,"max":7}}},
                "test#Minimum":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":2}}},
                "test#Maximum":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"max":9}}},
                "test#Plain":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#Boolean"}}
            }
        }"##).unwrap();
        for (target, expected) in [
            (
                "test#Bounded",
                "AttributeType::refined_map(AttributeType::string(), AttributeType::bool(), Some((Some(1), Some(7))))",
            ),
            (
                "test#Minimum",
                "AttributeType::refined_map(AttributeType::string(), AttributeType::string(), Some((Some(2), None)))",
            ),
            (
                "test#Maximum",
                "AttributeType::refined_map(AttributeType::string(), AttributeType::string(), Some((None, Some(9))))",
            ),
            ("test#Plain", "AttributeType::map(AttributeType::bool())"),
        ] {
            assert_eq!(
                map_length_fixture_type(&model, target),
                expected,
                "{target}"
            );
        }
    }

    #[test]
    fn map_length_tags_are_derived_from_each_resource_smithy_model() {
        for ((resource, model), minimum, maximum, key_length, value_length) in [
            (
                map_length_tag_fixture("logs.LogGroup"),
                Some(1),
                Some(50),
                "Some((Some(1), Some(128)))",
                "Some((Some(0), Some(256)))",
            ),
            (
                map_length_tag_fixture("iam.Role"),
                Some(0),
                Some(50),
                "Some((Some(1), Some(128)))",
                "Some((Some(0), Some(256)))",
            ),
            (
                map_length_tag_fixture("acm.Certificate"),
                Some(1),
                Some(50),
                "Some((Some(1), Some(128)))",
                "Some((None, Some(256)))",
            ),
            (
                map_length_tag_fixture("ec2.Vpc"),
                None,
                None,
                "None",
                "None",
            ),
        ] {
            let generated = generate_resource(&resource, &model).unwrap();
            let expected = if minimum.is_some() || maximum.is_some() {
                let key_pattern = if resource.name == "iam.Role" {
                    r"^[\p{L}\p{Z}\p{N}_.:/=+\-@]+$"
                } else if resource.name == "acm.Certificate" {
                    r"^([\p{L}\p{Z}\p{N}_.:/=+\-@]*)$"
                } else {
                    r"^([\p{L}\p{Z}\p{N}_.:/=+\-@]+)$"
                };
                let value_pattern = if resource.name == "iam.Role" {
                    r"^[\p{L}\p{Z}\p{N}_.:/=+\-@]*$"
                } else {
                    r"^([\p{L}\p{Z}\p{N}_.:/=+\-@]*)$"
                };
                format!(
                    "AttributeType::refined_map(AttributeType::refined_string(None, Some(carina_core::schema::PatternSpec::new({key_pattern:?}, carina_core::schema::PatternMatch::Substring {{ reason: carina_core::schema::PatternReason::new(\"Smithy tag patterns retain their authored search semantics.\").expect(\"visible pattern reason\") }})), {key_length}, None), AttributeType::refined_string(None, Some(carina_core::schema::PatternSpec::new({value_pattern:?}, carina_core::schema::PatternMatch::Substring {{ reason: carina_core::schema::PatternReason::new(\"Smithy tag patterns retain their authored search semantics.\").expect(\"visible pattern reason\") }})), {value_length}, None), Some(({minimum:?}, {maximum:?})))"
                )
            } else {
                "AttributeType::map(AttributeType::string())".to_string()
            };
            assert!(
                generated.contains(&format!("AttributeSchema::optional(\"tags\", {expected})")),
                "{}: {generated}",
                resource.name
            );
            assert!(!generated.contains("tags_type()"), "{}", resource.name);
        }
    }

    #[test]
    fn map_length_docgen_displays_map_and_key_constraints() {
        let attr_type = AttributeType::refined_map(
            AttributeType::refined_string(
                None,
                Some(carina_core::schema::PatternSpec::new(
                    "^[a-z]+$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(1), Some(8))),
                None,
            ),
            AttributeType::string(),
            Some((Some(2), Some(5))),
        );
        let display = discover_markdown_attribute_type(
            &attr_type,
            &HeadingPath::root("entries"),
            "aws.test.MapLength",
            &mut BTreeMap::new(),
            &mut MarkdownTypes::default(),
            None,
        )
        .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        let markdown = page.finish().unwrap();
        assert!(
            markdown.contains("`String (pattern: /^[a-z]+$/ (Full), len: 1..=8)`"),
            "{markdown}"
        );
        assert!(markdown.contains("(entries: 2..=5)"), "{markdown}");
    }

    #[test]
    fn map_length_tags_reject_conflicting_operation_shapes() {
        let resource = resource_defs::logs_resources().remove(0);
        let model = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "com.amazonaws.cloudwatchlogs#CreateLogGroup":{"type":"operation","input":{"target":"test#Create"}},
                "test#Create":{"type":"structure","members":{"tags":{"target":"test#CreateTags"}}},
                "com.amazonaws.cloudwatchlogs#TagResource":{"type":"operation","input":{"target":"test#Tag"}},
                "test#Tag":{"type":"structure","members":{"tags":{"target":"test#UpdateTags"}}},
                "test#CreateTags":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":1,"max":50}}},
                "test#UpdateTags":{"type":"map","key":{"target":"smithy.api#String"},"value":{"target":"smithy.api#String"},"traits":{"smithy.api#length":{"min":1,"max":10}}}
            }
        }"##).unwrap();
        let error = resource_tag_type(&resource, &model)
            .expect_err("different tag bounds must stop generation");
        for expected in [
            "logs.LogGroup",
            "test#CreateTags",
            "test#UpdateTags",
            "conflicting tag bounds",
        ] {
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn map_length_tags_follow_changes_to_the_resource_model() {
        let (resource, mut model) = map_length_tag_fixture("iam.Role");
        let Some(Shape::List(shape)) = model.shapes.get_mut("com.amazonaws.iam#tagListType") else {
            panic!("IAM tag list");
        };
        shape.traits.get_mut("smithy.api#length").unwrap()["max"] = 17.into();
        let generated = generate_resource(&resource, &model).unwrap();
        assert!(
            generated.contains("Some((Some(0), Some(17)))"),
            "{generated}"
        );
    }

    #[test]
    fn generated_integer_inputs_are_narrowed_before_guarded_parts() {
        let model = carina_smithy::parse(r##"{
            "smithy":"2.0", "shapes": {
                "com.amazonaws.test#CreateFixture":{"type":"operation","input":{"target":"com.amazonaws.test#Request"}},
                "com.amazonaws.test#Request":{"type":"structure","members":{
                    "Count":{"target":"smithy.api#Integer"},
                    "WideCount":{"target":"smithy.api#Long"},
                    "Options":{"target":"com.amazonaws.test#Payload"}
                }},
                "com.amazonaws.test#ReadFixture":{"type":"structure","members":{
                    "Count":{"target":"smithy.api#Integer"},
                    "WideCount":{"target":"smithy.api#Long"},
                    "Options":{"target":"com.amazonaws.test#Payload"}
                }},
                "com.amazonaws.test#Payload":{"type":"structure","members":{"Items":{"target":"com.amazonaws.test#Items"}}},
                "com.amazonaws.test#Items":{"type":"list","member":{"target":"com.amazonaws.test#Item"}},
                "com.amazonaws.test#Item":{"type":"structure","members":{"Count":{"target":"smithy.api#Integer"},"WideCount":{"target":"smithy.api#Long"}}}
            }
        }"##).unwrap();
        let resource = provider_fixture_resource(
            "test.IntegerSendFixture",
            "",
            &["Count", "WideCount", "Options"],
            vec![],
        );
        let dispositions = [
            resource_defs::SendDisposition::sent("Count"),
            resource_defs::SendDisposition::sent("WideCount"),
            resource_defs::SendDisposition::sent("Options"),
        ];
        let code = generate_send_models_code(
            &[(&resource, &dispositions)],
            &HashMap::from([(resource.service_namespace, model)]),
        )
        .unwrap();
        assert!(code.contains("count: Option<i32>"), "{code}");
        assert!(
            code.contains("optional_i32(_resource, \"count\")?"),
            "{code}"
        );
        assert!(code.contains("wide_count: Option<i64>"), "{code}");
        assert!(
            code.contains("optional_int(_resource, \"wide_count\")?"),
            "{code}"
        );
        let decode = code
            .split("pub(crate) fn decode")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn sent")
            .next()
            .unwrap();
        assert!(
            decode.contains("check_i32_value") && decode.contains("[\"items\", \"[]\", \"count\"]"),
            "{decode}"
        );
        assert!(!decode.contains("Sent::new"), "{decode}");
    }

    #[test]
    fn smithy_integer_bounds_preserve_int_enum_and_contained_range_constructors() {
        let model = carina_smithy::parse(
            r##"{
            "smithy": "2.0", "shapes": {"test#Integer": {"type":"integer"}}
        }"##,
        )
        .unwrap();
        let retention = resource_value_override("logs.LogGroup", "retention_in_days").unwrap();
        let original = retention.type_code.clone();
        let retention = bound_int_override(&model, "test#Integer", "Count", retention);
        assert_eq!(retention.type_code, original);
        assert!(matches!(
            retention.attribute_type.raw_shape(),
            RawShape::Int { range: None, .. }
        ));

        for (minimum, maximum) in [(0, 100), (i64::from(i32::MIN), i64::from(i32::MAX))] {
            let original = format!(
                "AttributeType::refined_int(None, Some((Some({minimum}), Some({maximum}))))"
            );
            let bounded = bound_int_override(
                &model,
                "test#Integer",
                "Count",
                ValueTypeOverride {
                    type_code: original.clone(),
                    documentation: None,
                    attribute_type: AttributeType::refined_int(
                        None,
                        Some((Some(minimum), Some(maximum))),
                    ),
                },
            );
            assert_eq!(bounded.type_code, original);
        }
    }

    #[test]
    fn smithy_range_trait_without_min_or_max_renders_no_constraint() {
        let model = carina_smithy::parse(
            r##"{
            "smithy": "2.0", "shapes": {
                "test#Unbounded": {"type":"long", "traits":{"smithy.api#range":{}}}
            }
        }"##,
        )
        .unwrap();
        let range = get_int_range(&model, "test#Unbounded", "Count").unwrap();
        assert_eq!((range.min, range.max), (None, None));
        let display = discover_markdown_type(
            &model,
            "test.Fixture",
            "test#Unbounded",
            "Count",
            &HeadingPath::root("count"),
            "count",
            "aws.test.Fixture",
            &HashMap::new(),
            &HashMap::new(),
            None,
            &mut BTreeMap::new(),
            &mut MarkdownTypes::default(),
            &mut HashMap::new(),
        )
        .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        page.table(["Type"], [[display.table_cell()]]).unwrap();
        assert_eq!(
            page.finish().unwrap(),
            "`Int`\n\n| Type |\n|------|\n| `Int` |\n"
        );
    }

    #[test]
    fn smithy_integer_ranges_intersect_width_traits_and_known_overrides() {
        let model = carina_smithy::parse(r##"{
            "smithy": "2.0", "shapes": {
                "test#Integer": {"type":"integer"},
                "test#PositiveInteger": {"type":"integer", "traits":{"smithy.api#range":{"min":1,"max":9223372036854775807}}},
                "test#Long": {"type":"long"},
                "test#PositiveLong": {"type":"long", "traits":{"smithy.api#range":{"min":1,"max":9223372036854775807}}}
            }
        }"##).unwrap();
        for target in ["test#Integer", "smithy.api#Integer"] {
            let range = get_int_range(&model, target, "Count")
                .expect("Integer must have its carrier bounds");
            assert_eq!(
                (range.min, range.max),
                (Some(i64::from(i32::MIN)), Some(i64::from(i32::MAX)))
            );
        }
        let range = get_int_range(&model, "test#PositiveInteger", "Count").unwrap();
        assert_eq!((range.min, range.max), (Some(1), Some(i64::from(i32::MAX))));
        let range = get_int_range(&model, "test#PositiveInteger", "FromPort").unwrap();
        assert_eq!((range.min, range.max), (Some(1), Some(65535)));
        assert!(get_int_range(&model, "test#Long", "Count").is_none());
        let range = get_int_range(&model, "test#PositiveLong", "Count").unwrap();
        assert_eq!((range.min, range.max), (Some(1), Some(i64::MAX)));
        for (target, maximum) in [
            ("test#Integer", i64::from(i32::MAX)),
            ("test#Long", i64::MAX),
        ] {
            let value = bound_int_override(&model, target, "Count", ValueTypeOverride {
                type_code: "AttributeType::refined_int(None, Some((Some(1), Some(9223372036854775807))))".to_string(),
                documentation: None,
                attribute_type: AttributeType::refined_int(None, Some((Some(1), Some(i64::MAX)))),
            });
            assert!(matches!(value.attribute_type.raw_shape(), RawShape::Int {
                range: Some((Some(1), Some(actual))), ..
            } if actual == maximum));
        }
    }

    #[test]
    fn s3_full_paths_refine_handwritten_helpers_and_keep_other_resources_unchanged() {
        // Keep this fixture self-contained: the Smithy download cache is absent in CI.
        let model = carina_smithy::parse(r##"{
            "smithy": "2.0",
            "shapes": {
                "com.amazonaws.s3#PutBucketCors": { "type": "operation", "input": { "target": "com.amazonaws.s3#Request" } },
                "com.amazonaws.s3#PutBucketLifecycleConfiguration": { "type": "operation", "input": { "target": "com.amazonaws.s3#Request" } },
                "com.amazonaws.s3#PutBucketReplication": { "type": "operation", "input": { "target": "com.amazonaws.s3#Request" } },
                "com.amazonaws.s3#Request": { "type": "structure", "members": { "Bucket": { "target": "smithy.api#String" } } }
            }
        }"##).unwrap();
        for resource in resource_defs::s3_resources()
            .into_iter()
            .filter(|resource| {
                matches!(
                    resource.name,
                    "s3.BucketCorsConfiguration"
                        | "s3.BucketLifecycleConfiguration"
                        | "s3.BucketReplicationConfiguration"
                )
            })
        {
            let generated = generate_resource(&resource, &model).unwrap();
            let constructor = match resource.name {
                "s3.BucketCorsConfiguration" => "super::bucket_cors_rules()",
                "s3.BucketLifecycleConfiguration" => "super::bucket_lifecycle_rules()",
                "s3.BucketReplicationConfiguration" => "super::bucket_replication_rules()",
                _ => unreachable!("fixture resource filter"),
            };
            assert!(generated.contains(constructor), "{generated}");
        }
        assert!(resource_value_override("s3.Other", "rules[].id").is_none());
        assert!(
            resource_value_override(
                "s3.BucketLifecycleConfiguration",
                "rules[].noncurrent_version_transitions[].noncurrent_days"
            )
            .is_none()
        );
        let markdown = lifecycle_markdown_fixture();
        assert!(markdown.contains("`Int (range: 1..=100)`"), "{markdown}");
        assert!(
            markdown.contains("`Int (range: 1..=2147483647)`"),
            "{markdown}"
        );
        assert!(markdown.contains("`String (len: 0..=255)`"), "{markdown}");
    }

    use super::*;

    fn route53_value_fixture() -> (ResourceDef, SmithyModel) {
        let resource = resource_defs::route53_resources().remove(0);
        let model = carina_smithy::parse(r##"{
            "smithy": "2.0",
            "shapes": {
                "com.amazonaws.route53#ChangeResourceRecordSets": {"type": "operation", "input": {"target": "com.amazonaws.route53#ChangeResourceRecordSetsRequest"}},
                "com.amazonaws.route53#ChangeResourceRecordSetsRequest": {"type": "structure", "members": {"HostedZoneId": {"target": "com.amazonaws.route53#ResourceId", "traits": {"smithy.api#required": {}}}}},
                "com.amazonaws.route53#ResourceRecordSet": {
                    "type": "structure",
                    "members": {
                        "Name": {"target": "com.amazonaws.route53#DNSName", "traits": {"smithy.api#required": {}}},
                        "Type": {"target": "com.amazonaws.route53#RRType", "traits": {"smithy.api#required": {}}},
                        "AliasTarget": {"target": "com.amazonaws.route53#AliasTarget"},
                        "ResourceRecords": {"target": "com.amazonaws.route53#ResourceRecords"},
                        "TTL": {"target": "com.amazonaws.route53#TTL"}
                    }
                },
                "com.amazonaws.route53#AliasTarget": {
                    "type": "structure",
                    "members": {
                        "DNSName": {"target": "com.amazonaws.route53#DNSName", "traits": {"smithy.api#required": {}}},
                        "HostedZoneId": {"target": "com.amazonaws.route53#ResourceId", "traits": {"smithy.api#required": {}}},
                        "EvaluateTargetHealth": {"target": "smithy.api#Boolean", "traits": {"smithy.api#required": {}}}
                    }
                },
                "com.amazonaws.route53#ResourceRecords": {"type": "list", "member": {"target": "com.amazonaws.route53#ResourceRecord"}, "traits": {"smithy.api#length": {"min": 1}}},
                "com.amazonaws.route53#ResourceRecord": {"type": "structure", "members": {"Value": {"target": "com.amazonaws.route53#RData", "traits": {"smithy.api#required": {}}}}},
                "com.amazonaws.route53#DNSName": {"type": "string", "traits": {"smithy.api#length": {"min": 0, "max": 1024}}},
                "com.amazonaws.route53#RData": {"type": "string", "traits": {"smithy.api#length": {"min": 0, "max": 4000}}},
                "com.amazonaws.route53#ResourceId": {"type": "string", "traits": {"smithy.api#length": {"min": 0, "max": 32}}},
                "com.amazonaws.route53#TTL": {"type": "long", "traits": {"smithy.api#range": {"min": 0, "max": 2147483647}}},
                "com.amazonaws.route53#RRType": {"type": "enum", "members": {"A": {"target": "smithy.api#Unit", "traits": {"smithy.api#enumValue": "A"}}, "CNAME": {"target": "smithy.api#Unit", "traits": {"smithy.api#enumValue": "CNAME"}}}}
            }
        }"##).unwrap();
        (resource, model)
    }

    #[test]
    fn acm_and_route53_dns_overrides_share_one_bounded_constructor() {
        for (resource, path) in [
            ("route53.RecordSet", "name"),
            ("route53.RecordSet", "alias_target.dns_name"),
            (
                "acm.Certificate",
                "domain_validation_options[].resource_record.name",
            ),
            (
                "acm.Certificate",
                "domain_validation_options[].resource_record.value",
            ),
            (
                "acm.Certificate",
                "renewal_summary.domain_validation_options[].resource_record.name",
            ),
            (
                "acm.Certificate",
                "renewal_summary.domain_validation_options[].resource_record.value",
            ),
        ] {
            let value = resource_value_override(resource, path).unwrap();
            assert_eq!(
                value.type_code, "carina_aws_types::route53_dns_name()",
                "{resource}.{path}"
            );
            let RawShape::String {
                identity,
                pattern,
                length,
                ..
            } = value.attribute_type.raw_shape()
            else {
                panic!("{resource}.{path}: DNS String");
            };
            assert!(identity.is_none());
            assert!(pattern.is_some());
            assert_eq!(length, Some((Some(1), Some(1024))));
        }
    }

    #[test]
    fn route53_codegen_refines_only_exact_paths_and_preserves_send_carriers() {
        let (resource, model) = route53_value_fixture();
        let attrs = collect_resource_attributes(&resource, &model)
            .unwrap()
            .attrs;
        for (path, send_type) in [
            ("name", SendFieldType::String),
            ("hosted_zone_id", SendFieldType::String),
            ("resource_records", SendFieldType::StringList),
        ] {
            let attr = attrs.iter().find(|attr| attr.snake_name == path).unwrap();
            assert_eq!(attr.send_type, send_type, "{path}");
            assert_ne!(attr.type_code, PLAIN_STRING_TYPE_CODE, "{path}");
        }
        let generated = generate_resource(&resource, &model).unwrap();
        assert!(generated.contains("carina_aws_types::route53_hosted_zone_id()"));
        assert!(generated.contains("Some(4000)"), "{generated}");
        assert!(
            generated.contains(
                "StructField::required(\"dns_name\", carina_aws_types::route53_dns_name())"
            )
        );
        assert!(
            generated.contains(
                "AttributeSchema::required(\"name\", carina_aws_types::route53_dns_name())"
            )
        );
        for path in [
            "name",
            "alias_target.dns_name",
            "hosted_zone_id",
            "resource_records[]",
        ] {
            assert!(
                resource_value_override(resource.name, path).is_some(),
                "{path}"
            );
            assert!(
                resource_value_override("route53.Other", path).is_none(),
                "{path}"
            );
        }
        for path in [
            "nested.name",
            "alias_target.name",
            "dns_name",
            "other.resource_records[]",
            "alias_target.hosted_zone_id",
            "ttl",
            "type",
        ] {
            assert!(
                resource_value_override(resource.name, path).is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn route53_codegen_documents_the_dns_and_record_value_carriers() {
        let (resource, model) = route53_value_fixture();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        for expected in [
            "len: 1..=1024",
            "len: 0..=4000",
            "Route53HostedZoneId",
            "pattern:",
        ] {
            assert!(
                markdown.contains(expected),
                "missing {expected}: {markdown}"
            );
        }
    }
    use carina_core::schema::StructField;

    fn markdown_resource_example(dsl_name: &str) -> DocExample {
        let source = format!("aws.{dsl_name} {{}}\n");
        example::check(
            &source,
            Path::new("resource-markdown-fixture.crn"),
            "aws",
            dsl_name,
            ExampleKind::Resource,
        )
        .expect("checked resource Markdown fixture")
    }

    #[test]
    fn markdown_internet_gateway_vpc_id_keeps_nat_gateway_constraints() {
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateInternetGateway": {
                  "type": "operation", "input": {"target": "com.amazonaws.ec2#CreateInternetGatewayRequest"}
                },
                "com.amazonaws.ec2#CreateInternetGatewayRequest": {"type": "structure", "members": {}},
                "com.amazonaws.ec2#InternetGateway": {
                  "type": "structure", "members": {"InternetGatewayId": {"target": "com.amazonaws.ec2#String"}}
                },
                "com.amazonaws.ec2#CreateNatGateway": {
                  "type": "operation", "input": {"target": "com.amazonaws.ec2#CreateNatGatewayRequest"}
                },
                "com.amazonaws.ec2#CreateNatGatewayRequest": {
                  "type": "structure", "members": {"VpcId": {"target": "com.amazonaws.ec2#String"}}
                },
                "com.amazonaws.ec2#NatGateway": {
                  "type": "structure", "members": {"NatGatewayId": {"target": "com.amazonaws.ec2#String"}}
                },
                "com.amazonaws.ec2#String": {"type": "string"}
              }
            }"##,
        )
        .unwrap();
        let vpc_id_type = |name| {
            let resource = resource_defs::ec2_resources()
                .into_iter()
                .find(|resource| resource.name == name)
                .unwrap();
            let markdown = generate_markdown_resource(
                &resource,
                &model,
                &markdown_resource_example(resource.name),
            )
            .unwrap();
            markdown
                .split("### `vpc_id`\n")
                .nth(1)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("- **Type:** "))
                .unwrap()
                .to_string()
        };
        let nat_type = vpc_id_type("ec2.NatGateway");
        assert_eq!(
            nat_type,
            "`VpcId (pattern: /^vpc-[0-9a-f]{8,}$/ (Full), custom validator)`"
        );
        assert_eq!(vpc_id_type("ec2.InternetGateway"), nat_type);
    }

    #[test]
    fn markdown_s3_bucket_arn_matches_schema_type() {
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.s3#CreateBucket": {
                  "type": "operation", "input": {"target": "com.amazonaws.s3#CreateBucketRequest"}
                },
                "com.amazonaws.s3#CreateBucketRequest": {
                  "type": "structure", "members": {
                    "Bucket": {"target": "smithy.api#String", "traits": {"smithy.api#required": {}}},
                    "BucketNamespace": {"target": "smithy.api#String"}
                  }
                }
              }
            }"##,
        )
        .unwrap();
        let resource = resource_defs::s3_resources().remove(0);
        let schema = generate_resource(&resource, &model).unwrap();
        assert!(
            schema.contains(r#"AttributeSchema::provider_populated("arn", self::arn())"#),
            "{schema}"
        );
        let attributes = collect_resource_attributes(&resource, &model).unwrap();
        let arn = attributes
            .attrs
            .iter()
            .find(|attr| attr.snake_name == "arn")
            .unwrap();
        assert_eq!(arn.type_code, "super::arn()");
        let schema_display = discover_markdown_override(
            &arn.type_code,
            &HeadingPath::root("arn"),
            "aws.s3.Bucket",
            &mut BTreeMap::new(),
            &mut MarkdownTypes::default(),
        )
        .unwrap();
        let mut page = PageBuilder::new();
        schema_display.write_to(&mut page);
        let schema_type = page.finish().unwrap();
        assert_eq!(
            schema_type,
            "`Arn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):[^:]+:.*$/ (Full), custom validator)`"
        );
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        let docs_type = markdown
            .split("### `arn`\n")
            .nth(1)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("- **Type:** "))
            .unwrap();
        assert_eq!(docs_type, schema_type);
        let data_source = resource_defs::s3_data_sources().remove(0);
        let data_source_markdown = generate_markdown_data_source(
            &data_source,
            &model,
            &markdown_data_source_example(data_source.name),
        )
        .unwrap();
        assert!(data_source_markdown.contains(&format!("- **Type:** {schema_type}\n")));
    }

    fn markdown_data_source_example(dsl_name: &str) -> DocExample {
        let source = format!("let fixture = read aws.{dsl_name} {{}}\n");
        example::check(
            &source,
            Path::new("data-source-markdown-fixture.crn"),
            "aws",
            dsl_name,
            ExampleKind::DataSource,
        )
        .expect("checked data-source Markdown fixture")
    }

    fn logs_log_group_value_fixture() -> (ResourceDef, SmithyModel) {
        let mut resource = resource_defs::logs_resources().remove(0);
        resource.extra_read_only.push("nested");
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.cloudwatchlogs#CreateLogGroup": {
                  "type": "operation",
                  "input": {"target": "com.amazonaws.cloudwatchlogs#CreateLogGroupRequest"}
                },
                "com.amazonaws.cloudwatchlogs#CreateLogGroupRequest": {
                  "type": "structure",
                  "members": {
                    "logGroupName": {
                      "target": "com.amazonaws.cloudwatchlogs#LogGroupName",
                      "traits": {"smithy.api#required": {}}
                    },
                    "kmsKeyId": {"target": "smithy.api#String"},
                    "logGroupClass": {"target": "com.amazonaws.cloudwatchlogs#LogGroupClass"},
                    "deletionProtectionEnabled": {"target": "smithy.api#Boolean"}
                  }
                },
                "com.amazonaws.cloudwatchlogs#LogGroup": {
                  "type": "structure",
                  "members": {
                    "logGroupName": {"target": "com.amazonaws.cloudwatchlogs#LogGroupName"},
                    "kmsKeyId": {"target": "smithy.api#String"},
                    "logGroupClass": {"target": "com.amazonaws.cloudwatchlogs#LogGroupClass"},
                    "deletionProtectionEnabled": {"target": "smithy.api#Boolean"},
                    "retentionInDays": {"target": "smithy.api#Integer"},
                    "nested": {"target": "com.amazonaws.cloudwatchlogs#Nested"}
                  }
                },
                "com.amazonaws.cloudwatchlogs#Nested": {
                  "type": "structure",
                  "members": {
                    "logGroupName": {"target": "com.amazonaws.cloudwatchlogs#LogGroupName"},
                    "retentionInDays": {"target": "smithy.api#Integer"}
                  }
                },
                "com.amazonaws.cloudwatchlogs#LogGroupName": {
                  "type": "string",
                  "traits": {
                    "smithy.api#length": {"min": 1, "max": 512},
                    "smithy.api#pattern": "^[.\\-_/#A-Za-z0-9]+$"
                  }
                },
                "com.amazonaws.cloudwatchlogs#LogGroupClass": {
                  "type": "enum",
                  "members": {
                    "STANDARD": {
                      "target": "smithy.api#Unit",
                      "traits": {"smithy.api#enumValue": "STANDARD"}
                    }
                  }
                }
              }
            }"##,
        )
        .expect("parse Logs value-refinement fixture");
        (resource, model)
    }

    #[test]
    fn logs_log_group_codegen_refines_name_at_exact_path() {
        let (resource, model) = logs_log_group_value_fixture();
        let generated = generate_resource(&resource, &model).expect("generate Logs schema");
        assert!(
            generated.contains(
                "AttributeSchema::required(\"log_group_name\", AttributeType::refined_string("
            ),
            "{generated}"
        );
        assert!(
            generated.contains("Some((Some(1), Some(512)))"),
            "{generated}"
        );
        assert!(
            generated.contains(r#"Some(carina_core::schema::PatternSpec::new("^[.\\-_/#A-Za-z0-9]{1,512}\\z", carina_core::schema::PatternMatch::Full))"#),
            "match AWSCC's exact anonymous name refinement: {generated}"
        );
        assert!(
            generated
                .contains("StructField::optional(\"log_group_name\", AttributeType::string())"),
            "a nested name with the same Smithy target must stay unchanged: {generated}"
        );

        // Use a real resource definition and its actual Smithy member names.
        // SecurityGroup.Description is still an unrefined String after EC2's
        // GroupName override; this control also runs complete schema generation.
        let resource = resource_defs::ec2_resources()
            .into_iter()
            .find(|resource| resource.name == "ec2.SecurityGroup")
            .expect("SecurityGroup resource");
        let model = carina_smithy::parse(
            r#"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateSecurityGroup": {
                  "type": "operation",
                  "input": {"target": "com.amazonaws.ec2#CreateSecurityGroupRequest"}
                },
                "com.amazonaws.ec2#CreateSecurityGroupRequest": {
                  "type": "structure",
                  "members": {
                    "Description": {"target": "com.amazonaws.ec2#String", "traits": {"smithy.api#required": {}}},
                    "GroupName": {"target": "com.amazonaws.ec2#String", "traits": {"smithy.api#required": {}}},
                    "VpcId": {"target": "com.amazonaws.ec2#VpcId"}
                  }
                },
                "com.amazonaws.ec2#SecurityGroup": {
                  "type": "structure",
                  "members": {
                    "Description": {"target": "com.amazonaws.ec2#String"},
                    "GroupName": {"target": "com.amazonaws.ec2#String"},
                    "GroupId": {"target": "com.amazonaws.ec2#String"},
                    "VpcId": {"target": "com.amazonaws.ec2#String"}
                  }
                },
                "com.amazonaws.ec2#String": {"type": "string"},
                "com.amazonaws.ec2#VpcId": {"type": "string"}
              }
            }"#,
        )
        .expect("parse SecurityGroup control");
        let collected =
            collect_resource_attributes(&resource, &model).expect("collect unrelated resource");
        let description = collected
            .attrs
            .iter()
            .find(|attribute| attribute.snake_name == "description")
            .expect("SecurityGroup description");
        assert_eq!(description.type_code, PLAIN_STRING_TYPE_CODE);
        let control = generate_resource(&resource, &model).expect("generate SecurityGroup control");
        assert!(
            control.contains("AttributeSchema::required(\"description\", AttributeType::string())"),
            "{control}"
        );
    }

    #[test]
    fn logs_log_group_codegen_retention_uses_shared_integer_enum() {
        let (resource, model) = logs_log_group_value_fixture();
        let collected =
            collect_resource_attributes(&resource, &model).expect("collect Logs schema");
        let retention = collected
            .attrs
            .iter()
            .find(|attr| attr.snake_name == "retention_in_days")
            .expect("retention attribute");
        assert_eq!(
            retention.type_code,
            "carina_aws_types::logs_log_group_retention_in_days()"
        );
        assert_eq!(retention.send_type, SendFieldType::Int);
        let generated = generate_resource(&resource, &model).expect("generate Logs schema");
        // Smithy Integer is 32-bit; unranged Integers carry the i32 range (#588 S3 slice)
        assert!(
            generated.contains(concat!(
                "StructField::optional(\"retention_in_days\", AttributeType::refined_int(\n",
                "                None,\n",
                "                Some((Some(-2147483648), Some(2147483647))),\n",
                "            ))",
            )),
            "the override must not match nested suffixes: {generated}"
        );
    }

    #[test]
    fn logs_log_group_codegen_markdown_documents_value_constraints() {
        let (resource, model) = logs_log_group_value_fixture();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate Logs markdown");
        assert!(
            markdown.contains(
                "`String (pattern: /^[.\\-_/#A-Za-z0-9]{1,512}\\z/ (Full), len: 1..=512)`"
            ),
            "{markdown}"
        );
        assert!(
            markdown.contains("`LogsLogGroupRetentionInDays (custom validator)`; allowed: 1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096, 1827, 2192, 2557, 2922, 3288, 3653"),
            "{markdown}"
        );
        assert!(
            markdown.contains("| `log_group_name` | `String` |"),
            "{markdown}"
        );
        // Smithy Integer is 32-bit; unranged Integers carry the i32 range (#588 S3 slice)
        assert!(
            markdown.contains("| `retention_in_days` | `Int (range: -2147483648..=2147483647)` |"),
            "{markdown}"
        );
    }

    fn iam_role_value_fixture() -> (ResourceDef, SmithyModel) {
        let fields = [("RoleName", true), ("Path", false), ("Description", false)];
        let model = string_fixture_model(&fields, &fields);
        let mut resource = provider_fixture_resource(
            "iam.Role",
            "RoleName",
            &["RoleName", "Path", "Description"],
            vec![],
        );
        resource.replacement = ReplacementClass::UniqueName("RoleName");
        (resource, model)
    }

    #[test]
    fn iam_role_codegen_refines_names_and_paths_without_changing_send_types() {
        let (resource, model) = iam_role_value_fixture();
        let attrs = collect_resource_attributes(&resource, &model)
            .unwrap()
            .attrs;
        for (field, constructor) in [
            ("role_name", "carina_aws_types::iam_role_name()"),
            ("path", "carina_aws_types::iam_path()"),
        ] {
            let attr = attrs.iter().find(|attr| attr.snake_name == field).unwrap();
            assert_eq!(attr.type_code, constructor);
            assert_eq!(attr.send_type, SendFieldType::String);
        }
        let generated = generate_resource(&resource, &model).unwrap();
        assert!(
            generated.contains(".with_unique_name_attribute(\"role_name\")"),
            "{generated}"
        );
        assert!(
            generated.contains(
                "AttributeSchema::required(\"role_name\", carina_aws_types::iam_role_name())"
            ),
            "{generated}"
        );
        assert!(
            generated.contains("AttributeSchema::optional(\"path\", carina_aws_types::iam_path())"),
            "{generated}"
        );
        assert!(
            generated
                .contains("AttributeSchema::optional(\"description\", AttributeType::string())"),
            "{generated}"
        );

        let unrelated = ResourceDef {
            name: "iam.Other",
            ..resource
        };
        let attrs = collect_resource_attributes_unvalidated(&unrelated, &model, &[])
            .unwrap()
            .attrs;
        assert!(
            attrs
                .iter()
                .all(|attr| attr.type_code == PLAIN_STRING_TYPE_CODE)
        );
        for path in ["nested.role_name", "nested.path", "role_name[]", "path[]"] {
            assert!(
                resource_value_override("iam.Role", path).is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn iam_role_codegen_markdown_documents_name_and_path_constraints() {
        let (resource, model) = iam_role_value_fixture();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        assert!(
            markdown.contains(
                r"`IamRoleName (pattern: /^[A-Za-z0-9_+=,.@-]+\z/ (Full), len: 1..=64, custom validator)`"
            ),
            "{markdown}"
        );
        assert!(
            markdown
                .contains(r"`String (pattern: /^(?:/|/[\x21-\x7E]+/)\z/ (Full), len: 1..=512)`"),
            "{markdown}"
        );
    }

    #[test]
    fn organizations_account_codegen_refines_exact_paths_and_keeps_send_strings() {
        let resource = organizations_account_resource_definition();
        let model = organizations_account_fixture_model();
        let attrs = collect_resource_attributes(&resource, &model)
            .unwrap()
            .attrs;
        for (field, expected_constructor) in [
            ("account_name", "AttributeType::refined_string("),
            ("role_name", "carina_aws_types::iam_role_name()"),
            ("email", "carina_aws_types::organizations_account_email()"),
            ("id", "carina_aws_types::aws_account_id()"),
            ("arn", "carina_aws_types::organizations_account_arn()"),
        ] {
            let attr = attrs.iter().find(|attr| attr.snake_name == field).unwrap();
            assert!(
                attr.type_code.starts_with(expected_constructor),
                "{field}: {}",
                attr.type_code
            );
            assert_eq!(attr.send_type, SendFieldType::String);
        }
        let timestamp = attrs
            .iter()
            .find(|attr| attr.snake_name == "joined_timestamp")
            .unwrap();
        assert_eq!(timestamp.type_code, PLAIN_STRING_TYPE_CODE);
        for path in [
            "nested.account_name",
            "nested.role_name",
            "account_name[]",
            "role_name[]",
            "nested.email",
            "email[]",
            "nested.id",
            "nested.arn",
            "joined_timestamp",
        ] {
            assert!(
                resource_value_override("organizations.Account", path).is_none(),
                "{path}"
            );
        }
        let output = resource_value_override("organizations.Organization", "master_account_email")
            .expect("management account output must share the Organizations Email type");
        assert_eq!(
            output.type_code,
            "carina_aws_types::organizations_account_email()"
        );
        for resource in [
            "organizations.Other",
            "acm.Certificate",
            "identitystore.User",
        ] {
            assert!(resource_value_override(resource, "email").is_none());
            assert!(resource_value_override(resource, "master_account_email").is_none());
        }
        for resource in ["organizations.Other", "iam.Role"] {
            assert!(resource_value_override(resource, "account_name").is_none());
        }
    }

    #[test]
    fn organizations_account_codegen_markdown_documents_name_constraints() {
        let resource = organizations_account_resource_definition();
        let markdown = generate_markdown_resource(
            &resource,
            &organizations_account_fixture_model(),
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        for expected in [
            r"`String (pattern: /^[ -;=?-~]+\z/ (Full), len: 1..=50)`",
            r"`IamRoleName (pattern: /^[A-Za-z0-9_+=,.@-]+\z/ (Full), len: 1..=64, custom validator)`",
            "`OrganizationsAccountEmail (len: 6..=64, custom validator)`",
            r"`AwsAccountId (pattern: /^\d{12}$/ (Full), len: 12..=12, custom validator)`",
            // #638: Organizations account ARN docs pin all eight supported partitions.
            r"`OrganizationsAccountArn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:account/o-[a-z0-9]{10,32}/[0-9]{12}\z/ (Full))`",
        ] {
            assert!(markdown.contains(expected), "{markdown}");
        }
    }

    #[test]
    fn organizations_organization_codegen_refines_id_and_documents_account_arn() {
        let fields = [
            ("Id", false),
            ("Arn", false),
            ("MasterAccountArn", false),
            ("MasterAccountId", false),
        ];
        let model = string_fixture_model(&[], &fields);
        let mut resource =
            provider_fixture_resource("organizations.Organization", "Id", &[], vec![]);
        resource.extra_read_only = vec!["Arn", "MasterAccountArn", "MasterAccountId"];
        let attrs = collect_resource_attributes(&resource, &model)
            .unwrap()
            .attrs;
        for (field, constructor) in [
            ("id", "AttributeType::refined_string("),
            ("arn", "carina_aws_types::organizations_organization_arn()"),
            (
                "master_account_arn",
                "carina_aws_types::organizations_account_arn()",
            ),
            ("master_account_id", "carina_aws_types::aws_account_id()"),
        ] {
            let attr = attrs.iter().find(|attr| attr.snake_name == field).unwrap();
            assert!(
                attr.type_code.starts_with(constructor),
                "{field}: {}",
                attr.type_code
            );
        }
        for path in [
            "nested.id",
            "nested.master_account_arn",
            "id[]",
            "master_account_arn[]",
        ] {
            assert!(resource_value_override(resource.name, path).is_none());
        }
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        for expected in [
            r"`String (pattern: /^o-[a-z0-9]{10,32}\z/ (Full), len: 12..=34)`",
            // #638: Management-account ARN docs pin all eight supported partitions.
            r"`OrganizationsAccountArn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:account/o-[a-z0-9]{10,32}/[0-9]{12}\z/ (Full))`",
            // #638: Organization ARN docs pin all eight supported partitions.
            r"`OrganizationsOrganizationArn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:organization/o-[a-z0-9]{10,32}\z/ (Full))`",
        ] {
            assert!(markdown.contains(expected), "{markdown}");
        }
    }

    const PROVIDER_FIXTURE_FIELDS: &[&str] = &[
        "AllocationId",
        "AvailabilityMode",
        "CidrBlock",
        "Description",
        "Name",
        "Options",
        "Path",
        "PeerOwnerId",
        "PeerVpcId",
        "PrivateDnsNameOptionsOnLaunch",
        "RoleName",
        "RouteTableIds",
        "SecurityGroupIds",
        "SubnetIds",
        "TransitGatewayId",
        "Unobservable",
        "VpcId",
    ];

    fn empty_test_model() -> SmithyModel {
        carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).expect("parse empty test model")
    }

    fn union_fixture() -> (ResourceDef, SmithyModel) {
        let mut resource = provider_fixture_resource("ec2.RouteTable", "", &[], vec![]);
        resource.read_structure = Some("Fixture");
        let model = carina_smithy::parse(include_str!("../tests/fixtures/unions.json"))
            .expect("parse union fixture");
        (resource, model)
    }

    #[test]
    fn smithy_union_schema_preserves_members_and_exactly_one_group() {
        let (resource, model) = union_fixture();
        let code = generate_resource(&resource, &model).expect("generate union schema");
        assert_eq!(
            code.matches("AttributeType::struct_with_field_groups(")
                .count(),
            4,
            "{code}"
        );
        assert_eq!(
            code.matches("FieldGroup::exactly_one([\"details\", \"flag\", \"text\"])")
                .count(),
            3,
            "{code}"
        );
        assert_eq!(
            code.matches("StructField::optional(\"text\", AttributeType::string())")
                .count(),
            3,
            "{code}"
        );
        assert!(
            code.contains("StructField::optional(\"details\", AttributeType::struct_("),
            "{code}"
        );
        // Smithy Integer is 32-bit; unranged Integers carry the i32 range (#588 S3 slice)
        assert!(
            code.contains(concat!(
                "StructField::optional(\"count\", AttributeType::refined_int(\n",
                "                None,\n",
                "                Some((Some(-2147483648), Some(2147483647))),\n",
                "            ))",
            )),
            "{code}"
        );
        assert!(code.contains("aws.ec2.RouteTable.Choice.Details"), "{code}");
        assert!(
            code.contains(".with_description(\"A literal value.\").with_provider_name(\"Text\")"),
            "{code}"
        );
        assert!(
            code.contains("ResourceSchema, StructField, FieldGroup}"),
            "{code}"
        );
    }

    #[test]
    fn smithy_union_single_member_schema_requires_its_member_without_group() {
        let (resource, model) = union_fixture();
        let code = generate_resource(&resource, &model).expect("generate singleton union schema");
        let single = code
            .split("\"SingleChoice\".to_string()")
            .nth(1)
            .expect("singleton struct");
        assert!(
            single.contains("StructField::required(\"text\", AttributeType::string())"),
            "{single}"
        );
        assert!(!single.contains("FieldGroup::"), "{single}");
    }

    #[test]
    fn smithy_union_enum_collections_emit_complete_enum_expressions() {
        let (resource, model) = union_fixture();
        let generated =
            generate_resource(&resource, &model).expect("generate enum collection union");
        let compact: String = generated
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        for (field, name, wrapper) in [
            ("enum_list", "EnumList", "list"),
            ("enum_map", "EnumMap", "map"),
        ] {
            let enum_expression = format!(
                concat!(
                    "AttributeType::enum_(carina_core::schema::enum_identity(\"{}\",Some(\"aws.ec2.RouteTable.EnumCollections\")),",
                    "Some(vec![\"FAST\".to_string()]),vec![(\"FAST\".to_string(),\"fast\".to_string())],None,None,)"
                ),
                name
            );
            let expected = format!(
                "StructField::optional(\"{field}\",AttributeType::{wrapper}({enum_expression}))"
            );
            assert!(
                compact.contains(&expected),
                "missing complete {wrapper} enum expression: {generated}"
            );
        }
        assert!(
            !generated.contains("/* enum */"),
            "unrendered enum in generated schema: {generated}"
        );
    }

    #[test]
    fn smithy_union_recursive_schema_reports_the_shape_cycle() {
        let (resource, _model) = union_fixture();
        let model = carina_smithy::parse(include_str!("../tests/fixtures/recursive-unions.json"))
            .expect("parse recursive fixture");
        let error =
            generate_resource(&resource, &model).expect_err("recursive schemas must fail codegen");
        assert_eq!(
            error.to_string(),
            "Recursive Smithy shape cycle: com.amazonaws.test#Choice -> com.amazonaws.test#Node -> com.amazonaws.test#Choice"
        );
    }

    #[test]
    fn recursive_struct_schema_reports_its_direct_cycle() {
        let (resource, _model) = union_fixture();
        let mut model =
            carina_smithy::parse(include_str!("../tests/fixtures/recursive-unions.json"))
                .expect("parse recursive fixture");
        let Some(Shape::Structure(root)) = model.shapes.get_mut("com.amazonaws.test#Fixture")
        else {
            panic!("fixture root")
        };
        root.members.remove("Choice");
        let Some(Shape::Structure(node)) = model.shapes.get_mut("com.amazonaws.test#Node") else {
            panic!("fixture node")
        };
        node.members.remove("Choice");
        let error = generate_resource(&resource, &model)
            .expect_err("self-recursive structs must fail codegen");
        assert_eq!(
            error.to_string(),
            "Recursive Smithy shape cycle: com.amazonaws.test#Node -> com.amazonaws.test#Node"
        );
    }

    #[test]
    fn smithy_union_recursive_structs_generate_docs_without_re_resolving_types() {
        let (resource, _model) = union_fixture();
        let model = carina_smithy::parse(include_str!("../tests/fixtures/recursive-unions.json"))
            .expect("parse recursive union and struct fixture");
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate recursive docs");
        assert_eq!(
            markdown
                .matches("- Exactly one of: `node`, `text`.")
                .count(),
            2,
            "{markdown}"
        );
        assert!(markdown.contains("| `next` | [Struct(Node)]"), "{markdown}");
        assert!(
            markdown.len() < 10_000,
            "recursive schemas must produce finite documentation"
        );
        carina_docgen::check_page(&markdown).expect("recursive links must resolve");
    }

    #[test]
    fn smithy_union_with_enum_members_is_sent_as_a_composite_value() {
        let (resource, mut model) = union_fixture();
        let Some(Shape::Union(union)) = model.shapes.get_mut("com.amazonaws.test#Choice") else {
            panic!("missing fixture union");
        };
        union.members.remove("Details");
        union
            .members
            .get_mut("Flag")
            .expect("fixture member")
            .target = "com.amazonaws.test#Mode".to_string();
        let collected =
            collect_resource_attributes(&resource, &model).expect("collect union types");
        let choice = collected
            .attrs
            .iter()
            .find(|attr| attr.snake_name == "choice")
            .expect("fixture choice attribute");
        assert!(
            choice
                .type_code
                .contains("FieldGroup::exactly_one([\"flag\", \"text\"])")
        );
        assert_eq!(choice.send_type, SendFieldType::Value);
    }

    #[test]
    fn smithy_union_markdown_documents_choice_and_singleton_requiredness() {
        let (resource, model) = union_fixture();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate union docs");
        assert_eq!(
            markdown
                .matches("- Exactly one of: `details`, `flag`, `text`.")
                .count(),
            3,
            "{markdown}"
        );
        assert!(
            markdown.contains("| `text` | `String` | No | A literal value. |"),
            "{markdown}"
        );
        assert!(
            markdown.contains("| `text` | `String` | Yes |"),
            "{markdown}"
        );
        assert!(markdown.contains("### single_choice\n"), "{markdown}");
        assert!(
            markdown.contains("aws.ec2.RouteTable.Choice.Details.Mode.fast"),
            "{markdown}"
        );
        carina_docgen::check_page(&markdown).expect("union docs must pass shared Markdown checks");
    }

    fn lifecycle_markdown_fixture() -> String {
        let model = carina_smithy::parse(
            r##"{
                "smithy": "2.0",
                "shapes": {
                    "com.amazonaws.s3#PutBucketLifecycleConfiguration": {
                        "type": "operation",
                        "input": { "target": "com.amazonaws.s3#LifecycleRequest" }
                    },
                    "com.amazonaws.s3#LifecycleRequest": {
                        "type": "structure",
                        "members": {}
                    }
                }
            }"##,
        )
        .expect("parse lifecycle documentation fixture");
        let resource = resource_defs::s3_resources()
            .into_iter()
            .find(|resource| resource.name == "s3.BucketLifecycleConfiguration")
            .expect("lifecycle resource");
        generate_markdown_resource(&resource, &model, &markdown_resource_example(resource.name))
            .expect("generate lifecycle docs from hand-written type")
    }

    #[test]
    fn markdown_lifecycle_dates_use_module_independent_labels() {
        let markdown = lifecycle_markdown_fixture();
        assert_eq!(
            markdown
                .matches("| `date` | `S3LifecycleDate (pattern: /^(?:[0-9]{4}-(?:(?:01\\|03\\|05\\|07\\|08\\|10\\|12)-(?:0[1-9]\\|[12][0-9]\\|3[01])\\|(?:04\\|06\\|09\\|11)-(?:0[1-9]\\|[12][0-9]\\|30)\\|02-(?:0[1-9]\\|1[0-9]\\|2[0-8]))\\|(?:[0-9]{2}(?:0[48]\\|[2468][048]\\|[13579][26])\\|(?:[02468][048]\\|[13579][26])00)-02-29)T00:00:00Z\\z/ (Full), len: 20..=20, custom validator)` | No |")
                .count(),
            2,
            "{markdown}"
        );
        assert!(!markdown.contains("CarinaAwsTypes"), "{markdown}");
        assert_eq!(
            type_code_to_display("carina_aws_types::s3_lifecycle_date()"),
            type_code_to_display("super::s3_lifecycle_date()")
        );
    }

    #[test]
    fn markdown_struct_field_groups_lifecycle_filter_uses_the_declared_type() {
        let markdown = lifecycle_markdown_fixture();
        let filter = markdown
            .split("### rules[].filter\n")
            .nth(1)
            .expect("the lifecycle filter must have a field table")
            .split("\n### ")
            .next()
            .expect("filter section");
        assert!(filter.contains("- At most one of: `prefix`, `tag`, `object_size_greater_than`, `object_size_less_than`, `and`."), "{filter}");
        assert!(filter.contains("| `prefix` | `String` | No |"), "{filter}");
        assert!(
            filter.contains("| `and` | [Struct(LifecycleRuleAndOperator)]"),
            "{filter}"
        );
        carina_docgen::check_page(&markdown).expect("lifecycle docs must pass shared checks");
    }

    #[test]
    fn markdown_struct_field_groups_are_read_from_types_inside_lists_and_maps() {
        for (group, sentence) in [
            (
                FieldGroup::at_most_one(["first", "second"]).unwrap(),
                "- At most one of: `first`, `second`.",
            ),
            (
                FieldGroup::exactly_one(["first", "second"]).unwrap(),
                "- Exactly one of: `first`, `second`.",
            ),
            (
                FieldGroup::at_least_one(["first", "second"]).unwrap(),
                "- At least one of: `first`, `second`.",
            ),
        ] {
            let attr_type =
                AttributeType::list(AttributeType::map(AttributeType::struct_with_field_groups(
                    "Choice",
                    vec![
                        StructField::optional("first", AttributeType::bool()).build(),
                        StructField::optional("second", AttributeType::bool()).build(),
                    ],
                    vec![group],
                )));
            let mut enums = BTreeMap::new();
            let mut structs = MarkdownTypes::default();
            let ty = discover_markdown_attribute_type(
                &attr_type,
                &HeadingPath::root("choices"),
                "aws.test",
                &mut enums,
                &mut structs,
                None,
            )
            .expect("discover nested type");
            let mut page = PageBuilder::new();
            page.heading(Heading::plain(1, "Declared type")).unwrap();
            ty.write_to(&mut page);
            page.push_str("\n\n");
            write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
            let markdown = page.finish().expect("valid declared-type docs");
            assert_eq!(markdown.matches(sentence).count(), 1, "{markdown}");
            assert!(
                markdown.find(sentence).unwrap() < markdown.find("| Field | Type |").unwrap(),
                "{markdown}"
            );
        }
    }

    #[test]
    fn markdown_declared_types_preserve_scalar_names_and_enum_aliases() {
        let ty = AttributeType::struct_(
            "Target",
            vec![
                StructField::optional("zone", carina_aws_types::route53_hosted_zone_id()).build(),
                StructField::optional(
                    "cloudfront_zone",
                    carina_aws_types::cloudfront_hosted_zone_id(),
                )
                .build(),
            ],
        );
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        discover_markdown_attribute_type(
            &ty,
            &HeadingPath::root("target"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap();
        let mut page = PageBuilder::new();
        page.heading(Heading::plain(1, "Declared types")).unwrap();
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page
            .finish()
            .expect("declared enum aliases produce valid docs");
        assert!(
            markdown.contains(
                "| `zone` | `Route53HostedZoneId (pattern: /^Z[A-Z0-9]+$/ (Full), len: ..=32)` |"
            ),
            "{markdown}"
        );
        assert!(
            markdown.contains("`aws.cloudfront.HostedZoneId.global`"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_data_source_input_and_aggregated_output_can_share_a_name() {
        let mut source = resource_defs::sts_data_sources().remove(0);
        source.inputs = vec![resource_defs::DataSourceInput {
            name: "names",
            provider_name: "Names",
            description: "Names to look up.",
            required: false,
            type_override: Some("super::availability_zone()"),
        }];
        source.shape = resource_defs::DataSourceShape::ListAggregated {
            aggregated_outputs: vec![resource_defs::DataSourceListOutput {
                name: "names",
                description: "Names returned by the lookup.",
                item_type: "super::availability_zone()",
            }],
        };
        let markdown = generate_markdown_data_source(
            &source,
            &empty_test_model(),
            &markdown_data_source_example(source.name),
        )
        .expect("an input and output can share an attribute name");
        assert_eq!(markdown.matches("### `names`").count(), 2, "{markdown}");
        assert!(markdown.contains("Names to look up."), "{markdown}");
        assert!(
            markdown.contains("Names returned by the lookup."),
            "{markdown}"
        );
        assert!(
            markdown.contains(
                "List\\<`ZoneName (custom validator, DSL transform: HyphenToUnderscore)`\\> (unordered)"
            ),
            "{markdown}"
        );
        assert_eq!(
            markdown
                .matches("`ZoneName (custom validator, DSL transform: HyphenToUnderscore)`")
                .count(),
            2,
            "{markdown}"
        );
        let ids = carina_docgen::heading_ids(&markdown);
        assert!(ids.iter().any(|id| id == "names"));
        assert!(ids.iter().any(|id| id == "names-1"));
        carina_docgen::check_page(&markdown).unwrap();
    }

    #[test]
    fn markdown_data_source_list_validation_note_follows_the_list() {
        let mut source = resource_defs::sts_data_sources().remove(0);
        source.name = "acm.Certificate";
        source.inputs.clear();
        source.shape = resource_defs::DataSourceShape::ListAggregated {
            aggregated_outputs: vec![resource_defs::DataSourceListOutput {
                name: "subject_alternative_names",
                description: "Certificate domain names.",
                item_type: "AttributeType::string()",
            }],
        };
        let markdown = generate_markdown_data_source(
            &source,
            &empty_test_model(),
            &markdown_data_source_example(source.name),
        )
        .expect("data source list with a validation note");
        let type_line = markdown
            .lines()
            .find(|line| line.starts_with("- **Type:** "))
            .unwrap();
        assert!(
            type_line.ends_with(&format!(
                "\\> (unordered); {}",
                carina_aws_types::ACM_CERTIFICATE_REQUEST_DOMAIN_RULE
            )),
            "{markdown}"
        );
        assert_eq!(
            type_line
                .matches(carina_aws_types::ACM_CERTIFICATE_REQUEST_DOMAIN_RULE)
                .count(),
            1
        );
        assert!(type_line.contains("custom validator)`"), "{markdown}");
        carina_docgen::check_page(&markdown).unwrap();
    }

    #[test]
    fn markdown_data_source_nested_struct_field_groups_use_the_shared_renderer() {
        let mut source = resource_defs::sts_data_sources().remove(0);
        source.shape = resource_defs::DataSourceShape::Single {
            output_attributes: vec![resource_defs::DataSourceOutput {
                name: "rules",
                provider_name: None,
                description: "Lifecycle rules.",
                type_code: "super::bucket_lifecycle_rules()",
            }],
        };
        let markdown = generate_markdown_data_source(
            &source,
            &empty_test_model(),
            &markdown_data_source_example(source.name),
        )
        .expect("data source struct docs");
        assert!(markdown.contains("### rules[].filter\n\n**Field groups:**\n\n- At most one of: `prefix`, `tag`, `object_size_greater_than`, `object_size_less_than`, `and`.\n\n| Field | Type |"), "{markdown}");
        carina_docgen::check_page(&markdown).expect("valid data source field-group docs");
    }

    #[test]
    fn markdown_data_source_inputs_expose_nested_struct_field_groups() {
        let mut source = resource_defs::sts_data_sources().remove(0);
        source.inputs = vec![resource_defs::DataSourceInput {
            name: "rules",
            provider_name: "Rules",
            description: "Lifecycle rules.",
            required: false,
            type_override: Some("super::bucket_lifecycle_rules()"),
        }];
        let markdown = generate_markdown_data_source(
            &source,
            &empty_test_model(),
            &markdown_data_source_example(source.name),
        )
        .expect("data source input docs");
        assert!(markdown.contains("### rules[].filter\n\n**Field groups:**\n\n- At most one of: `prefix`, `tag`, `object_size_greater_than`, `object_size_less_than`, `and`."), "{markdown}");
        carina_docgen::check_page(&markdown).expect("valid input field-group docs");
    }

    #[test]
    fn smithy_union_zero_members_is_a_schema_and_markdown_codegen_error() {
        let (resource, mut model) = union_fixture();
        let Some(Shape::Union(union)) = model.shapes.get_mut("com.amazonaws.test#Choice") else {
            panic!("missing fixture union");
        };
        union.members.clear();
        let error = generate_resource(&resource, &model)
            .expect_err("empty unions must fail schema codegen");
        assert!(
            error
                .to_string()
                .contains("Smithy union com.amazonaws.test#Choice has no members"),
            "{error:#}"
        );
        let error = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect_err("empty unions must fail Markdown codegen");
        assert!(
            error
                .to_string()
                .contains("Smithy union com.amazonaws.test#Choice has no members"),
            "{error:#}"
        );
    }

    fn s3_bucket_description_fixture() -> (ResourceDef, SmithyModel) {
        let resource = resource_defs::s3_resources()
            .into_iter()
            .find(|resource| resource.name == "s3.Bucket")
            .unwrap();
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.s3#CreateBucket": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.s3#CreateBucketRequest" }
                },
                "com.amazonaws.s3#CreateBucketRequest": {
                  "type": "structure",
                  "members": {
                    "Bucket": {
                      "target": "smithy.api#String",
                      "traits": { "smithy.api#required": {} }
                    },
                    "BucketNamespace": { "target": "smithy.api#String" }
                  }
                }
              }
            }"##,
        )
        .expect("parse minimal S3 bucket model");
        (resource, model)
    }

    #[test]
    fn description_managed_read_only_docs_preserve_authored_placeholders() {
        let (resource, model) = s3_bucket_description_fixture();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate managed docs");
        let attributes = markdown.split("## Attribute Reference").nth(1).unwrap();
        assert!(
            attributes.contains("Bucket domain name (`<bucket>.s3.amazonaws.com`)."),
            "{attributes}"
        );
    }

    #[test]
    fn description_managed_schema_preserves_authored_placeholders() {
        let (resource, model) = s3_bucket_description_fixture();
        let code = generate_resource(&resource, &model).expect("generate managed schema");
        assert!(
            code.contains(".with_description(\"Bucket domain name (`<bucket>.s3.amazonaws.com`). (read-only)\")"),
            "{code}"
        );
    }

    #[test]
    fn description_smithy_placeholders_are_escaped_in_generated_markdown() {
        let mut model = provider_fixture_model();
        let mut resource =
            provider_fixture_resource("iam.Role", "RoleName", &["RoleName", "Options"], vec![]);
        resource.extra_read_only.push("Arn");
        for (shape, member) in [
            ("ReadFixture", None),
            ("CreateFixtureRequest", Some("RoleName")),
            ("ReadFixture", Some("Arn")),
            ("Options", Some("AmazonSideAsn")),
        ] {
            let Some(Shape::Structure(structure)) =
                model.shapes.get_mut(&format!("com.amazonaws.test#{shape}"))
            else {
                panic!("missing fixture structure {shape}");
            };
            let traits = match member {
                Some(member) => &mut structure.members.get_mut(member).unwrap().traits,
                None => &mut structure.traits,
            };
            traits.insert(
                "smithy.api#documentation".to_string(),
                "<p>&lt;bucket&gt; &amp; *literal*</p>".into(),
            );
        }
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate docs");
        assert_eq!(
            markdown.matches("&lt;bucket&gt; &amp; *literal*").count(),
            4,
            "{markdown}"
        );
    }

    #[test]
    fn description_truncation_preserves_escape_sequences_at_all_emitter_boundaries() {
        for (last_character, escaped) in [('\\', r"\\"), ('"', r#"\""#)] {
            for tail in ["", "tail"] {
                let mut model = provider_fixture_model();
                let resource = provider_fixture_resource(
                    "iam.Role",
                    "RoleName",
                    &["RoleName", "Options"],
                    vec![],
                );
                let mut expected = Vec::new();
                for (shape, member, prefix_character, limit) in [
                    ("ReadFixture", None, "r", 200),
                    ("CreateFixtureRequest", Some("RoleName"), "a", 150),
                    ("Options", Some("AmazonSideAsn"), "f", 150),
                ] {
                    let Some(Shape::Structure(structure)) =
                        model.shapes.get_mut(&format!("com.amazonaws.test#{shape}"))
                    else {
                        panic!("missing fixture structure {shape}");
                    };
                    let traits = match member {
                        Some(member) => &mut structure.members.get_mut(member).unwrap().traits,
                        None => &mut structure.traits,
                    };
                    let prefix = prefix_character.repeat(limit - 1);
                    traits.insert(
                        "smithy.api#documentation".to_string(),
                        format!("{prefix}{last_character}{tail}").into(),
                    );
                    let ellipsis = if tail.is_empty() { "" } else { "..." };
                    expected.push(format!(
                        ".with_description(\"{prefix}{escaped}{ellipsis}\")"
                    ));
                }
                let code = generate_resource(&resource, &model).expect("generate schema");
                for description in expected {
                    assert!(code.contains(&description), "missing {description}: {code}");
                }
            }
        }
    }

    #[test]
    fn description_smithy_html_decodes_entities_after_removing_tags() {
        assert_eq!(
            Description::from_smithy_html(
                "<p>Use <code>&lt;bucket&gt;</code> &amp; &quot;name&quot; &#39;value&#39;.</p>"
            )
            .rust_string_literal(None),
            r#"Use <bucket> & \"name\" 'value'."#
        );
    }

    #[test]
    fn description_smithy_unbalanced_backtick_does_not_disable_tag_removal() {
        assert_eq!(
            Description::from_smithy_html(
                "<p>Unbalanced `backtick.</p><p>Keep <b>stripping</b> <code>tags</code>.</p>"
            )
            .rust_string_literal(None),
            "Unbalanced `backtick. Keep stripping tags."
        );
    }

    #[test]
    fn description_s3_managed_and_data_source_outputs_use_identical_wording() {
        let (resource, model) = s3_bucket_description_fixture();
        let ds = resource_defs::s3_data_sources().pop().unwrap();
        let resource_defs::DataSourceShape::Single { output_attributes } = &ds.shape else {
            panic!("S3 bucket must be a single data source");
        };
        let markdown =
            generate_markdown_data_source(&ds, &model, &markdown_data_source_example(ds.name))
                .expect("generate data source docs");
        let schema = generate_data_source(&ds, &model, true).expect("generate data source schema");
        for attr in &resource.extra_attributes {
            let output = output_attributes
                .iter()
                .find(|output| output.name == attr.name.to_snake_case())
                .expect("every managed bucket output has a data-source counterpart");
            assert_eq!(attr.description, Some(output.description), "{}", attr.name);
            assert!(markdown.contains(output.description), "{markdown}");
            assert!(schema.contains(output.description), "{schema}");
        }
    }

    #[test]
    fn description_data_source_outputs_preserve_authored_text_in_both_shapes() {
        const AUTHORED: &str =
            "Use `<bucket>.s3.amazonaws.com`, &lt;literal&gt;, \"quotes\" and \\paths.\n Next.";
        for mut ds in [
            resource_defs::s3_data_sources().pop().unwrap(),
            resource_defs::iam_data_sources().pop().unwrap(),
        ] {
            match &mut ds.shape {
                resource_defs::DataSourceShape::Single { output_attributes } => {
                    for output in output_attributes {
                        output.description = AUTHORED;
                    }
                }
                resource_defs::DataSourceShape::ListAggregated { aggregated_outputs } => {
                    for output in aggregated_outputs {
                        output.description = AUTHORED;
                    }
                }
            }
            let description = Description::authored(AUTHORED);
            let schema = generate_data_source(&ds, &empty_test_model(), true).unwrap();
            assert!(
                schema.contains(&description.rust_string_literal(None)),
                "{schema}"
            );
            let markdown = generate_markdown_data_source(
                &ds,
                &empty_test_model(),
                &markdown_data_source_example(ds.name),
            )
            .unwrap();
            assert!(
                markdown.contains(description.markdown().as_ref()),
                "{markdown}"
            );
        }
    }

    #[test]
    fn generation_rejects_unclassified_kms_candidate_before_type_overrides() {
        let model = string_fixture_model(&[("KmsNewKey", false)], &[]);
        let mut resource =
            provider_fixture_resource("test.Resource", "Name", &["KmsNewKey"], vec![]);
        resource
            .type_overrides
            .push(("KmsNewKey", "AttributeType::string()"));
        let error = generate_resource(&resource, &model).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unclassified KMS string test.Resource.kms_new_key"),
            "{error:#}"
        );
    }

    fn string_fixture_model(
        create_fields: &[(&str, bool)],
        read_fields: &[(&str, bool)],
    ) -> SmithyModel {
        fn members(fields: &[(&str, bool)]) -> String {
            fields
                .iter()
                .map(|(name, required)| {
                    let traits = if *required {
                        r#"{"smithy.api#required":{}}"#
                    } else {
                        "{}"
                    };
                    format!(r#""{name}":{{"target":"smithy.api#String","traits":{traits}}}"#)
                })
                .collect::<Vec<_>>()
                .join(",")
        }

        let document = r##"{
          "smithy": "2.0",
          "shapes": {
            "com.amazonaws.test#CreateFixture": {
              "type": "operation",
              "input": { "target": "com.amazonaws.test#CreateFixtureRequest" },
              "output": { "target": "smithy.api#Unit" },
              "traits": {}
            },
            "com.amazonaws.test#CreateFixtureRequest": {
              "type": "structure",
              "members": { __CREATE_FIELDS__ },
              "traits": { "smithy.api#input": {} }
            },
            "com.amazonaws.test#ReadFixture": {
              "type": "structure",
              "members": { __READ_FIELDS__ },
              "traits": {}
            }
          }
        }"##
        .replace("__CREATE_FIELDS__", &members(create_fields))
        .replace("__READ_FIELDS__", &members(read_fields));
        carina_smithy::parse(&document).expect("parse string-only fixture model")
    }

    fn nested_write_only_fixture_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.test#CreateFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.test#CreateFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.test#CreateFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "Options": { "target": "com.amazonaws.test#OuterOptions", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.test#ReadFixture": {
                  "type": "structure",
                  "members": {
                    "Options": { "target": "com.amazonaws.test#OuterOptions", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#OuterOptions": {
                  "type": "structure",
                  "members": {
                    "Nested": { "target": "com.amazonaws.test#InnerOptions", "traits": {} },
                    "ValidationMethod": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#InnerOptions": {
                  "type": "structure",
                  "members": {
                    "ValidationMethod": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse nested write-only fixture model")
    }

    fn organizations_account_fixture_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.organizations#CreateAccount": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.organizations#CreateAccountRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.organizations#CreateAccountRequest": {
                  "type": "structure",
                  "members": {
                    "AccountName": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "Email": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "IamUserAccessToBilling": { "target": "smithy.api#String", "traits": {} },
                    "RoleName": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.organizations#Account": {
                  "type": "structure",
                  "members": {
                    "Id": { "target": "smithy.api#String", "traits": {} },
                    "Arn": { "target": "smithy.api#String", "traits": {} },
                    "Email": { "target": "smithy.api#String", "traits": {} },
                    "Name": { "target": "smithy.api#String", "traits": {} },
                    "Status": { "target": "smithy.api#String", "traits": {} },
                    "JoinedMethod": { "target": "smithy.api#String", "traits": {} },
                    "JoinedTimestamp": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse inline Organizations account model")
    }

    fn account_update_fixture_model(input_members: Option<&str>) -> SmithyModel {
        let shapes = input_members.map_or_else(
            || "{}".to_string(),
            |input_members| {
                format!(
                    r##"{{
                      "com.amazonaws.account#PutAccountName": {{
                        "type": "operation",
                        "input": {{ "target": "com.amazonaws.account#PutAccountNameRequest" }},
                        "output": {{ "target": "smithy.api#Unit" }},
                        "traits": {{}}
                      }},
                      "com.amazonaws.account#PutAccountNameRequest": {{
                        "type": "structure",
                        "members": {{ {input_members} }},
                        "traits": {{ "smithy.api#input": {{}} }}
                      }}
                    }}"##
                )
            },
        );
        carina_smithy::parse(&format!(
            r##"{{
              "smithy": "2.0",
              "shapes": {shapes}
            }}"##
        ))
        .expect("parse inline Account Management update model")
    }

    fn organizations_account_resource_definition() -> ResourceDef {
        resource_defs::organizations_resources()
            .into_iter()
            .find(|resource| resource.name == "organizations.Account")
            .expect("Organizations account resource definition")
    }

    fn organizations_account_models(
        account_model: SmithyModel,
    ) -> HashMap<&'static str, SmithyModel> {
        HashMap::from([
            (
                "com.amazonaws.organizations",
                organizations_account_fixture_model(),
            ),
            ("com.amazonaws.account", account_model),
        ])
    }

    #[test]
    fn organizations_account_codegen_keeps_read_contract_while_allowing_name_updates() {
        let resource = organizations_account_resource_definition();
        let account_model = account_update_fixture_model(Some(
            r#""AccountName": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } }"#,
        ));
        validate_cross_service_update_operations(
            &resource,
            &organizations_account_models(account_model),
        )
        .expect("validate Organizations account cross-service update operation");
        let generated =
            collect_resource_attributes(&resource, &organizations_account_fixture_model())
                .expect("generate Organizations account attributes from inline Smithy");

        let account_name = generated
            .attrs
            .iter()
            .find(|attribute| attribute.snake_name == "account_name")
            .expect("account_name attribute");
        assert_eq!(account_name.provider_name, "AccountName");
        assert!(account_name.is_required);
        assert!(
            !account_name.is_create_only,
            "account_name must be updated in place"
        );

        let email = generated
            .attrs
            .iter()
            .find(|attribute| attribute.snake_name == "email")
            .expect("email attribute");
        assert!(email.is_create_only, "email still requires replacement");

        for name in ["role_name", "iam_user_access_to_billing"] {
            let attribute = generated
                .attrs
                .iter()
                .find(|attribute| attribute.snake_name == name)
                .unwrap_or_else(|| panic!("{name} attribute"));
            assert!(attribute.is_create_only, "{name} remains create-only");
            assert!(attribute.is_write_only, "{name} remains write-only");
        }
    }

    #[test]
    fn cross_service_update_rejects_missing_operation() {
        let resource = organizations_account_resource_definition();
        let error = validate_cross_service_update_operations(
            &resource,
            &organizations_account_models(account_update_fixture_model(None)),
        )
        .expect_err("missing cross-service update operation must fail codegen");

        assert!(
            error
                .to_string()
                .contains("com.amazonaws.account#PutAccountName"),
            "{error:#}"
        );
    }

    #[test]
    fn cross_service_update_rejects_missing_field() {
        let resource = organizations_account_resource_definition();
        let account_model = account_update_fixture_model(Some(
            r#""AccountId": { "target": "smithy.api#String", "traits": {} }"#,
        ));
        let error = validate_cross_service_update_operations(
            &resource,
            &organizations_account_models(account_model),
        )
        .expect_err("missing declared cross-service update field must fail codegen");

        let message = error.to_string();
        assert!(message.contains("PutAccountName"), "{error:#}");
        assert!(message.contains("AccountName"), "{error:#}");
    }

    /// Minimal Smithy surface shared by codegen unit tests. Every shape here
    /// is exercised by at least one test below; no ignored AWS model cache is
    /// needed for the codegen contract to be tested in CI.
    fn provider_fixture_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.test#CreateFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.test#CreateFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.test#CreateFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "AllocationId": { "target": "smithy.api#String", "traits": {} },
                    "AvailabilityMode": { "target": "smithy.api#String", "traits": {} },
                    "CidrBlock": { "target": "smithy.api#String", "traits": {} },
                    "Description": { "target": "smithy.api#String", "traits": {} },
                    "Name": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "Options": { "target": "com.amazonaws.test#Options", "traits": {} },
                    "Path": { "target": "smithy.api#String", "traits": {} },
                    "PeerOwnerId": { "target": "smithy.api#String", "traits": {} },
                    "PeerVpcId": { "target": "smithy.api#String", "traits": {} },
                    "PrivateDnsNameOptionsOnLaunch": { "target": "com.amazonaws.test#PrivateDnsOptions", "traits": {} },
                    "RoleName": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "RouteTableIds": { "target": "com.amazonaws.test#StringList", "traits": {} },
                    "SecurityGroupIds": { "target": "com.amazonaws.test#StringList", "traits": {} },
                    "SubnetIds": { "target": "com.amazonaws.test#StringList", "traits": {} },
                    "TransitGatewayId": { "target": "smithy.api#String", "traits": {} },
                    "Unobservable": { "target": "smithy.api#String", "traits": {} },
                    "VpcId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.test#ReadFixture": {
                  "type": "structure",
                  "members": {
                    "AccepterVpcInfo": { "target": "com.amazonaws.test#VpcInfo", "traits": {} },
                    "Arn": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "Attachments": { "target": "com.amazonaws.test#VpcInfoList", "traits": {} },
                    "AvailabilityMode": { "target": "smithy.api#String", "traits": {} },
                    "CidrBlock": { "target": "smithy.api#String", "traits": {} },
                    "Description": { "target": "smithy.api#String", "traits": {} },
                    "EgressOnlyInternetGatewayId": { "target": "smithy.api#String", "traits": {} },
                    "Groups": { "target": "com.amazonaws.test#GroupList", "traits": {} },
                    "Name": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "NatGatewayAddresses": { "target": "com.amazonaws.test#NatGatewayAddressList", "traits": {} },
                    "NatGatewayId": { "target": "smithy.api#String", "traits": {} },
                    "Options": { "target": "com.amazonaws.test#Options", "traits": {} },
                    "Path": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "PrivateDnsNameOptionsOnLaunch": { "target": "com.amazonaws.test#PrivateDnsOptions", "traits": {} },
                    "RequesterVpcInfo": { "target": "com.amazonaws.test#VpcInfo", "traits": {} },
                    "RoleId": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "RoleName": { "target": "smithy.api#String", "traits": { "smithy.api#required": {} } },
                    "RouteTableIds": { "target": "com.amazonaws.test#StringList", "traits": {} },
                    "SubnetId": { "target": "smithy.api#String", "traits": {} },
                    "SubnetIds": { "target": "com.amazonaws.test#StringList", "traits": {} },
                    "TransitGatewayId": { "target": "smithy.api#String", "traits": {} },
                    "VpcEndpointId": { "target": "smithy.api#String", "traits": {} },
                    "VpcId": { "target": "smithy.api#String", "traits": {} },
                    "VpcPeeringConnectionId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#StringList": {
                  "type": "list",
                  "member": { "target": "smithy.api#String", "traits": {} },
                  "traits": {}
                },
                "com.amazonaws.test#VpcInfoList": {
                  "type": "list",
                  "member": { "target": "com.amazonaws.test#VpcInfo", "traits": {} },
                  "traits": {}
                },
                "com.amazonaws.test#VpcInfo": {
                  "type": "structure",
                  "members": {
                    "OwnerId": { "target": "smithy.api#String", "traits": {} },
                    "VpcId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#NatGatewayAddressList": {
                  "type": "list",
                  "member": { "target": "com.amazonaws.test#NatGatewayAddress", "traits": {} },
                  "traits": {}
                },
                "com.amazonaws.test#NatGatewayAddress": {
                  "type": "structure",
                  "members": {
                    "AllocationId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#GroupList": {
                  "type": "list",
                  "member": { "target": "com.amazonaws.test#Group", "traits": {} },
                  "traits": {}
                },
                "com.amazonaws.test#Group": {
                  "type": "structure",
                  "members": {
                    "GroupId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#Options": {
                  "type": "structure",
                  "members": {
                    "AmazonSideAsn": { "target": "smithy.api#Long", "traits": {} },
                    "DnsSupport": { "target": "com.amazonaws.test#FeatureState", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#PrivateDnsOptions": {
                  "type": "structure",
                  "members": {
                    "EnableResourceNameDnsARecord": { "target": "smithy.api#Boolean", "traits": {} },
                    "HostnameType": { "target": "com.amazonaws.test#HostnameType", "traits": {} }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#FeatureState": {
                  "type": "enum",
                  "members": {
                    "enable": { "target": "smithy.api#Unit", "traits": { "smithy.api#enumValue": "enable" } },
                    "disable": { "target": "smithy.api#Unit", "traits": { "smithy.api#enumValue": "disable" } }
                  },
                  "traits": {}
                },
                "com.amazonaws.test#HostnameType": {
                  "type": "enum",
                  "members": {
                    "ip-name": { "target": "smithy.api#Unit", "traits": { "smithy.api#enumValue": "ip-name" } }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse provider codegen fixture model")
    }

    fn provider_fixture_model_without_read_fields(fields: &[&str]) -> SmithyModel {
        let mut model = provider_fixture_model();
        let read_shape = model
            .shapes
            .get_mut("com.amazonaws.test#ReadFixture")
            .expect("provider fixture read shape");
        let carina_smithy::Shape::Structure(read_structure) = read_shape else {
            panic!("provider fixture read shape must be a structure");
        };
        for field in fields {
            read_structure.members.remove(*field);
        }
        model
    }

    fn provider_fixture_resource(
        name: &'static str,
        identifier: &'static str,
        fields: &[&'static str],
        derived_attributes: Vec<resource_defs::DerivedAttribute>,
    ) -> ResourceDef {
        ResourceDef {
            input_rules: vec![],
            read_back: resource_defs::ReadBackOverrides::default(),
            name,
            replacement: ReplacementClass::Conflicting,
            service_namespace: "com.amazonaws.test",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "CreateFixture",
            read_structure: Some("ReadFixture"),
            read_ops: vec![],
            delete_op: "",
            update_ops: vec![],
            identifier,
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: PROVIDER_FIXTURE_FIELDS
                .iter()
                .copied()
                .filter(|field| !fields.contains(field))
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
            nested_update_field_enums: &[],
            read_shape_overrides: vec![],
            derived_attributes,
        }
    }

    fn enum_fixture_model() -> SmithyModel {
        carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.test#CreateEnumFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.test#EnumFixture" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.test#EnumFixture": {
                  "type": "structure",
                  "members": {
                    "Type": { "target": "com.amazonaws.test#RecordType", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.test#RecordType": {
                  "type": "enum",
                  "members": {
                    "A": { "target": "smithy.api#Unit", "traits": { "smithy.api#enumValue": "A" } },
                    "CNAME": { "target": "smithy.api#Unit", "traits": { "smithy.api#enumValue": "CNAME" } }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse enum codegen fixture model")
    }

    fn enum_fixture_resource() -> ResourceDef {
        ResourceDef {
            input_rules: vec![],
            read_back: resource_defs::ReadBackOverrides::default(),
            // Use a registered resource name because generate_resource also
            // emits the provider's closed CloudFormation type mapping.
            name: "iam.Role",
            replacement: ReplacementClass::Conflicting,
            service_namespace: "com.amazonaws.test",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "CreateEnumFixture",
            read_structure: Some("EnumFixture"),
            read_ops: vec![],
            delete_op: "",
            update_ops: vec![],
            identifier: "",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![],
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
            nested_update_field_enums: &[],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        }
    }

    #[test]
    fn mutable_identity_fails_attribute_collection() {
        for required in [false, true] {
            for synthetic in [false, true] {
                let model = if synthetic {
                    string_fixture_model(&[], &[])
                } else {
                    string_fixture_model(&[("Description", required)], &[("Description", required)])
                };
                let mut resource =
                    provider_fixture_resource("test.MutableIdentity", "", &["Description"], vec![]);
                resource.identity_overrides = vec!["Description"];
                resource.update_ops.push(resource_defs::UpdateOp {
                    operation: "UpdateFixture",
                    fields: resource_defs::FieldLayout::Flat(vec!["Description"]),
                });
                if synthetic {
                    resource.extra_attributes.push(resource_defs::ExtraField {
                        name: "Description",
                        read_source: None,
                        description: None,
                    });
                    if required {
                        resource.required_overrides.push("Description");
                    }
                }

                let error = collect_resource_attributes_unvalidated(&resource, &model, &[])
                    .err()
                    .expect("mutable identity must fail before rendering");
                assert_eq!(
                    error.to_string(),
                    "test.MutableIdentity identity attribute Description must be create-only",
                    "required={required}, synthetic={synthetic}"
                );
            }
        }
    }

    #[test]
    fn read_only_identity_fails_attribute_collection() {
        let model = string_fixture_model(&[], &[("Description", false)]);
        let mut resource =
            provider_fixture_resource("test.ReadOnlyIdentity", "", &["Description"], vec![]);
        resource.extra_read_only = vec!["Description"];
        resource.identity_overrides = vec!["Description"];

        let error = collect_resource_attributes_unvalidated(&resource, &model, &[])
            .err()
            .expect("read-only identity must fail before rendering");
        assert_eq!(
            error.to_string(),
            "test.ReadOnlyIdentity identity attribute Description must be create-only"
        );
    }

    #[test]
    fn identity_emission_follows_create_only_for_required_and_optional_attributes() {
        for required in [false, true] {
            let model =
                string_fixture_model(&[("Description", required)], &[("Description", required)]);
            let mut resource = provider_fixture_resource("iam.Role", "", &["Description"], vec![]);
            resource.identity_overrides = vec!["Description"];

            let generated =
                generate_resource(&resource, &model).expect("generate create-only identity");
            let constructor = if required { "required" } else { "optional" };
            assert!(
                generated.contains(&format!(
                    "AttributeSchema::{constructor}(\"description\", AttributeType::string())\n                .create_only()\n                .identity()"
                )),
                "{generated}"
            );
        }
    }

    #[test]
    fn identity_overrides_match_lower_camel_case_smithy_members() {
        let model = string_fixture_model(&[("logGroupName", true)], &[("logGroupName", true)]);
        let mut resource = provider_fixture_resource("iam.Role", "", &["logGroupName"], vec![]);
        resource.identity_overrides = vec!["LogGroupName"];

        let generated =
            generate_resource(&resource, &model).expect("generate lower-camel identity");
        assert!(
            generated.contains(
                "AttributeSchema::required(\"log_group_name\", AttributeType::string())\n                .create_only()\n                .identity()"
            ),
            "{generated}"
        );
    }

    #[test]
    fn unclassified_read_attribute_fails_generation() {
        let model = string_fixture_model(
            &[("Description", false), ("Unobservable", false)],
            &[("Description", false)],
        );
        let resource =
            provider_fixture_resource("test.UnclassifiedFixture", "", &["Unobservable"], vec![]);

        let error = validate_read_contract(&resource, &model, &[])
            .expect_err("a schema attribute without a read disposition must fail generation");
        assert!(error.to_string().contains("Unobservable"), "{error:#}");
    }

    #[test]
    fn write_only_read_disposition_requires_create_only_attribute() {
        let model = string_fixture_model(&[("Description", false)], &[("Description", false)]);
        let mut resource =
            provider_fixture_resource("test.UpdatableFixture", "", &["Description"], vec![]);
        resource.update_ops.push(resource_defs::UpdateOp {
            operation: "UpdateFixture",
            fields: resource_defs::FieldLayout::Flat(vec!["Description"]),
        });
        let dispositions = [resource_defs::ReadDisposition::WriteOnly {
            field: "Description",
            reason: "test fixture",
        }];

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("write-only must not hide an updatable attribute");
        assert!(error.to_string().contains("Description"), "{error:#}");
        assert!(error.to_string().contains("create-only"), "{error:#}");
    }

    #[test]
    fn schema_omission_requires_a_non_empty_reason() {
        let model = string_fixture_model(&[("Unobservable", false)], &[]);
        let resource =
            provider_fixture_resource("test.OmittedFixture", "", &["Unobservable"], vec![]);
        let dispositions = [resource_defs::ReadDisposition::OmitSchema {
            field: "Unobservable",
            reason: "",
        }];

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("omitting a schema attribute without a reason must fail generation");
        assert!(error.to_string().contains("Unobservable"), "{error:#}");
        assert!(error.to_string().contains("reason"), "{error:#}");
    }

    #[test]
    fn nested_write_only_marks_only_the_resolved_parent_member() {
        let model = nested_write_only_fixture_model();
        let resource = provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);
        let dispositions = [resource_defs::ReadDisposition::NestedWriteOnly {
            field: "Options",
            member: "ValidationMethod",
            reason: "test fixture",
        }];

        let collected =
            collect_resource_attributes_with_read_dispositions(&resource, &model, &dispositions)
                .expect("generate nested write-only fixture");
        let options_type = &collected
            .attrs
            .iter()
            .find(|attr| attr.provider_name == "Options")
            .expect("Options attribute")
            .type_code;

        assert_eq!(options_type.matches(".write_only()").count(), 1);
        assert!(
            options_type.contains(
                "StructField::optional(\"validation_method\", AttributeType::string()).write_only().with_provider_name(\"ValidationMethod\")"
            ),
            "outer member was not marked write-only:\n{options_type}"
        );
        assert!(
            options_type.contains(
                "StructField::optional(\"validation_method\", AttributeType::string()).with_provider_name(\"ValidationMethod\")"
            ),
            "same-named member in the inner struct was incorrectly marked write-only:\n{options_type}"
        );
    }

    #[test]
    fn nested_write_only_requires_exact_smithy_spelling() {
        let model = nested_write_only_fixture_model();
        let resource = provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);

        for (field, member) in [
            ("options", "ValidationMethod"),
            ("Options", "validation_method"),
        ] {
            let dispositions = [resource_defs::ReadDisposition::NestedWriteOnly {
                field,
                member,
                reason: "test fixture",
            }];
            let error = validate_read_contract(&resource, &model, &dispositions)
                .expect_err("normalized nested write-only spelling must fail generation");
            assert!(
                error.to_string().contains("Smithy member spelling"),
                "{error:#}"
            );
        }
    }

    #[test]
    fn nested_write_only_fails_when_a_type_override_bypasses_struct_emission() {
        let model = nested_write_only_fixture_model();
        let mut resource =
            provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);
        resource
            .type_overrides
            .push(("Options", "AttributeType::string()"));
        let dispositions = [resource_defs::ReadDisposition::NestedWriteOnly {
            field: "Options",
            member: "ValidationMethod",
            reason: "test fixture",
        }];

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("type override must not silently drop nested write-only emission");
        let message = error.to_string();
        assert!(message.contains("Options.ValidationMethod"), "{error:#}");
        assert!(message.contains("generated 0"), "{error:#}");
        assert!(message.contains("expected exactly one"), "{error:#}");
    }

    #[test]
    fn nested_write_only_fails_when_a_parent_shape_is_emitted_twice() {
        let mut model = nested_write_only_fixture_model();
        for shape_id in [
            "com.amazonaws.test#CreateFixtureRequest",
            "com.amazonaws.test#ReadFixture",
        ] {
            let shape = model.shapes.get_mut(shape_id).expect("fixture shape");
            let carina_smithy::Shape::Structure(structure) = shape else {
                panic!("fixture shape must be a structure");
            };
            structure.members.insert(
                "OtherOptions".to_string(),
                carina_smithy::ShapeRef {
                    target: "com.amazonaws.test#OuterOptions".to_string(),
                    traits: Default::default(),
                },
            );
        }
        let resource = provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);
        let dispositions = [resource_defs::ReadDisposition::NestedWriteOnly {
            field: "Options",
            member: "ValidationMethod",
            reason: "test fixture",
        }];

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("reused parent shape must not emit nested write-only markers twice");
        let message = error.to_string();
        assert!(message.contains("generated 2"), "{error:#}");
        assert!(message.contains("expected exactly one"), "{error:#}");
    }

    #[test]
    fn nested_write_only_rejects_conflicting_top_level_dispositions() {
        let model = nested_write_only_fixture_model();
        let resource = provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);
        let conflicts = [
            resource_defs::ReadDisposition::WriteOnly {
                field: "Options",
                reason: "test fixture",
            },
            resource_defs::ReadDisposition::OmitSchema {
                field: "Options",
                reason: "test fixture",
            },
        ];

        for conflict in conflicts {
            let dispositions = [
                resource_defs::ReadDisposition::NestedWriteOnly {
                    field: "Options",
                    member: "ValidationMethod",
                    reason: "test fixture",
                },
                conflict,
            ];
            let error = validate_read_contract(&resource, &model, &dispositions)
                .expect_err("conflicting top-level disposition must fail generation");
            assert!(
                error.to_string().contains("conflicts with top-level"),
                "{error:#}"
            );
        }
    }

    #[test]
    fn nested_write_only_requires_a_readable_parent_attribute() {
        let mut model = nested_write_only_fixture_model();
        let read_shape = model
            .shapes
            .get_mut("com.amazonaws.test#ReadFixture")
            .expect("read fixture shape");
        let carina_smithy::Shape::Structure(read_structure) = read_shape else {
            panic!("read fixture must be a structure");
        };
        read_structure.members.remove("Options");
        let resource = provider_fixture_resource("test.NestedWriteOnly", "", &["Options"], vec![]);
        let dispositions = [resource_defs::ReadDisposition::NestedWriteOnly {
            field: "Options",
            member: "ValidationMethod",
            reason: "test fixture",
        }];

        let error = validate_read_contract(&resource, &model, &dispositions)
            .expect_err("nested write-only parent must remain readable");
        assert!(
            error
                .to_string()
                .contains("parent attribute to be readable"),
            "{error:#}"
        );
    }

    #[test]
    fn unclassified_writable_attribute_fails_send_generation() {
        let model = string_fixture_model(&[("Description", false)], &[("Description", false)]);
        let resource =
            provider_fixture_resource("test.UnclassifiedSendFixture", "", &["Description"], vec![]);

        let error = validate_send_contract(&resource, &model, &[]).expect_err(
            "a writable schema attribute without a send disposition must fail generation",
        );
        assert!(error.to_string().contains("Description"), "{error:#}");
        assert!(error.to_string().contains("send"), "{error:#}");
    }

    #[test]
    fn not_sent_disposition_requires_a_non_empty_reason() {
        let model = string_fixture_model(&[("Description", false)], &[("Description", false)]);
        let resource =
            provider_fixture_resource("test.NotSentReasonFixture", "", &["Description"], vec![]);
        let dispositions = [resource_defs::SendDisposition::NotSent {
            field: "Description",
            reason: "",
        }];

        let error = validate_send_contract(&resource, &model, &dispositions)
            .expect_err("NotSent without a reason must fail generation");
        assert!(error.to_string().contains("Description"), "{error:#}");
        assert!(error.to_string().contains("reason"), "{error:#}");
    }

    #[test]
    fn synthetic_tags_not_sent_reason_is_validated_and_retained() {
        let model = string_fixture_model(&[("Description", false)], &[("Description", false)]);
        let mut resource =
            provider_fixture_resource("test.TagsSendFixture", "", &["Description"], vec![]);
        resource.has_tags = true;
        let dispositions = [
            resource_defs::SendDisposition::sent("Description"),
            resource_defs::SendDisposition::NotSent {
                field: "Tags",
                reason: "tags are resolved by the fixture's external tag owner",
            },
        ];
        let read_dispositions = resource.read_dispositions();
        let mut collected =
            collect_resource_attributes_unvalidated(&resource, &model, &read_dispositions)
                .expect("fixture attributes");
        let applied = apply_send_contract(&resource, &mut collected.attrs, &dispositions)
            .expect("classified synthetic tags");
        assert_eq!(
            applied.tags_not_sent_reason.as_deref(),
            Some("tags are resolved by the fixture's external tag owner")
        );

        let empty_reason = [
            resource_defs::SendDisposition::sent("Description"),
            resource_defs::SendDisposition::NotSent {
                field: "Tags",
                reason: "",
            },
        ];
        let error = validate_send_contract(&resource, &model, &empty_reason)
            .expect_err("synthetic tags still require a NotSent reason");
        assert!(error.to_string().contains("Tags"), "{error:#}");
        assert!(error.to_string().contains("reason"), "{error:#}");
    }

    #[test]
    fn generated_send_inputs_exclude_not_sent_and_create_only_from_update() {
        let model = string_fixture_model(
            &[("Description", false), ("Name", true), ("Path", false)],
            &[("Description", false), ("Name", true), ("Path", false)],
        );
        let mut resource = provider_fixture_resource(
            "test.TypedSendFixture",
            "",
            &["Description", "Name", "Path"],
            vec![],
        );
        resource.update_ops.push(resource_defs::UpdateOp {
            operation: "UpdateFixture",
            fields: resource_defs::FieldLayout::Flat(vec!["Description"]),
        });
        let dispositions = [
            resource_defs::SendDisposition::follow_up_mapped("description", &["WireDescription"]),
            resource_defs::SendDisposition::sent("Name"),
            resource_defs::SendDisposition::NotSent {
                field: "Path",
                reason: "the fixture resolves this path from its provider identifier",
            },
        ];

        let code = generate_send_models_code(
            &[(&resource, dispositions.as_slice())],
            &HashMap::from([(resource.service_namespace, model)]),
        )
        .expect("generate typed send inputs");

        let create = code
            .split("pub(crate) struct TestTypedSendFixtureCreateInput")
            .nth(1)
            .expect("create input")
            .split("pub(crate) struct TestTypedSendFixtureUpdateInput")
            .next()
            .expect("create input body");
        assert!(create.contains("description"), "{create}");
        assert!(create.contains("name"), "{create}");
        assert!(!create.contains("path"), "{create}");
        let create_sent = create
            .split("pub(crate) fn sent(&self)")
            .nth(1)
            .expect("create sent method")
            .split("pub(crate) fn into_parts")
            .next()
            .expect("create sent body");
        assert!(
            !create_sent.contains("sent.insert(\"description\"")
                && create_sent.contains("sent.insert(\"name\""),
            "follow-up fields must stay out of the protocol create-request Sent set: {create_sent}"
        );

        let update = code
            .split("pub(crate) struct TestTypedSendFixtureUpdateInput")
            .nth(1)
            .expect("update input")
            .split("#[cfg(test)]")
            .next()
            .expect("update input body");
        assert!(update.contains("description"), "{update}");
        assert!(!update.contains("name"), "{update}");
        assert!(!update.contains("path"), "{update}");
        assert!(
            code.contains("pub(crate) struct TestTypedSendFixtureCreateParts {")
                && code.contains("pub(crate) description: crate::send_model::Sent<Option<String>>"),
            "generated parts must be named and guarded: {code}"
        );
        assert!(
            code.contains("capture_names: &[\"WireDescription\"]"),
            "capture lookup must use the same snake_case normalization as classification: {code}"
        );
    }

    #[test]
    fn nested_update_field_set_tracks_new_model_members() {
        let model = carina_smithy::parse(
            r##"{
            "smithy":"2.0", "shapes": {
                "test#Dns": { "type":"structure", "members": {
                    "HostnameType": {"target":"smithy.api#String"},
                    "FutureOption": {"target":"smithy.api#Boolean"}
                } }
            }
        }"##,
        )
        .unwrap();
        let mut code = String::new();
        emit_update_field_enum(&mut code, &model, "test#Dns", "DnsField").unwrap();
        assert!(code.contains("    FutureOption,"), "{code}");
        assert!(code.contains("Self::FutureOption,"), "{code}");
        assert!(
            code.contains("Self::FutureOption => \"future_option\""),
            "{code}"
        );
        assert!(!code.contains("_ =>"), "{code}");
        let error =
            emit_update_field_enum(&mut String::new(), &model, "test#Absent", "Field").unwrap_err();
        assert!(error.to_string().contains("test#Absent"), "{error}");
    }

    #[test]
    fn generated_update_fields_require_a_removal_decision() {
        let model = string_fixture_model(
            &[("Description", false), ("Name", true)],
            &[("Description", false), ("Name", true)],
        );
        let mut resource =
            provider_fixture_resource("test.RemovalFixture", "", &["Description", "Name"], vec![]);
        resource.update_ops.push(resource_defs::UpdateOp {
            operation: "UpdateFixture",
            fields: resource_defs::FieldLayout::Flat(vec!["Description", "Name"]),
        });
        resource.nested_update_field_enums = &[resource_defs::NestedUpdateFieldEnum {
            structure: "ReadFixture",
            rust_name: "DeclaredFixtureField",
        }];
        let dispositions = [
            resource_defs::SendDisposition::sent("Description"),
            resource_defs::SendDisposition::sent("Name"),
        ];
        let code = generate_send_models_code(
            &[(&resource, dispositions.as_slice())],
            &HashMap::from([(resource.service_namespace, model)]),
        )
        .expect("generate update change types");
        assert!(code.contains("enum DeclaredFixtureField"), "{code}");
        assert!(
            code.contains("Self::Description => \"description\""),
            "{code}"
        );
        let update = code
            .split("pub(crate) struct TestRemovalFixtureUpdateInput")
            .nth(1)
            .unwrap();
        assert!(
            update.contains("description: crate::helpers::AttributeChange<Option<String>>"),
            "{update}"
        );
        assert!(update.contains("pub(crate) description: crate::send_model::Sent<crate::helpers::AttributeChange<Option<String>>>"), "{update}");
        assert!(update.contains("crate::send_model::Sent::new(\"test.RemovalFixture.update.description\", description)"), "{update}");
        assert!(
            update.contains("pub(crate) name: crate::send_model::Sent<String>"),
            "{update}"
        );
        assert!(
            !update.contains("crate::helpers::AttributeChange<String>"),
            "{update}"
        );
        assert!(
            update.contains(
                "crate::send_model::Sent::new(\"test.RemovalFixture.update.name\", name)"
            ),
            "{update}"
        );
        assert!(
            update.contains("_resource: &crate::helpers::UpdateResource"),
            "{update}"
        );
        assert!(
            update.contains("crate::send_model::decode_change"),
            "{update}"
        );
        assert!(
            !update.contains("&impl crate::send_model::SendResource"),
            "{update}"
        );
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

    fn test_args(format: OutputFormat, examples_dir: Option<PathBuf>) -> Args {
        Args {
            model_dir: PathBuf::from("models"),
            output_dir: PathBuf::from("output"),
            resource: None,
            format,
            examples_dir,
        }
    }

    #[test]
    fn markdown_examples_dir_must_be_an_existing_directory() {
        let root = tempfile::tempdir().expect("temporary root");
        let missing = root.path().join("missing");
        let file = tempfile::NamedTempFile::new_in(root.path()).expect("temporary file");

        let resolved = ResolvedArgs::try_from(test_args(
            OutputFormat::Markdown,
            Some(root.path().to_path_buf()),
        ))
        .expect("existing examples directory");
        assert_eq!(
            resolved.output_mode,
            OutputMode::Markdown {
                examples_dir: root.path().to_path_buf()
            }
        );
        assert!(ResolvedArgs::try_from(test_args(OutputFormat::Rust, None)).is_ok());
        assert!(ResolvedArgs::try_from(test_args(OutputFormat::Markdown, Some(missing))).is_err());
        assert!(
            ResolvedArgs::try_from(test_args(
                OutputFormat::Markdown,
                Some(file.path().to_path_buf())
            ))
            .is_err()
        );
    }

    #[test]
    fn markdown_format_requires_examples_dir() {
        let error = ResolvedArgs::try_from(test_args(OutputFormat::Markdown, None))
            .expect_err("markdown generation must require --examples-dir");

        assert_eq!(
            error.to_string(),
            "--examples-dir is required when --format is markdown"
        );
    }

    #[test]
    fn output_format_is_parsed_as_a_value_enum_with_md_alias() {
        for (value, expected) in [
            ("markdown", OutputFormat::Markdown),
            ("md", OutputFormat::Markdown),
            ("provider", OutputFormat::Provider),
            ("rust", OutputFormat::Rust),
        ] {
            let args = Args::try_parse_from([
                "smithy-codegen",
                "--model-dir",
                "models",
                "--output-dir",
                "output",
                "--format",
                value,
            ])
            .expect("known output format");
            assert_eq!(args.format, expected);
        }

        assert!(
            Args::try_parse_from([
                "smithy-codegen",
                "--model-dir",
                "models",
                "--output-dir",
                "output",
                "--format",
                "html",
            ])
            .is_err()
        );
    }

    #[test]
    fn unmatched_example_directories_are_reported_in_sorted_order() {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        for name in ["ec2_vpc", "z_unmatched", "a_unmatched"] {
            std::fs::create_dir(examples_dir.path().join(name)).expect("example directory");
        }

        let rendered = HashSet::from(["ec2_vpc".to_string()]);
        let error = ensure_examples_match_rendered_pages(examples_dir.path(), &rendered)
            .expect_err("unmatched example directories must fail generation");

        assert_eq!(
            error.to_string(),
            "Example directories without rendered pages: a_unmatched, z_unmatched"
        );
    }

    #[test]
    fn filtered_markdown_invocation_uses_all_documentable_pages_for_reverse_check() {
        let model_dir = tempfile::tempdir().expect("temporary model directory");
        let output_dir = tempfile::tempdir().expect("temporary output directory");
        std::fs::write(
            model_dir.path().join("ec2.json"),
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.ec2#CreateVpc": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.ec2#CreateVpcRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.ec2#CreateVpcRequest": {
                  "type": "structure",
                  "members": {
                    "CidrBlock": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.ec2#Vpc": {
                  "type": "structure",
                  "members": {
                    "VpcId": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("EC2 model fixture");
        let examples_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");

        run(ResolvedArgs {
            model_dir: model_dir.path().to_path_buf(),
            output_dir: output_dir.path().to_path_buf(),
            resource: Some("ec2.Vpc".to_string()),
            output_mode: OutputMode::Markdown { examples_dir },
        })
        .expect("--resource must not make other page examples look unmatched");

        assert!(output_dir.path().join("ec2/vpc.md").is_file());
        assert!(!output_dir.path().join("ec2/subnet.md").exists());
    }

    #[test]
    fn markdown_run_rejects_an_example_directory_without_a_documentable_page() {
        let examples_dir = tempfile::tempdir().expect("temporary examples directory");
        let output_dir = tempfile::tempdir().expect("temporary output directory");
        write_markdown_example_fixture(
            examples_dir.path(),
            "test",
            "fixture",
            "aws.test.Fixture { name = 'fixture' }\n",
        );
        std::fs::create_dir(examples_dir.path().join("extra_unmatched"))
            .expect("unmatched example directory");

        let all_resources = vec![provider_fixture_resource(
            "test.Fixture",
            "Name",
            &["Name"],
            vec![],
        )];
        let resources = vec![&all_resources[0]];
        let all_data_sources = Vec::new();
        let data_sources = Vec::new();
        let models: HashMap<&str, SmithyModel> =
            HashMap::from([("com.amazonaws.test", provider_fixture_model())]);

        let error = run_markdown(
            output_dir.path(),
            examples_dir.path(),
            &resources,
            &data_sources,
            &all_resources,
            &all_data_sources,
            &models,
        )
        .expect_err("the markdown run must execute its reverse example check");

        assert_eq!(
            error.to_string(),
            "Example directories without rendered pages: extra_unmatched"
        );
    }

    fn write_markdown_example_fixture(
        examples_dir: &Path,
        service: &str,
        page_stem: &str,
        source: &str,
    ) -> PathBuf {
        let directory = examples_dir.join(markdown_example_dir_name(service, page_stem));
        std::fs::create_dir(&directory).expect("example directory");
        let path = directory.join("main.crn");
        std::fs::write(&path, source).expect("example source");
        path
    }

    #[test]
    fn markdown_renderers_reject_examples_checked_for_another_page() {
        let model = provider_fixture_model();
        let resource = provider_fixture_resource("test.Fixture", "Name", &["Name"], vec![]);
        let wrong_resource = markdown_resource_example("test.Other");

        let error = generate_markdown_resource(&resource, &model, &wrong_resource)
            .expect_err("resource renderer must verify the checked type");
        assert_eq!(
            error.to_string(),
            "Documentation example checked for resource aws.test.Other cannot render resource \
             aws.test.Fixture"
        );

        let data_source = resource_defs::sts_data_sources()
            .into_iter()
            .next()
            .expect("STS data source");
        let wrong_kind = markdown_resource_example(data_source.name);
        let error = generate_markdown_data_source(&data_source, &model, &wrong_kind)
            .expect_err("data-source renderer must verify the checked kind");
        assert_eq!(
            error.to_string(),
            "Documentation example checked for resource aws.sts.CallerIdentity cannot render data \
             source aws.sts.CallerIdentity"
        );
    }

    fn assert_emitted_arn_partitions(emitted: &str, sample_rest: &str) {
        let literal = emitted
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("Some(carina_core::schema::PatternSpec::new(")?
                    .strip_suffix(", carina_core::schema::PatternMatch::Full)),")
            })
            .unwrap_or_else(|| panic!("missing emitted ARN regex:\n{emitted}"));
        let pattern = syn::parse_str::<syn::LitStr>(literal)
            .expect("emitted ARN regex must be a Rust string literal")
            .value();
        let regex = regex::Regex::new(&pattern).expect("emitted ARN regex must compile");
        for partition in carina_aws_types::Partition::ALL {
            let arn = format!("arn:{}:{sample_rest}", partition.id());
            assert!(regex.is_match(&arn), "{pattern} must accept {arn}");
        }
        let arn = format!("arn:aws-xx:{sample_rest}");
        assert!(!regex.is_match(&arn), "{pattern} must reject {arn}");
    }

    #[test]
    fn emit_arn_regexes_accept_all_partitions_and_reject_unknown_partitions() {
        for entry in ARN_VALIDATIONS.iter() {
            // Exercise inline PerKind emission even for rows whose normal
            // resource name delegates to a shared catalog constructor.
            let emitted = emit_arn_helper(
                entry.service,
                "PartitionTest",
                ArnEmitChoice::PerKind(entry),
            );
            assert_emitted_arn_partitions(&emitted, entry.sample_arn_rest);
        }
        let emitted = emit_arn_helper("ec2", "Vpc", ArnEmitChoice::ServicePrefix("ec2"));
        assert_emitted_arn_partitions(&emitted, "ec2:us-east-1:123456789012:vpc/vpc-12345678");
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
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_override(
            "AttributeType::union(vec![super::cloudfront_hosted_zone_id(), super::route53_hosted_zone_id()])",
            &HeadingPath::root("zone"),
            "aws.test",
            &mut enums,
            &mut structs,
        )
        .expect("discover the production union");
        let mut page: PageBuilder<MarkdownTarget> = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().expect("valid union display and definitions");
        assert!(
            markdown.starts_with("[Union(Zone)](#zone-union)"),
            "{markdown}"
        );
        assert!(
            markdown.contains("- [HostedZoneId](#zone-hostedzoneid)"),
            "{markdown}"
        );
        assert!(
            markdown
                .contains("- `Route53HostedZoneId (pattern: /^Z[A-Z0-9]+$/ (Full), len: ..=32)`"),
            "{markdown}"
        );
        assert_eq!(structs.len(), 1);
        assert_eq!(
            structs
                .values()
                .next()
                .unwrap()
                .union_members
                .as_ref()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn markdown_union_keys_distinguish_identical_members_at_different_positions() {
        let member = || AttributeType::union(vec![AttributeType::string(), AttributeType::bool()]);
        let ty = AttributeType::union(vec![member(), member()]);
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_attribute_type(
            &ty,
            &HeadingPath::root("choice"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap();
        assert_eq!(structs.len(), 3, "each canonical node has a declaration");
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().unwrap();
        let members: Vec<_> = markdown
            .lines()
            .filter(|line| line.starts_with("- [Union(Choice)]"))
            .collect();
        assert_eq!(members.len(), 2, "{markdown}");
        assert_ne!(members[0], members[1], "{markdown}");
        assert_eq!(markdown.matches("- `String`\n- `Bool`\n").count(), 2);
    }

    #[test]
    fn type_display_union_splits_only_on_top_level_commas() {
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_override(
            "AttributeType::union(vec![AttributeType::map(AttributeType::union(vec![super::route53_hosted_zone_id(), AttributeType::bool()])), super::cloudfront_hosted_zone_id()])",
            &HeadingPath::root("zone"),
            "aws.test",
            &mut enums,
            &mut structs,
        )
        .expect("nested constructor commas do not split the outer union");
        assert_eq!(structs.len(), 2, "both unions have definitions");
        for definition in structs.values() {
            assert_eq!(definition.union_members.as_ref().unwrap().len(), 2);
        }
        let mut page: PageBuilder<MarkdownTarget> = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().expect("valid nested union display");
        assert!(
            markdown.starts_with("[Union(Zone)](#zone-union)"),
            "{markdown}"
        );
        assert!(
            markdown.contains("- Map\\<`String`, [Union(Zone)](#zonekey-union)\\>"),
            "{markdown}"
        );
        assert!(markdown.contains("- `Bool`\n"), "{markdown}");
        assert!(markdown.contains("- [HostedZoneId]"), "{markdown}");
        assert!(
            markdown.contains("- `Route53HostedZoneId (pattern:"),
            "{markdown}"
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
            is_sent: false,
            send_via: None,
            send_reason: None,
            send_type: SendFieldType::Value,
            sdk_i32: false,
            sdk_i32_paths: Vec::new(),
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
    fn unique_name_replacement_allows_only_its_own_identity() {
        let mut attrs = vec![
            test_attr(
                "RoleName",
                "role_name",
                PLAIN_STRING_TYPE_CODE,
                false,
                true,
                true,
                None,
            ),
            test_attr(
                "Path",
                "path",
                PLAIN_STRING_TYPE_CODE,
                false,
                true,
                false,
                None,
            ),
        ];
        attrs[0].is_create_only = true;
        attrs[1].is_create_only = true;
        let mut code = String::new();
        emit_replacement_builder(
            &mut code,
            "test.Resource",
            ReplacementClass::UniqueName("roleName"),
            &attrs,
        )
        .expect("the unique-name attribute may be the sole identity");
        assert_eq!(code, "        .with_unique_name_attribute(\"role_name\")\n");

        attrs[1].is_identity = true;
        let error = emit_replacement_builder(
            &mut String::new(),
            "test.Resource",
            ReplacementClass::UniqueName("RoleName"),
            &attrs,
        )
        .expect_err("an additional identity must still be rejected");
        assert!(error.to_string().contains("composite identity"), "{error}");
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
        let model = empty_test_model();
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
        let model = empty_test_model();
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
        let model = empty_test_model();
        let ds = resource_defs::sts_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let md = generate_markdown_data_source(&ds, &model, &markdown_data_source_example(ds.name))
            .expect("md");

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
        let model = empty_test_model();
        let ds = resource_defs::iam_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let generated = generate_data_source(&ds, &model, false).expect("generate_data_source");

        assert!(
            generated.contains(".as_data_source()"),
            "must mark as data source: {generated}"
        );
        assert!(
            !generated.contains(", types}"),
            "shared constructors must not add an unused builtin types import: {generated}"
        );
        // arns is List<iam_role_arn>
        assert!(
            generated.contains(
                "AttributeSchema::provider_populated(\"arns\", AttributeType::unordered_list(super::super::iam::role::arn()))"
            ),
            "arns must be List(iam_role_arn): {generated}"
        );
        // names is an unordered list of the shared, named role-name type.
        assert!(
            generated.contains(
                "AttributeSchema::provider_populated(\"names\", AttributeType::unordered_list(carina_aws_types::iam_role_name()))"
            ),
            "names must be List(refined String): {generated}"
        );
        // Inputs are both optional strings.
        assert!(
            generated.contains(
                r#"AttributeSchema::optional("path_prefix", carina_aws_types::iam_path_prefix())"#
            ),
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

        let unrelated = resource_defs::DataSourceDef {
            name: "sts.CallerIdentity",
            ..ds
        };
        let generated =
            generate_data_source(&unrelated, &model, false).expect("unrelated data source");
        assert!(
            !generated.contains("carina_aws_types::iam_role_name()")
                && !generated.contains("carina_aws_types::iam_path_prefix()"),
            "{generated}"
        );
    }

    #[test]
    fn markdown_data_source_for_iam_roles_emits_list_types() {
        let model = empty_test_model();
        let ds = resource_defs::iam_data_sources()
            .into_iter()
            .next()
            .unwrap();

        let md = generate_markdown_data_source(&ds, &model, &markdown_data_source_example(ds.name))
            .expect("md");

        assert!(md.contains("### `arns`"), "arns section: {md}");
        assert!(md.contains("### `names`"), "names section: {md}");
        assert!(
            md.contains("- **Type:** List\\<`IamRoleArn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):iam::[^:]*:role/.+$/ (Full), custom validator)`\\> (unordered)"),
            "aggregated outputs must use MarkdownType's list renderer: {md}"
        );
        assert!(
            md.contains(r"- **Type:** `String (pattern: /^/[\x21-\x7F]*\z/ (Full), len: 1..=512)`"),
            "path_prefix constraint: {md}"
        );
        assert!(
            md.contains(
                r"- **Type:** List\<`IamRoleName (pattern: /^[A-Za-z0-9_+=,.@-]+\z/ (Full), len: 1..=64, custom validator)`\> (unordered)"
            ),
            "names element constraint: {md}"
        );
    }

    #[test]
    fn markdown_resource_for_iam_role_displays_specific_arn_type() {
        let model = provider_fixture_model();
        let mut res = provider_fixture_resource("iam.Role", "RoleName", &["RoleName"], vec![]);
        res.extra_read_only = vec!["Arn"];
        res.type_overrides = vec![("Arn", "super::super::iam::role::arn()")];

        let md = generate_markdown_resource(&res, &model, &markdown_resource_example(res.name))
            .expect("md");

        assert!(md.contains("### `arn`"), "{md}");
        assert!(md.contains("- **Type:** `IamRoleArn (pattern: /^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):iam::[^:]*:role/.+$/ (Full), custom validator)`"), "{md}");
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

        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("markdown");
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
        let model = enum_fixture_model();
        let resource = enum_fixture_resource();

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
        let model = enum_fixture_model();
        let resource = enum_fixture_resource();

        let generated = generate_resource(&resource, &model).expect("failed to generate resource");

        assert!(
            generated.contains("AttributeSchema, AttributeType, ResourceSchema"),
            "enums require AttributeType even without inline struct expressions: {generated}"
        );

        assert!(
            generated.contains("AttributeType::enum_("),
            "enum-like strings should be emitted as enum_: {generated}"
        );
        assert!(
            !generated.contains(".with_completions("),
            "enum completions should come from schema type metadata: {generated}"
        );
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
            input_rules: vec![],
            read_back: resource_defs::ReadBackOverrides::default(),
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
            nested_update_field_enums: &[],
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
                    "DomainName": { "target": "smithy.api#String", "traits": {} },
                    "IdempotencyToken": { "target": "smithy.api#String", "traits": {} },
                    "KeyAlgorithm": { "target": "smithy.api#String", "traits": {} },
                    "Options": {
                      "target": "com.amazonaws.acm#CertificateOptions",
                      "traits": {}
                    },
                    "DomainValidationOptions": {
                      "target": "com.amazonaws.acm#RequestDomainValidationList",
                      "traits": {}
                    },
                    "SubjectAlternativeNames": {
                      "target": "smithy.api#String",
                      "traits": {}
                    },
                    "ValidationMethod": {
                      "target": "com.amazonaws.acm#ValidationMethod",
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
                    },
                    "ValidationMethod": {
                      "target": "com.amazonaws.acm#ValidationMethod",
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
                    "DomainName": { "target": "smithy.api#String", "traits": {} },
                    "KeyAlgorithm": { "target": "smithy.api#String", "traits": {} },
                    "Options": {
                      "target": "com.amazonaws.acm#CertificateOptions",
                      "traits": {}
                    },
                    "SubjectAlternativeNames": {
                      "target": "smithy.api#String",
                      "traits": {}
                    },
                    "ValidationMethod": {
                      "target": "com.amazonaws.acm#ValidationMethod",
                      "traits": {}
                    },
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
                    "HttpRedirect": {
                      "target": "com.amazonaws.acm#HttpRedirect",
                      "traits": {}
                    },
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
                "com.amazonaws.acm#HttpRedirect": {
                  "type": "structure",
                  "members": {
                    "RedirectFrom": { "target": "smithy.api#String", "traits": {} },
                    "RedirectTo": { "target": "smithy.api#String", "traits": {} }
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
                    "UpdatedAt": {
                      "target": "com.amazonaws.acm#TStamp",
                      "traits": { "smithy.api#required": {} }
                    },
                    "DomainValidationOptions": {
                      "target": "com.amazonaws.acm#DomainValidationList",
                      "traits": {}
                    }
                  },
                  "traits": {}
                },
                "com.amazonaws.acm#TStamp": { "type": "timestamp" },
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
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("generate markdown");

        assert!(markdown.contains("## Enum Values"), "{markdown}");
        assert!(
            markdown.contains("### options.hostname_type (HostnameType)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_table_rows_without_pipes_keep_previous_format() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("generate markdown");

        assert!(
            markdown.contains(concat!(
                "|",
                " Value | DSL Identifier |\n",
                "|-------|----------------|\n",
                "|",
                " `TOP` | `aws.ec2.Vpc.Type.top` |\n",
            )),
            "{markdown}"
        );
        assert!(
            markdown.contains(concat!(
                "|",
                " Field | Type | Required | Description |\n",
                "|-------|------|----------|-------------|\n",
                "|",
                " `name` | `String` | No |  |\n",
            )),
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
            &markdown_resource_example(resource.name),
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
    fn markdown_smithy_collection_headings_preserve_every_hop() {
        let model = carina_smithy::parse(r##"{
            "smithy": "2.0", "shapes": {
                "test#Tags": {"type":"map", "key":{"target":"smithy.api#String"}, "value":{"target":"test#Outer"}},
                "test#Outer": {"type":"list", "member":{"target":"test#Inner"}},
                "test#Inner": {"type":"list", "member":{"target":"test#Entry"}},
                "test#Entry": {"type":"structure", "members":{"Value":{"target":"test#Kind"}}},
                "test#Kind": {"type":"enum", "members":{"A":{"target":"smithy.api#Unit", "traits":{"smithy.api#enumValue":"A"}}}}
            }
        }"##).unwrap();
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_type(
            &model,
            "test.Fixture",
            "test#Tags",
            "Tags",
            &HeadingPath::root("tags"),
            "tags",
            "aws.test",
            &HashMap::new(),
            &HashMap::new(),
            None,
            &mut enums,
            &mut structs,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            structs.keys().next().unwrap().heading_path.to_string(),
            "tags['<key>'][][]"
        );
        assert_eq!(
            enums.keys().next().unwrap().heading_path.to_string(),
            "tags['<key>'][][].value"
        );
        let roots = HashMap::from([("tags".into(), "test#Tags".into())]);
        validate_markdown_heading_paths(
            &model,
            &roots,
            structs.keys().map(|key| &key.heading_path),
        )
        .unwrap();
        validate_markdown_heading_paths(&model, &roots, enums.keys().map(|key| &key.heading_path))
            .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().unwrap();
        assert!(markdown.contains("[Struct(Entry)](#tagskey)"), "{markdown}");
        assert!(
            markdown.contains("[Value](#tagskeyvalue-value)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_heading_validation_requires_explicit_collection_hops() {
        let model = markdown_read_shape_override_model();
        let roots = HashMap::from([(
            "renewal_summary".into(),
            "com.amazonaws.acm#RenewalSummary".into(),
        )]);
        let list = HeadingPath::root("renewal_summary")
            .child(TypeEdge::Field("domain_validation_options"));
        let omitted = list.child(TypeEdge::Field("resource_record"));
        assert!(validate_markdown_heading_paths(&model, &roots, [&omitted]).is_err());
        let wrong_kind = list.child(TypeEdge::Value);
        assert!(validate_markdown_heading_paths(&model, &roots, [&wrong_kind]).is_err());
    }

    #[test]
    fn markdown_heading_validation_rejects_union_member_hops() {
        let model = markdown_read_shape_override_model();
        let roots = HashMap::from([(
            "renewal_summary".into(),
            "com.amazonaws.acm#RenewalSummary".into(),
        )]);
        let path = HeadingPath::root("renewal_summary")
            .child(TypeEdge::Field("domain_validation_options"))
            .child(TypeEdge::Item)
            .child(TypeEdge::Member(0))
            .child(TypeEdge::Field("resource_record"))
            .child(TypeEdge::Field("type"));
        let error = validate_markdown_heading_paths(&model, &roots, [&path])
            .expect_err("Smithy heading paths cannot contain identity-only union members");
        assert!(
            error.to_string().contains(&format!("`{path}`")),
            "{error:#}"
        );
        assert!(error.to_string().contains("union-member hop"), "{error:#}");
    }

    #[test]
    fn markdown_attribute_map_keys_have_distinct_heading_anchors() {
        let ty = AttributeType::refined_map(
            markdown_test_enum("Kind", "A"),
            markdown_test_enum("Kind", "B"),
            None,
        );
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_attribute_type(
            &ty,
            &HeadingPath::root("tags"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().unwrap();
        for link in ["[Kind](#tags-keys-kind)", "[Kind](#tagskey-kind)"] {
            assert!(markdown.contains(link), "{markdown}");
        }
        assert!(markdown.contains("### tags keys (Kind)\n"), "{markdown}");
        assert!(
            markdown.contains("### tags['\\<key\\>'] (Kind)\n"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_s3_bucket_policy_pins_condition_key_heading() {
        let resource = resource_defs::s3_resources()
            .into_iter()
            .find(|resource| resource.name == "s3.BucketPolicy")
            .unwrap();
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0", "shapes": {
                "com.amazonaws.s3#PutBucketPolicy": {
                  "type": "operation", "input": {"target": "com.amazonaws.s3#PutBucketPolicyRequest"}
                },
                "com.amazonaws.s3#PutBucketPolicyRequest": {
                  "type": "structure", "members": {
                    "Bucket": {"target": "smithy.api#String"},
                    "Policy": {"target": "smithy.api#String"}
                  }
                }
              }
            }"##,
        ).unwrap();
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        let headings: HashMap<_, _> = markdown
            .lines()
            .filter(|line| line.starts_with('#'))
            .zip(carina_docgen::heading_ids(&markdown))
            .collect();
        let key_anchor = headings
            .get("### policy.statement[].condition keys (ConditionOperator)")
            .expect("generated condition key heading");
        let value_anchor = headings
            .get("### policy.statement[].condition['\\<key\\>']['\\<key\\>'] union")
            .expect("generated condition value heading");
        assert_eq!(
            key_anchor,
            "policystatementcondition-keys-conditionoperator"
        );
        assert!(
            markdown.contains(&format!("[ConditionOperator](#{key_anchor})")),
            "{markdown}"
        );
        assert_ne!(key_anchor, value_anchor);
    }

    #[test]
    fn markdown_map_heading_slug_collisions_use_claim_unique_heading() {
        let ty = AttributeType::struct_(
            "Fixture",
            vec![
                StructField::optional(
                    "tags",
                    AttributeType::map(AttributeType::struct_(
                        "Entry",
                        vec![
                            StructField::optional(
                                "value",
                                AttributeType::struct_("Value", Vec::new()),
                            )
                            .build(),
                        ],
                    )),
                )
                .build(),
                StructField::optional("tagskeyvalue", AttributeType::struct_("Flat", Vec::new()))
                    .build(),
            ],
        );
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_attribute_type(
            &ty,
            &HeadingPath::root("fixture"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap();
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new()).unwrap();
        let markdown = page.finish().unwrap();
        assert!(
            markdown.contains("### fixture.tags['\\<key\\>'].value\n"),
            "{markdown}"
        );
        assert!(
            markdown
                .contains("### fixture.tagskeyvalue — aws.test.Fixture.Flat AttributeType:Flat\n"),
            "{markdown}"
        );
        for link in [
            "[Struct(Value)](#fixturetagskeyvalue)",
            "[Struct(Flat)](#fixturetagskeyvalue--awstestfixtureflat-attributetypeflat)",
        ] {
            assert!(markdown.contains(link), "{markdown}");
        }
        let ids = carina_docgen::heading_ids(&markdown);
        assert!(ids.contains(&"fixturetagskeyvalue".into()), "{markdown}");
        assert!(
            ids.contains(&"fixturetagskeyvalue--awstestfixtureflat-attributetypeflat".into()),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_acm_headings_preserve_list_element_hops() {
        let resource = resource_defs::acm_resources().remove(0);
        let markdown = generate_markdown_resource(
            &resource,
            &markdown_read_shape_override_model(),
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        for heading in [
            "domain_validation_options[]",
            "domain_validation_options[].resource_record",
            "domain_validation_options[].resource_record.type (ResourceRecordType)",
            "renewal_summary.domain_validation_options[]",
            "renewal_summary.domain_validation_options[].resource_record",
            "renewal_summary.domain_validation_options[].resource_record.type (ResourceRecordType)",
        ] {
            assert!(
                markdown
                    .lines()
                    .any(|line| line == format!("### {heading}")),
                "missing heading: {heading}"
            );
        }
    }

    fn markdown_union_member_definitions(members: Vec<AttributeType>) -> Result<String> {
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let ty = AttributeType::union(members);
        let display = discover_markdown_attribute_type(
            &ty,
            &HeadingPath::root("choice"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )?;
        let mut page = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        write_markdown_type_definitions(&mut page, &enums, &structs, &HashMap::new())?;
        Ok(page.finish()?)
    }

    fn markdown_test_enum(name: &str, value: &str) -> AttributeType {
        AttributeType::enum_(
            TypeIdentity::bare(name),
            Some(vec![value.into()]),
            Vec::new(),
            None,
            None,
        )
    }

    #[test]
    fn markdown_union_enum_headings_use_member_type_names() {
        let markdown = markdown_union_member_definitions(vec![
            markdown_test_enum("EnumA", "A"),
            markdown_test_enum("EnumB", "B"),
        ])
        .unwrap();
        for heading in ["choice (EnumA)", "choice (EnumB)"] {
            assert!(
                markdown
                    .lines()
                    .any(|line| line == format!("### {heading}")),
                "{markdown}"
            );
        }
        assert!(markdown.contains("[EnumA](#choice-enuma)"), "{markdown}");
        assert!(markdown.contains("[EnumB](#choice-enumb)"), "{markdown}");
    }

    #[test]
    fn markdown_union_struct_headings_use_member_type_names() {
        let markdown = markdown_union_member_definitions(vec![
            AttributeType::struct_("StructA", Vec::new()),
            AttributeType::struct_("StructB", Vec::new()),
        ])
        .unwrap();
        for heading in ["choice (StructA)", "choice (StructB)"] {
            assert!(
                markdown
                    .lines()
                    .any(|line| line == format!("### {heading}")),
                "{markdown}"
            );
        }
        assert!(
            markdown.contains("[Struct(StructA)](#choice-structa)"),
            "{markdown}"
        );
        assert!(
            markdown.contains("[Struct(StructB)](#choice-structb)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_union_rejects_ambiguous_enum_type_names() {
        let error = markdown_union_member_definitions(vec![
            markdown_test_enum("Same", "A"),
            markdown_test_enum("Same", "B"),
        ])
        .expect_err("same path and type name cannot distinguish union members");
        assert!(
            matches!(
                error.downcast_ref::<DocgenError>(),
                Some(DocgenError::AmbiguousUnionHeading { path, type_name })
                    if path.to_string() == "choice" && type_name == "Same"
            ),
            "{error}"
        );
    }

    #[test]
    fn markdown_union_rejects_ambiguous_struct_type_names() {
        let error = markdown_union_member_definitions(vec![
            AttributeType::struct_("Same", Vec::new()),
            AttributeType::struct_("Same", Vec::new()),
        ])
        .expect_err("same path and type name cannot distinguish union members");
        assert!(
            matches!(
                error.downcast_ref::<DocgenError>(),
                Some(DocgenError::AmbiguousUnionHeading { path, type_name })
                    if path.to_string() == "choice" && type_name == "Same"
            ),
            "{error}"
        );
    }

    #[test]
    fn markdown_acm_headings_use_dsl_field_paths() {
        let resource = resource_defs::acm_resources()
            .into_iter()
            .find(|resource| resource.name == "acm.Certificate")
            .expect("certificate resource");
        let markdown = generate_markdown_resource(
            &resource,
            &markdown_read_shape_override_model(),
            &markdown_resource_example(resource.name),
        )
        .expect("generate certificate markdown");

        for heading in [
            "### domain_validation_options[].resource_record.type (ResourceRecordType)",
            "### `options`",
            "### domain_validation_options[]",
            "### renewal_summary.domain_validation_options[]",
        ] {
            assert!(markdown.contains(heading), "missing {heading}:\n{markdown}");
        }
        assert!(
            markdown.contains("### options.validation_method (ValidationMethod)"),
            "missing nested ACM validation method:\n{markdown}"
        );
        assert!(!markdown.contains("### CertificateOptions"), "{markdown}");
        assert!(!markdown.contains("### DomainValidation\n"), "{markdown}");
    }

    #[test]
    fn acm_codegen_refinements_preserve_send_types_and_unordered_sans() {
        let resource = resource_defs::acm_resources().remove(0);
        let model = markdown_read_shape_override_model();
        let attrs = collect_resource_attributes(&resource, &model)
            .unwrap()
            .attrs;
        for (name, expected) in [
            ("domain_name", SendFieldType::String),
            ("key_algorithm", SendFieldType::Enum),
        ] {
            assert_eq!(
                attrs
                    .iter()
                    .find(|attr| attr.snake_name == name)
                    .unwrap()
                    .send_type,
                expected
            );
        }
        let sans = attrs
            .iter()
            .find(|attr| attr.snake_name == "subject_alternative_names")
            .unwrap();
        assert_eq!(
            sans.type_code,
            "AttributeType::unordered_list(carina_aws_types::acm_certificate_subject_alternative_name())"
        );
        let generated = generate_resource(&resource, &model).unwrap();
        assert!(
            generated.contains("StructField::required(\"updated_at\", AttributeType::string())"),
            "{generated}"
        );
        assert!(
            generated.contains("carina_aws_types::acm_certificate_arn()"),
            "{generated}"
        );
        assert!(
            generated.contains("carina_aws_types::acm_certificate_key_algorithm()"),
            "{generated}"
        );
        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .unwrap();
        for expected in [
            "len: 1..=64",
            "len: 1..=253",
            carina_aws_types::ACM_CERTIFICATE_REQUEST_DOMAIN_RULE,
            "aws.acm.Certificate.KeyAlgorithm.rsa_2048",
            "aws.acm.Certificate.KeyAlgorithm.ec_prime256v1",
            "aws.acm.Certificate.KeyAlgorithm.ec_secp384r1",
        ] {
            assert!(
                markdown.contains(expected),
                "missing {expected}: {markdown}"
            );
        }
        assert!(!markdown.contains("aws.acm.Certificate.KeyAlgorithm.rsa_4096"));
    }

    #[test]
    fn acm_codegen_emits_all_four_nested_validation_url_constructors() {
        let resource = resource_defs::acm_resources().remove(0);
        let generated =
            generate_resource(&resource, &markdown_read_shape_override_model()).unwrap();
        assert_eq!(
            generated
                .matches("carina_aws_types::acm_certificate_validation_url()")
                .count(),
            4,
            "{generated}"
        );
    }

    #[test]
    fn acm_resource_record_enum_identity_override_preserves_values_and_aliases() {
        let resource = resource_defs::acm_resources()
            .into_iter()
            .find(|resource| resource.name == "acm.Certificate")
            .expect("certificate resource");
        let generated = generate_resource(&resource, &markdown_read_shape_override_model())
            .expect("generate certificate schema");

        assert_eq!(
            generated
                .matches("carina_aws_types::AwsType::AcmCertificateResourceRecordType.identity()")
                .count(),
            2,
            "both projections of ACM's ResourceRecord shape need its CNAME-only identity: {generated}"
        );
        assert_eq!(
            generated
                .matches("Some(vec![\"CNAME\".to_string()])")
                .count(),
            2,
            "identity override must retain ACM's own CNAME-only model values: {generated}"
        );
        assert_eq!(
            generated
                .matches("vec![(\"CNAME\".to_string(), \"cname\".to_string())]")
                .count(),
            2,
            "identity override must retain generated DSL aliases: {generated}"
        );
    }

    #[test]
    fn markdown_acm_resource_record_enum_uses_acm_identity_and_model_values() {
        let resource = resource_defs::acm_resources()
            .into_iter()
            .find(|resource| resource.name == "acm.Certificate")
            .expect("certificate resource");
        let markdown = generate_markdown_resource(
            &resource,
            &markdown_read_shape_override_model(),
            &markdown_resource_example(resource.name),
        )
        .expect("generate certificate markdown");

        assert!(
            markdown.contains("| `CNAME` | `aws.acm.Certificate.ResourceRecordType.cname` |"),
            "the identifier table must use ACM's CNAME-only identity: {markdown}"
        );
        assert!(
            !markdown.contains("aws.route53.RecordSet.Type."),
            "ACM must not reuse Route 53's broader enum identity: {markdown}"
        );
        assert!(
            !markdown.contains("| `A` | `aws.acm.Certificate.ResourceRecordType.a` |"),
            "the identity override must not replace ACM's CNAME-only value list: {markdown}"
        );
        assert!(
            !markdown.contains("aws.acm.Certificate.DomainValidation.ResourceRecord.Type.cname"),
            "the old structural identity must not leak into the identifier table: {markdown}"
        );
    }

    #[test]
    fn markdown_heading_paths_only_use_schema_field_names() {
        let model = markdown_read_shape_override_model();
        let root_targets = HashMap::from([(
            "renewal_summary".to_string(),
            "com.amazonaws.acm#RenewalSummary".to_string(),
        )]);
        let valid = HeadingPath::root("renewal_summary")
            .child(TypeEdge::Field("domain_validation_options"))
            .child(TypeEdge::Item)
            .child(TypeEdge::Field("resource_record"))
            .child(TypeEdge::Field("type"));

        validate_markdown_heading_paths(&model, &root_targets, [&valid])
            .expect("every qualified heading segment is a schema field");

        let invalid = HeadingPath::root("renewal_summary")
            .child(TypeEdge::Field("domain_validation"))
            .child(TypeEdge::Item)
            .child(TypeEdge::Field("resource_record"))
            .child(TypeEdge::Field("type"));
        let error = validate_markdown_heading_paths(&model, &root_targets, [&invalid])
            .expect_err("a Smithy shape name is not a DSL field path");
        assert!(error.to_string().contains("domain_validation"), "{error:#}");
    }

    #[test]
    fn markdown_uses_registered_slug_for_attribute_struct_collision() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("generate markdown");

        assert!(
            markdown.contains("- **Type:** [Struct(Options)](#options-1)"),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_type_display_preserves_literal_patterns() {
        let pattern = r"^[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?\z";
        let attribute_type = AttributeType::refined_string(
            None,
            Some(PatternSpec::new(pattern, PatternMatch::Full)),
            None,
            None,
        );
        let mut enums = BTreeMap::new();
        let mut structs = MarkdownTypes::default();
        let display = discover_markdown_attribute_type(
            &attribute_type,
            &HeadingPath::root("name"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap();
        let mut page: PageBuilder<MarkdownTarget> = PageBuilder::new();
        display.write_to(&mut page);
        page.push_str("\n\n");
        page.table(["Type"], [[display.table_cell()]]).unwrap();
        page.push_str("\n");
        discover_markdown_attribute_type(
            &AttributeType::list(attribute_type),
            &HeadingPath::root("names"),
            "aws.test",
            &mut enums,
            &mut structs,
            None,
        )
        .unwrap()
        .write_to(&mut page);

        let markdown = page.finish().expect("literal regex passes page validation");
        assert_eq!(
            markdown
                .matches(&format!("`String (pattern: /{pattern}/ (Full))`"))
                .count(),
            3,
            "{markdown}"
        );
        assert!(
            markdown.contains(&format!(
                "List\\<`String (pattern: /{pattern}/ (Full))`\\> (ordered)"
            )),
            "{markdown}"
        );
    }

    #[test]
    fn markdown_renders_list_links_outside_code_spans() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
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
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("generate markdown");

        assert!(markdown.contains("### type (Type)"), "{markdown}");
        assert!(markdown.contains("### options.type (Type)"), "{markdown}");
        assert!(markdown.contains("`aws.ec2.Vpc.Type.top`"), "{markdown}");
        assert!(
            markdown.contains("`aws.ec2.Vpc.Options.Type.nested`"),
            "{markdown}"
        );
        assert!(markdown.contains("[Type](#optionstype-type)"), "{markdown}");
    }

    #[test]
    fn markdown_enum_sections_are_sorted_by_visible_heading() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
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
            &markdown_resource_example("ec2.Vpc"),
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
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("generate markdown without collision");
        let with_collision = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
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

        let markdown = generate_markdown_data_source(
            &data_source,
            &model,
            &markdown_data_source_example(data_source.name),
        )
        .expect("markdown");

        assert!(!markdown.contains("CloudFormation Type"), "{markdown}");
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("resource markdown");
        assert!(!markdown.contains("CloudFormation Type"), "{markdown}");
    }

    #[test]
    fn markdown_lists_nested_enum_identifiers_for_every_struct_path() {
        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &markdown_regression_model(),
            &markdown_resource_example("ec2.Vpc"),
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

        let markdown = generate_markdown_resource(
            &resource,
            &model,
            &markdown_resource_example(resource.name),
        )
        .expect("generate markdown");

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

        let markdown = generate_markdown_resource(
            &markdown_regression_resource(),
            &model,
            &markdown_resource_example("ec2.Vpc"),
        )
        .expect("distinct enums must generate unique headings");
        let ids = carina_docgen::heading_ids(&markdown);
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "{markdown}");
        assert!(markdown.contains("`aws.ec2.Vpc.Options.Protocol.one`"));
        assert!(markdown.contains("`aws.ec2.Vpc.Options.Protocol.two`"));
        assert!(markdown.contains("`aws.ec2.Vpc.FirstProtocol.one`"));
        assert!(markdown.contains("`aws.ec2.Vpc.SecondProtocol.one`"));
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
        let mut res = provider_fixture_resource(
            "test.RequiredFixture",
            "RoleName",
            &["Description", "Path", "RoleName"],
            vec![],
        );
        res.extra_read_only = vec!["Arn", "RoleId"];
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource("test.TypedFixture", "VpcId", &["CidrBlock"], vec![]);
        let code = provider_code_for_single_resource_with_model(
            res,
            string_fixture_model(
                &[("CidrBlock", false)],
                &[("CidrBlock", false), ("VpcId", false)],
            ),
        );

        assert!(
            code.contains("pub(crate) struct TestTypedFixtureReadFields"),
            "extractor output must be a generated typed struct: {code}"
        );
        assert!(
            code.contains(") -> TestTypedFixtureReadFields"),
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
                && code.contains(
                    "let crate::read_models_generated::TestTypedFixtureReadModelSupplement"
                ),
            "generated code must consume every extracted and hand-supplied field: {code}"
        );
        assert!(
            !code.contains("attributes: &mut HashMap<String, Value>"),
            "extractors must not communicate through string-keyed maps: {code}"
        );
    }

    #[test]
    fn legacy_map_based_read_ops_fail_generation() {
        let mut resource =
            provider_fixture_resource("test.LegacyReadOpsFixture", "VpcId", &["CidrBlock"], vec![]);
        resource.read_ops = vec![resource_defs::ReadOp {
            operation: "ReadFixture",
            fields: vec![("CidrBlock", None)],
            defaults: vec![],
        }];

        let panic = provider_code_panic_for_single_resource_with_model(
            resource,
            string_fixture_model(
                &[("CidrBlock", false)],
                &[("CidrBlock", false), ("VpcId", false)],
            ),
        )
        .expect("legacy map-based read_ops must fail generation");
        assert!(
            panic.contains(
                "legacy map-based read_ops are unsupported for test.LegacyReadOpsFixture"
            ),
            "unexpected panic: {panic}"
        );
    }

    #[test]
    fn generated_read_models_cache_validated_schemas() {
        let resource =
            provider_fixture_resource("test.CacheFixture", "VpcId", &["CidrBlock"], vec![]);
        let model = string_fixture_model(
            &[("CidrBlock", false)],
            &[("CidrBlock", false), ("VpcId", false)],
        );
        let mut models = HashMap::new();
        models.insert(resource.service_namespace, model);

        let code = generate_read_models_code(&[resource], &[], &models)
            .expect("generate test.CacheFixture read model");

        assert!(
            code.contains("static VALIDATED_SCHEMA: OnceLock"),
            "each model must cache its validated schema: {code}"
        );
        assert!(
            code.contains("cached_validated_read_schema("),
            "generated models must use the cache seam: {code}"
        );
        assert_eq!(
            code.matches("test_cache_fixture_config().schema").count(),
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
        let res = provider_fixture_resource("test.RequiredNameFixture", "Name", &["Name"], vec![]);
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource(
            "test.ListFirstFixture",
            "NatGatewayId",
            &["AllocationId", "AvailabilityMode"],
            vec![resource_defs::DerivedAttribute {
                attr: "AllocationId",
                source: resource_defs::DerivedSource::ListFirst {
                    list_member: "NatGatewayAddresses",
                    child_member: "AllocationId",
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource(
            "test.ListFirstVpcFixture",
            "EgressOnlyInternetGatewayId",
            &["VpcId"],
            vec![resource_defs::DerivedAttribute {
                attr: "VpcId",
                source: resource_defs::DerivedSource::ListFirst {
                    list_member: "Attachments",
                    child_member: "VpcId",
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(
            res,
            provider_fixture_model_without_read_fields(&["VpcId"]),
        );

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
        let res = provider_fixture_resource(
            "test.ListStringsFixture",
            "VpcEndpointId",
            &["RouteTableIds", "SubnetIds"],
            vec![],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource(
            "test.ListAndStructFixture",
            "",
            &["Options", "SubnetIds"],
            vec![resource_defs::DerivedAttribute {
                attr: "Options",
                source: resource_defs::DerivedSource::FromStruct {
                    struct_member: "Options",
                    children: &["DnsSupport"],
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());
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
        let res = provider_fixture_resource(
            "test.ListAllFixture",
            "VpcEndpointId",
            &["SecurityGroupIds"],
            vec![resource_defs::DerivedAttribute {
                attr: "SecurityGroupIds",
                source: resource_defs::DerivedSource::ListAll {
                    list_member: "Groups",
                    child_member: "GroupId",
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource(
            "test.StructFixture",
            "NatGatewayId",
            &["Options"],
            vec![resource_defs::DerivedAttribute {
                attr: "Options",
                source: resource_defs::DerivedSource::FromStruct {
                    struct_member: "Options",
                    children: &["AmazonSideAsn", "DnsSupport"],
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
            ) && code.contains("options: options.clone(),")
                && code.contains("nat_gateway_id: nat_gateway_id.clone(),"),
            "must return the nested options map as a typed field: {code}"
        );
    }

    #[test]
    fn extract_ec2_vpc_peering_connection_emits_struct_with_rename() {
        let res = provider_fixture_resource(
            "test.RenamedStructFixture",
            "VpcPeeringConnectionId",
            &["PeerOwnerId", "PeerVpcId", "VpcId"],
            vec![
                resource_defs::DerivedAttribute {
                    attr: "VpcId",
                    source: resource_defs::DerivedSource::FromStruct {
                        struct_member: "RequesterVpcInfo",
                        children: &["VpcId"],
                    },
                },
                resource_defs::DerivedAttribute {
                    attr: "PeerVpcId",
                    source: resource_defs::DerivedSource::FromStruct {
                        struct_member: "AccepterVpcInfo",
                        children: &["VpcId"],
                    },
                },
                resource_defs::DerivedAttribute {
                    attr: "PeerOwnerId",
                    source: resource_defs::DerivedSource::FromStruct {
                        struct_member: "AccepterVpcInfo",
                        children: &["OwnerId"],
                    },
                },
            ],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let res = provider_fixture_resource(
            "test.PrivateDnsFixture",
            "SubnetId",
            &["PrivateDnsNameOptionsOnLaunch"],
            vec![resource_defs::DerivedAttribute {
                attr: "PrivateDnsNameOptionsOnLaunch",
                source: resource_defs::DerivedSource::FromStruct {
                    struct_member: "PrivateDnsNameOptionsOnLaunch",
                    children: &["HostnameType", "EnableResourceNameDnsARecord"],
                },
            }],
        );
        let code = provider_code_for_single_resource_with_model(res, provider_fixture_model());

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
        let model = carina_smithy::parse(
            r##"{
              "smithy": "2.0",
              "shapes": {
                "com.amazonaws.test#CreateEmailFixture": {
                  "type": "operation",
                  "input": { "target": "com.amazonaws.test#CreateEmailFixtureRequest" },
                  "output": { "target": "smithy.api#Unit" },
                  "traits": {}
                },
                "com.amazonaws.test#CreateEmailFixtureRequest": {
                  "type": "structure",
                  "members": {
                    "Email": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": { "smithy.api#input": {} }
                },
                "com.amazonaws.test#EmailFixture": {
                  "type": "structure",
                  "members": {
                    "Email": { "target": "smithy.api#String", "traits": {} }
                  },
                  "traits": {}
                }
              }
            }"##,
        )
        .expect("parse email type fixture model");
        let resource = ResourceDef {
            input_rules: vec![],
            read_back: resource_defs::ReadBackOverrides::default(),
            // Use a registered resource name because generate_resource also
            // emits the provider's closed CloudFormation type mapping.
            name: "iam.Role",
            replacement: ReplacementClass::Conflicting,
            service_namespace: "com.amazonaws.test",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "CreateEmailFixture",
            read_structure: Some("EmailFixture"),
            read_ops: vec![],
            delete_op: "",
            update_ops: vec![],
            identifier: "",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![],
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
            nested_update_field_enums: &[],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        };

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
