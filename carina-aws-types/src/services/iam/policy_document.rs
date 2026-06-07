use carina_core::resource::Value;
use carina_core::schema::{AttributeType, StructField};

use crate::{dsl_enum_value, enum_with_dsl_aliases};

use super::condition_operator::{all_condition_operator_snake_forms, validate_condition_operators};

// ========== IAM Policy Document ==========

/// String or list of strings type — for IAM policy fields like action, resource
fn string_or_list_of_strings() -> AttributeType {
    AttributeType::union(vec![
        AttributeType::string(),
        AttributeType::list(AttributeType::string()),
    ])
}

/// String or principal struct type — for IAM policy principal fields
/// Principal can be either a string (e.g., "*") or a struct with known fields
/// (Service, AWS, Federated) whose values are string or list of strings.
fn string_or_principal_struct() -> AttributeType {
    // Struct must come before String because Union tries members in order,
    // and dsl_value_to_aws's fallthrough to value_to_json would match
    // Value::Map against String incorrectly.
    AttributeType::union(vec![
        AttributeType::struct_(
            "Principal".to_string(),
            vec![
                StructField::new("service", string_or_list_of_strings())
                    .with_provider_name("Service"),
                StructField::new("aws", string_or_list_of_strings()).with_provider_name("AWS"),
                StructField::new("federated", string_or_list_of_strings())
                    .with_provider_name("Federated"),
                StructField::new("canonical_user", string_or_list_of_strings())
                    .with_provider_name("CanonicalUser"),
            ],
        ),
        AttributeType::string(),
    ])
}

/// IAM Policy Effect enum type. Allows `Allow` / `Deny` (AWS canonical) and
/// their snake_case DSL aliases `allow` / `deny`, so users can write
/// `effect = allow` as a bare identifier — matching the bare-identifier
/// convention used by every other enum field in the same `.crn` file.
/// The namespace also makes the fully-qualified form
/// `aws.iam.PolicyDocument.Statement.Effect.allow` parse and
/// resolve: the resolver's canonical shape is
/// `<namespace>.<type_name>.<value>`, so `type_name` is the trailing
/// `Effect` segment and `namespace` carries the containing structs.
pub(crate) fn iam_policy_effect() -> AttributeType {
    enum_with_dsl_aliases(
        &["Allow", "Deny"],
        carina_core::schema::enum_identity("Effect", Some("aws.iam.PolicyDocument.Statement")),
    )
}

/// IAM Policy Document Version enum type. Allows `2012-10-17` / `2008-10-17`
/// (AWS canonical) with snake_case DSL aliases `2012_10_17` / `2008_10_17`,
/// so users can write `version = 2012_10_17` as a bare identifier —
/// matching the bare-identifier convention `effect = allow` uses in the
/// same `.crn` block. The fully-qualified form
/// `aws.iam.PolicyDocument.Version.2012_10_17` parses via `namespaced_id` +
/// the numeric-tail extension from `carina-rs/carina#3051` and resolves
/// through this namespace: the resolver's canonical shape is
/// `<namespace>.<type_name>.<value>`, so `type_name` is the trailing
/// `Version` segment and `namespace` is `aws.iam.PolicyDocument`.
pub(crate) fn iam_policy_version() -> AttributeType {
    enum_with_dsl_aliases(
        &["2012-10-17", "2008-10-17"],
        carina_core::schema::enum_identity("Version", Some("aws.iam.PolicyDocument")),
    )
}

/// IAM condition map type: Map<ConditionOperator, Map<ConditionKey, StringOrList>>
///
/// The `ConditionOperator` key set is the full cross-product produced by
/// `all_condition_operator_snake_forms()` — base operators plus the
/// `for_all_values_` / `for_any_value_` qualifier prefixes and the
/// `_if_exists` suffix — so the schema accepts every spelling that
/// `condition_operator_to_aws` already converts. See issue #340.
pub(crate) fn condition_type() -> AttributeType {
    let operator_values: Vec<String> = all_condition_operator_snake_forms();
    let operator_aliases: Vec<(String, String)> = operator_values
        .iter()
        .filter_map(|v| {
            let dsl = dsl_enum_value(v);
            if dsl == *v {
                None
            } else {
                Some((v.clone(), dsl))
            }
        })
        .collect();
    AttributeType::map_with_key(
        AttributeType::enum_(
            carina_core::schema::enum_identity(
                "ConditionOperator",
                Some("aws.iam.PolicyDocument.Statement.Condition"),
            ),
            Some(operator_values),
            operator_aliases,
            None,
            None,
        ),
        AttributeType::map(string_or_list_of_strings()),
    )
}

/// IAM Policy Statement struct type
fn iam_policy_statement() -> AttributeType {
    AttributeType::struct_(
        "Statement".to_string(),
        vec![
            StructField::new("sid", AttributeType::string()).with_provider_name("Sid"),
            StructField::new("effect", iam_policy_effect()).with_provider_name("Effect"),
            StructField::new("action", string_or_list_of_strings()).with_provider_name("Action"),
            StructField::new("not_action", string_or_list_of_strings())
                .with_provider_name("NotAction"),
            StructField::new("resource", string_or_list_of_strings())
                .with_provider_name("Resource"),
            StructField::new("not_resource", string_or_list_of_strings())
                .with_provider_name("NotResource"),
            StructField::new("principal", string_or_principal_struct())
                .with_provider_name("Principal"),
            StructField::new("not_principal", string_or_principal_struct())
                .with_provider_name("NotPrincipal"),
            StructField::new("condition", condition_type()).with_provider_name("Condition"),
        ],
    )
}

/// IAM Policy Document type
/// Validates the structure of IAM policy documents (trust policies, inline policies, etc.)
///
/// Uses `Struct` type so that `dsl_value_to_aws` and `aws_value_to_dsl` properly
/// convert between snake_case DSL field names and PascalCase IAM field names
/// (e.g., `version` <-> `Version`, `statement` <-> `Statement`).
pub fn iam_policy_document() -> AttributeType {
    AttributeType::struct_(
        "PolicyDocument".to_string(),
        vec![
            StructField::new("version", iam_policy_version()).with_provider_name("Version"),
            StructField::new("id", AttributeType::string()).with_provider_name("Id"),
            StructField::new("statement", AttributeType::list(iam_policy_statement()))
                .with_provider_name("Statement")
                .with_block_name("statement"),
        ],
    )
}

/// Validate IAM policy document structure and condition operators.
pub fn validate_iam_policy_document(value: &Value) -> Result<(), String> {
    // The IAM policy schema is flat (no `AttributeType::Ref`), so an
    // empty `defs` map is sound here (carina#3345).
    carina_core::schema::Schema::flat(iam_policy_document())
        .validate(value)
        .map_err(|e| e.to_string())?;
    validate_condition_operators(value)
}
