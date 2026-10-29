//! ACM Certificate service implementation.
//!
//! Uses `RequestCertificate` (create), `DescribeCertificate` (read),
//! `UpdateCertificateOptions` (limited update — CT logging only) and
//! `DeleteCertificate` (delete). `ListTagsForCertificate` /
//! `AddTagsToCertificate` / `RemoveTagsFromCertificate` for tags.
//!
//! Certificate **validation** is intentionally not handled here — the
//! carina-core wait construct (carina#2825) is the canonical way to
//! block downstream resources on `status == ISSUED`. See
//! `examples/acm-cert-with-wait/` for the pattern.

use std::collections::HashMap;

use aws_sdk_acm::operation::request_certificate::{
    RequestCertificateError, RequestCertificateInput,
};
use aws_sdk_acm::types::{CertificateStatus, KeyAlgorithm, ValidationMethod};
use indexmap::IndexMap;

use carina_core::provider::{AppliedIdentifier, AppliedState, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::{SendFailure, api_error_with_meta};
use crate::helpers::{
    optional_enum_attr, optional_enum_struct_field, require_string_attr, sdk_error_message,
    send_aws_operation_with_token,
};
use crate::provider::CreateCompletion;
use crate::provider_generated::AcmCertificateReadFields;
use crate::read_model::{ManagedReadModel, found_managed_read};
use crate::read_models_generated::{AcmCertificateReadModel, AcmCertificateReadModelSupplement};

const DVO_MISSING_ATTRIBUTE: &str = "domain_validation_options";

pub(crate) fn request_certificate_error(
    id: &ResourceId,
    error: SendFailure<RequestCertificateError>,
) -> ProviderError {
    api_error_with_meta("RequestCertificate failed", error).for_resource(id.clone())
}

pub(crate) fn request_certificate_outcome_unknown_error(
    id: &ResourceId,
    error: SendFailure<RequestCertificateError>,
) -> ProviderError {
    api_error_with_meta(
        "acm.Certificate may have been created; check before re-applying",
        error,
    )
    .for_resource(id.clone())
}

fn extract_string_list(value: &Value) -> Vec<String> {
    if let Value::Concrete(ConcreteValue::List(items)) = value {
        items
            .iter()
            .filter_map(|v| {
                if let Value::Concrete(ConcreteValue::String(s)) = v {
                    Some(s.expose_plaintext().to_string())
                } else {
                    None
                }
            })
            .collect()
    } else if let Value::Concrete(ConcreteValue::StringList(items)) = value {
        items
            .iter()
            .map(|item| item.expose_plaintext().to_string())
            .collect()
    } else {
        Vec::new()
    }
}

/// Compute the tag-reconciliation deltas needed to drive `desired` from
/// `current`.
///
/// Returns `(to_add, to_remove)`:
///   - `to_add` lists every key whose desired value differs from current
///     (or is absent from current). Values that aren't strings are
///     skipped; ACM tag values are strings, and a malformed entry should
///     not stall the whole reconcile.
///   - `to_remove` lists every key present in current but absent from
///     desired.
///
/// A key whose value merely changed appears in `to_add` only — never
/// `to_remove` — because `AddTagsToCertificate` upserts by key, so a
/// changed value is one add, not a remove + add round-trip.
///
/// Pure function over `IndexMap`s so it can be unit-tested without an
/// AWS client. The caller wraps the results in
/// `AddTagsToCertificate` / `RemoveTagsFromCertificate` calls.
fn compute_tag_diff(
    desired: &IndexMap<String, Value>,
    current: &IndexMap<String, Value>,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut to_add = Vec::new();
    for (key, value) in desired {
        let Value::Concrete(ConcreteValue::String(val)) = value else {
            continue;
        };
        let unchanged = matches!(
            current.get(key),
            Some(Value::Concrete(ConcreteValue::String(c))) if c == val,
        );
        if !unchanged {
            to_add.push((key.clone(), val.expose_plaintext().to_string()));
        }
    }
    let to_remove: Vec<String> = current
        .keys()
        .filter(|k| !desired.contains_key(*k))
        .cloned()
        .collect();
    (to_add, to_remove)
}

struct CertificateRequestEnums {
    validation_method: Option<ValidationMethod>,
    key_algorithm: Option<KeyAlgorithm>,
}

fn resolve_certificate_request_enums(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<CertificateRequestEnums> {
    Ok(CertificateRequestEnums {
        validation_method: optional_enum_attr(resource, schema, "validation_method")?
            .map(ValidationMethod::from),
        key_algorithm: optional_enum_attr(resource, schema, "key_algorithm")?
            .map(KeyAlgorithm::from),
    })
}

pub(crate) fn validate_acm_certificate_inputs<S>(resource: &Resource<S>) -> ProviderResult<()> {
    if resource.id.provider != "aws" || resource.id.resource_type != "acm.Certificate" {
        return Ok(());
    }
    let Some(Value::Concrete(ConcreteValue::String(domain_name))) =
        resource.get_attr("domain_name")
    else {
        return Ok(());
    };
    if resource
        .get_attr("subject_alternative_names")
        .is_some_and(|value| {
            extract_string_list(value)
                .iter()
                .any(|san| acm_dns_names_equal(san, domain_name.expose_plaintext()))
        })
    {
        return Err(ProviderError::invalid_input(
            "acm.Certificate subject_alternative_names must not contain domain_name; ACM adds the primary domain automatically",
        )
        .for_resource(resource.id.clone()));
    }
    Ok(())
}

fn normalize_acm_dns_name(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

fn acm_dns_names_equal(left: &str, right: &str) -> bool {
    normalize_acm_dns_name(left) == normalize_acm_dns_name(right)
}

fn canonicalize_acm_subject_alternative_names(
    domain_name: Option<&Value>,
    subject_alternative_names: Option<Value>,
) -> Value {
    let primary_domain = match domain_name {
        Some(Value::Concrete(ConcreteValue::String(domain))) => Some(domain.expose_plaintext()),
        _ => None,
    };
    match subject_alternative_names {
        Some(Value::Concrete(ConcreteValue::List(names))) => Value::Concrete(ConcreteValue::List(
            names
                .into_iter()
                .filter(|name| match (name, primary_domain) {
                    (Value::Concrete(ConcreteValue::String(name)), Some(primary_domain)) => {
                        !acm_dns_names_equal(name.expose_plaintext(), primary_domain)
                    }
                    _ => true,
                })
                .collect(),
        )),
        Some(value) => value,
        None => Value::Concrete(ConcreteValue::List(Vec::new())),
    }
}

fn build_request_certificate(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<RequestCertificateInput> {
    validate_acm_certificate_inputs(resource)?;
    let domain_name = require_string_attr(resource, "domain_name")?;
    let request_enums = resolve_certificate_request_enums(resource, schema)?;

    let sans = resource
        .get_attr("subject_alternative_names")
        .map(extract_string_list)
        .filter(|values| !values.is_empty());
    let options = build_create_certificate_options(resource, schema)?;

    let mut tags = Vec::new();
    if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") {
        for (key, value) in tag_map {
            if let Value::Concrete(ConcreteValue::String(val)) = value {
                tags.push(
                    aws_sdk_acm::types::Tag::builder()
                        .key(key)
                        .value(val.expose_plaintext())
                        .build()
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message(
                                "Failed to build ACM tag",
                                &e,
                            ))
                            .for_resource(resource.id.clone())
                        })?,
                );
            }
        }
    }

    RequestCertificateInput::builder()
        .domain_name(domain_name)
        .set_validation_method(request_enums.validation_method)
        .set_subject_alternative_names(sans)
        .set_options(options)
        .set_tags((!tags.is_empty()).then_some(tags))
        .set_key_algorithm(request_enums.key_algorithm)
        .build()
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message(
                "Failed to build RequestCertificate input",
                &e,
            ))
            .for_resource(resource.id.clone())
        })
}

fn build_update_certificate_options(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<Option<aws_sdk_acm::types::CertificateOptions>> {
    let Some(pref) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "certificate_transparency_logging_preference",
    )?
    else {
        return Ok(None);
    };
    Ok(Some(
        aws_sdk_acm::types::CertificateOptions::builder()
            .certificate_transparency_logging_preference(pref.into())
            .build(),
    ))
}

fn build_create_certificate_options(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<Option<aws_sdk_acm::types::CertificateOptions>> {
    use aws_sdk_acm::types::{CertificateExport, CertificateTransparencyLoggingPreference};

    let mut options = aws_sdk_acm::types::CertificateOptions::builder();
    let mut has_options = false;

    if let Some(pref) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "certificate_transparency_logging_preference",
    )? {
        options = options.certificate_transparency_logging_preference(
            CertificateTransparencyLoggingPreference::from(pref),
        );
        has_options = true;
    }
    if let Some(export) = optional_enum_struct_field(resource, schema, "options", "export")? {
        options = options.export(CertificateExport::from(export));
        has_options = true;
    }

    Ok(has_options.then(|| options.build()))
}

struct CertificateDetailReadFields {
    extracted: AcmCertificateReadFields,
    options: Option<Value>,
    validation_method: Option<Value>,
    domain_validation_options: Option<Value>,
    renewal_summary: Option<Value>,
}

impl CertificateDetailReadFields {
    fn into_model(self, id: &ResourceId, tags: Value) -> ProviderResult<AcmCertificateReadModel> {
        let Self {
            extracted,
            options,
            validation_method,
            domain_validation_options,
            renewal_summary,
        } = self;
        let (_, model) = extracted.into_model(
            id,
            AcmCertificateReadModelSupplement {
                options,
                validation_method,
                domain_validation_options,
                renewal_summary,
                tags,
            },
        )?;
        Ok(model)
    }

    /// Partial create results still use core's attribute map, but they cross
    /// that boundary only after the typed extraction is complete.
    fn into_partial_attributes(
        self,
        id: &ResourceId,
        tags: Value,
    ) -> ProviderResult<HashMap<String, Value>> {
        self.into_model(id, tags)?.into_attributes()
    }
}

fn domain_validations_value(validations: &[aws_sdk_acm::types::DomainValidation]) -> Value {
    let mut list = Vec::with_capacity(validations.len());
    for validation in validations {
        let mut fields = IndexMap::new();
        fields.insert(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String(
                validation.domain_name().to_string().into(),
            )),
        );
        if let Some(record) = validation.resource_record() {
            let record_fields = IndexMap::from([
                (
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String(record.name().to_string().into())),
                ),
                (
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::String(
                        record.r#type().as_str().to_string().into(),
                    )),
                ),
                (
                    "value".to_string(),
                    Value::Concrete(ConcreteValue::String(record.value().to_string().into())),
                ),
            ]);
            fields.insert(
                "resource_record".to_string(),
                Value::Concrete(ConcreteValue::Map(record_fields)),
            );
        }
        if let Some(status) = validation.validation_status() {
            fields.insert(
                "validation_status".to_string(),
                Value::Concrete(ConcreteValue::String(status.as_str().to_string().into())),
            );
        }
        if let Some(method) = validation.validation_method() {
            fields.insert(
                "validation_method".to_string(),
                Value::Concrete(ConcreteValue::String(method.as_str().to_string().into())),
            );
        }
        list.push(Value::Concrete(ConcreteValue::Map(fields)));
    }
    Value::Concrete(ConcreteValue::List(list))
}

/// Typed projection of a `DescribeCertificate` response. Tags are fetched by
/// the caller through ACM's separate tag API.
fn certificate_detail_read_fields(
    cert: &aws_sdk_acm::types::CertificateDetail,
) -> CertificateDetailReadFields {
    let extracted = AwsProvider::extract_acm_certificate_attributes(cert);
    let string_value =
        |value: &str| Value::Concrete(ConcreteValue::String(value.to_string().into()));
    let enum_value = |value: &str| Value::Concrete(ConcreteValue::String(value.to_string().into()));
    let options = cert.options().and_then(|options| {
        let mut fields = IndexMap::new();
        if let Some(value) = options.certificate_transparency_logging_preference() {
            fields.insert(
                "certificate_transparency_logging_preference".to_string(),
                enum_value(value.as_str()),
            );
        }
        if let Some(value) = options.export() {
            fields.insert("export".to_string(), enum_value(value.as_str()));
        }
        (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
    });

    // Measured with `aws acm describe-certificate --certificate-arn <arn>
    // --region ap-northeast-1` on 2026-10-29: ACM includes DomainName in
    // SubjectAlternativeNames. The DSL models only the additional names.
    let domain_name = extracted.domain_name().cloned();
    let extracted = extracted.map_subject_alternative_names(|subject_alternative_names| {
        Some(canonicalize_acm_subject_alternative_names(
            domain_name.as_ref(),
            subject_alternative_names,
        ))
    });
    let domain_validation_options = (!cert.domain_validation_options().is_empty())
        .then(|| domain_validations_value(cert.domain_validation_options()));
    let validation_method = cert
        .domain_validation_options()
        .first()
        .and_then(|validation| validation.validation_method())
        .map(|method| enum_value(method.as_str()));
    let renewal_summary = cert.renewal_summary().map(|summary| {
        let mut fields = IndexMap::from([
            (
                "domain_validation_options".to_string(),
                domain_validations_value(summary.domain_validation_options()),
            ),
            (
                "renewal_status".to_string(),
                enum_value(summary.renewal_status().as_str()),
            ),
            (
                "updated_at".to_string(),
                string_value(&summary.updated_at().to_string()),
            ),
        ]);
        if let Some(reason) = summary.renewal_status_reason() {
            fields.insert(
                "renewal_status_reason".to_string(),
                enum_value(reason.as_str()),
            );
        }
        Value::Concrete(ConcreteValue::Map(fields))
    });

    CertificateDetailReadFields {
        extracted,
        options,
        validation_method,
        domain_validation_options,
        renewal_summary,
    }
}

fn dvo_resource_records_populated(cert: &aws_sdk_acm::types::CertificateDetail) -> bool {
    let dvs = cert.domain_validation_options();
    !dvs.is_empty() && dvs.iter().all(|dv| dv.resource_record().is_some())
}

fn failed_certificate_reason(cert: &aws_sdk_acm::types::CertificateDetail) -> Option<String> {
    if cert.status() != Some(&CertificateStatus::Failed) {
        return None;
    }

    let mut reason = "ACM certificate request FAILED on the AWS side".to_string();
    if let Some(failure_reason) = cert.failure_reason() {
        reason.push_str(&format!(" (FailureReason: {})", failure_reason.as_str()));
    }
    reason.push_str("; retrying apply will not help. Fix the domain and replace the resource.");
    Some(reason)
}

fn partial_acm_certificate_state(
    id: &ResourceId,
    arn: &AppliedIdentifier,
    domain_name: &str,
    resource: &Resource<impl Sized>,
    request_enums: &CertificateRequestEnums,
    cert: Option<&aws_sdk_acm::types::CertificateDetail>,
) -> ProviderResult<AppliedState> {
    let arn_value = arn.as_str();
    let tags = resource
        .get_attr("tags")
        .cloned()
        .unwrap_or_else(|| Value::Concrete(ConcreteValue::Map(IndexMap::new())));
    let mut attributes = match cert {
        Some(cert) => certificate_detail_read_fields(cert).into_partial_attributes(id, tags)?,
        None => HashMap::new(),
    };

    attributes.insert(
        "certificate_arn".to_string(),
        Value::Concrete(ConcreteValue::String(arn_value.to_string().into())),
    );
    attributes
        .entry("domain_name".to_string())
        .or_insert_with(|| Value::Concrete(ConcreteValue::String(domain_name.to_string().into())));

    attributes.remove(DVO_MISSING_ATTRIBUTE);

    if let Some(validation_method) = request_enums.validation_method.as_ref() {
        attributes
            .entry("validation_method".to_string())
            .or_insert_with(|| {
                Value::Concrete(ConcreteValue::String(
                    validation_method.as_str().to_string().into(),
                ))
            });
    }
    if let Some(key_algorithm) = request_enums.key_algorithm.as_ref() {
        attributes
            .entry("key_algorithm".to_string())
            .or_insert_with(|| {
                Value::Concrete(ConcreteValue::String(
                    key_algorithm.as_str().to_string().into(),
                ))
            });
    }
    if let Some(value) = resource.get_attr("subject_alternative_names") {
        let sans = extract_string_list(value);
        attributes
            .entry("subject_alternative_names".to_string())
            .or_insert_with(|| {
                Value::Concrete(ConcreteValue::List(
                    sans.into_iter()
                        .map(|san| Value::Concrete(ConcreteValue::String(san.into())))
                        .collect(),
                ))
            });
    }
    if let Some(options) = resource.get_attr("options") {
        attributes
            .entry("options".to_string())
            .or_insert_with(|| options.clone());
    }
    if let Some(tags) = resource.get_attr("tags") {
        attributes
            .entry("tags".to_string())
            .or_insert_with(|| tags.clone());
    }

    Ok(AppliedState::from_identifier(
        id.clone(),
        arn.clone(),
        attributes,
    ))
}

fn partial_acm_certificate_create_outcome(
    id: &ResourceId,
    arn: &AppliedIdentifier,
    domain_name: &str,
    resource: &Resource<impl Sized>,
    request_enums: &CertificateRequestEnums,
    cert: Option<&aws_sdk_acm::types::CertificateDetail>,
    reason: String,
) -> ProviderResult<CreateCompletion> {
    let state = partial_acm_certificate_state(id, arn, domain_name, resource, request_enums, cert)?;
    Ok(CreateCompletion::partial(
        state,
        reason,
        vec![DVO_MISSING_ATTRIBUTE.to_string()],
    ))
}

impl AwsProvider {
    /// Issue one `RequestCertificate` call with the caller's stable token.
    pub(crate) async fn request_acm_certificate(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        client_token: &ClientToken,
    ) -> ProviderResult<Result<AppliedIdentifier, SendFailure<RequestCertificateError>>> {
        let id = resource.id.clone();
        let request = build_request_certificate(resource, schema)?;

        let req = self.acm_client.request_certificate().configure(|builder| {
            builder
                .set_domain_name(request.domain_name)
                .set_validation_method(request.validation_method)
                .set_subject_alternative_names(request.subject_alternative_names)
                .set_options(request.options)
                .set_tags(request.tags)
                .set_key_algorithm(request.key_algorithm)
        });
        // RequestCertificate accepts tags inline, avoiding a follow-up
        // AddTagsToCertificate round-trip on the create path.
        // The previous request-side `domain_validation_options`
        // (per-SAN email validation domain override) has been
        // dropped: the codegen schema now models DVO as the
        // *response* shape (with `resource_record`, `validation_status`,
        // ...) rather than the request shape (just `domain_name` +
        // `validation_domain`), so the user cannot set it from DSL
        // anymore. The override was a rare-use email-validation
        // feature; the common DNS-validation path is unaffected.
        // carina-rs/carina-provider-aws#296.

        let output = match send_aws_operation_with_token(client_token, req).await {
            Ok(output) => output,
            Err(error) => return Ok(Err(error)),
        };

        let arn = output.certificate_arn().ok_or_else(|| {
            ProviderError::api_error("RequestCertificate returned no certificate_arn")
                .for_resource(id.clone())
        })?;
        let arn = crate::helpers::applied_identifier(&id, "acm.RequestCertificate", arn)?;
        Ok(Ok(arn))
    }

    /// Continue an issued ACM create through its normal read-back path.
    pub(crate) async fn continue_acm_certificate_create(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        arn: AppliedIdentifier,
    ) -> ProviderResult<CreateCompletion> {
        let id = resource.id.clone();
        let request = build_request_certificate(resource, schema)?;
        let domain_name = request.domain_name.clone().ok_or_else(|| {
            ProviderError::internal("built RequestCertificate input has no domain_name")
                .for_resource(id.clone())
        })?;
        let request_enums = CertificateRequestEnums {
            validation_method: request.validation_method,
            key_algorithm: request.key_algorithm,
        };

        // DNS validation populates DomainValidationOptions[].ResourceRecord
        // asynchronously after RequestCertificate. Wait so the post-create
        // read-back carries the records downstream resources reference.
        // carina-rs/carina-provider-aws#298.
        if matches!(
            request_enums.validation_method.as_ref(),
            Some(ValidationMethod::Dns)
        ) {
            let dvo_outcome = wait_for_dvo_populated(
                &id,
                || async {
                    self.acm_client
                        .describe_certificate()
                        .certificate_arn(arn.as_str())
                        .send()
                        .await
                        .map_err(|e| {
                            api_error_with_meta("DescribeCertificate failed", e)
                                .for_resource(id.clone())
                        })
                        .map(|out| out.certificate().cloned())
                },
                6,
                std::time::Duration::from_secs(5),
            )
            .await;
            let dvo_outcome = match dvo_outcome {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Ok(self.create_outcome_from_read(
                        &id,
                        arn,
                        resource.resolved_attributes(),
                        Err(error),
                    ));
                }
            };
            return match dvo_outcome {
                DvoWaitOutcome::Populated(cert) => {
                    let outcome = self
                        .read_acm_certificate_from_detail(&id, &arn, Some(&cert))
                        .await;
                    Ok(self.create_outcome_from_read(
                        &id,
                        arn,
                        resource.resolved_attributes(),
                        outcome,
                    ))
                }
                DvoWaitOutcome::Incomplete {
                    certificate,
                    reason,
                } => partial_acm_certificate_create_outcome(
                    &id,
                    &arn,
                    &domain_name,
                    resource,
                    &request_enums,
                    certificate.as_ref(),
                    reason,
                ),
            };
        }

        // Read back so the State carries every server-side attribute the
        // user might wait on (`status`, `domain_validation_options`, ...).
        let outcome = self.read_acm_certificate(&id, &arn).await;
        Ok(self.create_outcome_from_read(&id, arn, resource.resolved_attributes(), outcome))
    }

    /// Read an ACM certificate via `DescribeCertificate`.
    pub(crate) async fn read_acm_certificate(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let arn = identifier.as_str();
        let output = self
            .acm_client
            .describe_certificate()
            .certificate_arn(arn)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("DescribeCertificate failed", e).for_resource(id.clone())
            })?;
        self.read_acm_certificate_from_detail(id, applied_identifier, output.certificate())
            .await
    }

    /// Build State for an ACM certificate from an already-fetched
    /// `CertificateDetail`. Skips the `DescribeCertificate` call that
    /// `read_acm_certificate` would otherwise make, letting callers
    /// reuse a `DescribeCertificate` response they already have (the
    /// create path's DVO-poll loop). Still calls `ListTagsForCertificate`
    /// since that's a separate API.
    pub(crate) async fn read_acm_certificate_from_detail(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        cert: Option<&aws_sdk_acm::types::CertificateDetail>,
    ) -> ProviderResult<ReadOutcome> {
        let Some(cert) = cert else {
            return Ok(ReadOutcome::not_found(id.clone()));
        };
        let arn = identifier.as_str();

        let fields = certificate_detail_read_fields(cert);

        // Tags are fetched separately via `ListTagsForCertificate`.
        let tag_output = self
            .acm_client
            .list_tags_for_certificate()
            .certificate_arn(arn)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("ListTagsForCertificate failed", e).for_resource(id.clone())
            })?;
        let mut tag_map = IndexMap::new();
        for tag in tag_output.tags() {
            let value = tag.value().unwrap_or("").to_string();
            tag_map.insert(
                tag.key().to_string(),
                Value::Concrete(ConcreteValue::String(value.into())),
            );
        }

        let model = fields.into_model(id, Value::Concrete(ConcreteValue::Map(tag_map)))?;

        found_managed_read(id, identifier, model)
    }

    /// Update an ACM certificate. Only `CertificateTransparencyLoggingPreference`
    /// is mutable in-place via `UpdateCertificateOptions`. Tag changes
    /// flow through `AddTagsToCertificate` / `RemoveTagsFromCertificate`.
    /// Any other field change requires resource replacement.
    pub(crate) async fn update_acm_certificate(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        if let Some(options) = build_update_certificate_options(&to, schema)? {
            self.acm_client
                .update_certificate_options()
                .certificate_arn(identifier.as_str())
                .options(options)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("UpdateCertificateOptions failed", e)
                        .for_resource(id.clone())
                })?;
        }
        self.apply_acm_tags(&id, identifier.as_str(), &to, from)
            .await?;

        Ok(self.read_acm_certificate(&id, identifier).await)
    }

    /// Reconcile tags on an existing certificate by issuing
    /// `AddTagsToCertificate` / `RemoveTagsFromCertificate` for the
    /// add/remove sets returned by [`compute_tag_diff`].
    ///
    /// Either call is skipped when its set is empty.
    async fn apply_acm_tags(
        &self,
        id: &ResourceId,
        arn: &str,
        to: &Resource<impl Sized>,
        from: &State,
    ) -> ProviderResult<()> {
        let empty = IndexMap::new();
        let desired = match to.get_attr("tags") {
            Some(Value::Concrete(ConcreteValue::Map(m))) => m,
            _ => &empty,
        };
        let current = match from.attributes.get("tags") {
            Some(Value::Concrete(ConcreteValue::Map(m))) => m,
            _ => &empty,
        };
        let (to_add, to_remove) = compute_tag_diff(desired, current);

        if !to_add.is_empty() {
            let mut req = self
                .acm_client
                .add_tags_to_certificate()
                .certificate_arn(arn);
            for (k, v) in to_add {
                let tag = aws_sdk_acm::types::Tag::builder()
                    .key(k)
                    .value(v)
                    .build()
                    .map_err(|e| {
                        ProviderError::api_error(sdk_error_message("Failed to build ACM tag", &e))
                            .for_resource(id.clone())
                    })?;
                req = req.tags(tag);
            }
            req.send().await.map_err(|e| {
                api_error_with_meta("AddTagsToCertificate failed", e).for_resource(id.clone())
            })?;
        }

        if !to_remove.is_empty() {
            let mut req = self
                .acm_client
                .remove_tags_from_certificate()
                .certificate_arn(arn);
            for k in to_remove {
                // RemoveTagsFromCertificate accepts a Tag with just `key`.
                let tag = aws_sdk_acm::types::Tag::builder()
                    .key(k)
                    .build()
                    .map_err(|e| {
                        ProviderError::api_error(sdk_error_message(
                            "Failed to build ACM tag for removal",
                            &e,
                        ))
                        .for_resource(id.clone())
                    })?;
                req = req.tags(tag);
            }
            req.send().await.map_err(|e| {
                api_error_with_meta("RemoveTagsFromCertificate failed", e).for_resource(id.clone())
            })?;
        }

        Ok(())
    }

    /// Delete an ACM certificate via `DeleteCertificate`.
    pub(crate) async fn delete_acm_certificate(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        self.acm_client
            .delete_certificate()
            .certificate_arn(identifier)
            .send()
            .await
            .map_err(|e| api_error_with_meta("DeleteCertificate failed", e).for_resource(id))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum DvoWaitOutcome {
    Populated(aws_sdk_acm::types::CertificateDetail),
    Incomplete {
        certificate: Option<aws_sdk_acm::types::CertificateDetail>,
        reason: String,
    },
}

/// Poll a fetch closure that returns an ACM `CertificateDetail`
/// until every `domain_validation_options[]` entry has its
/// `resource_record` populated, the certificate request reaches
/// AWS-side `FAILED`, or the attempt budget is exhausted. Returns the
/// last fetched `CertificateDetail` so callers can build state from it
/// without a redundant `DescribeCertificate`.
///
/// AWS populates the DNS validation record asynchronously after
/// `RequestCertificate` returns. Without this loop the post-create
/// read-back races AWS and may surface a state where
/// `domain_validation_options` is empty or has entries lacking
/// `resource_record`, leaving downstream chained references like
/// `cert.domain_validation_options[0].resource_record.name`
/// unresolvable.
///
/// `max_attempts` × `interval` bounds the total wait. The fetch
/// closure is invoked up to `max_attempts` times, with `interval`
/// between attempts (no sleep before the first attempt or after the
/// last). On timeout or AWS-side `FAILED`, returns `Incomplete`
/// instead of an error because the certificate exists and must be
/// recorded in state.
pub(crate) async fn wait_for_dvo_populated<F, Fut>(
    _id: &ResourceId,
    mut fetch: F,
    max_attempts: u32,
    interval: std::time::Duration,
) -> ProviderResult<DvoWaitOutcome>
where
    F: FnMut() -> Fut,
    Fut:
        std::future::Future<Output = ProviderResult<Option<aws_sdk_acm::types::CertificateDetail>>>,
{
    let mut last_certificate = None;
    for attempt in 0..max_attempts {
        if let Some(cert) = fetch().await? {
            if let Some(reason) = failed_certificate_reason(&cert) {
                return Ok(DvoWaitOutcome::Incomplete {
                    certificate: Some(cert),
                    reason,
                });
            }
            if dvo_resource_records_populated(&cert) {
                return Ok(DvoWaitOutcome::Populated(cert));
            }
            last_certificate = Some(cert);
        }
        if attempt + 1 < max_attempts {
            tokio::time::sleep(interval).await;
        }
    }
    Ok(DvoWaitOutcome::Incomplete {
        certificate: last_certificate,
        reason: format!(
            "ACM did not populate domain_validation_options[].resource_record within \
         {} attempts (~{:?}); the certificate exists but cannot be referenced by \
         downstream resources yet. Re-run apply to complete the read, or check \
         the ACM console for the certificate ARN.",
            max_attempts,
            interval.saturating_mul(max_attempts),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_acm::error::SdkError;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;
    use carina_aws_types::SendOutcome;

    fn acm_schema() -> ResourceSchema {
        crate::schemas::generated::acm::certificate::acm_certificate_config().schema
    }
    use aws_sdk_acm::types::{
        CertificateDetail, CertificateExport, CertificateOptions,
        CertificateTransparencyLoggingPreference, DomainValidation, RecordType, ResourceRecord,
    };
    use std::sync::Mutex;
    use std::time::Duration;

    fn id() -> ResourceId {
        ResourceId::with_provider_identity("aws", "acm.Certificate", "test-cert", None)
    }

    fn applied_identifier(value: &str) -> AppliedIdentifier {
        AppliedIdentifier::new(value).expect("test provider identifier must be non-empty")
    }

    #[test]
    fn maybe_sent_request_certificate_uses_acm_specific_outcome_unknown_message() {
        let service_error = RequestCertificateError::generic(
            ErrorMetadata::builder()
                .code("ThrottlingException")
                .message("final attempt throttled")
                .build(),
        );
        let response = HttpResponse::new(429.try_into().expect("valid status"), SdkBody::empty());
        let error: SdkError<RequestCertificateError> =
            SdkError::service_error(service_error, response);
        let id = id();

        let error = request_certificate_outcome_unknown_error(
            &id,
            SendFailure::new(SendOutcome::MaybeSent, error),
        );

        assert_eq!(
            error.to_wire().message,
            "acm.Certificate may have been created; check before re-applying"
        );
        assert_eq!(
            error
                .to_wire()
                .resource_id
                .as_ref()
                .map(|resource_id| resource_id.identity.as_str()),
            Some("test-cert")
        );
        assert!(error.to_string().contains("final attempt throttled"));
    }

    fn dv_without_rr(domain: &str) -> DomainValidation {
        DomainValidation::builder()
            .domain_name(domain)
            .build()
            .expect("builder")
    }

    fn dv_with_rr(domain: &str) -> DomainValidation {
        let rr = ResourceRecord::builder()
            .name("_abc.example.com.")
            .r#type(RecordType::Cname)
            .value("_xyz.acm-validations.aws.")
            .build()
            .expect("rr builder");
        DomainValidation::builder()
            .domain_name(domain)
            .resource_record(rr)
            .build()
            .expect("dv builder")
    }

    fn cert_with(dvs: Vec<DomainValidation>) -> CertificateDetail {
        let mut b = CertificateDetail::builder().domain_name("example.com");
        for dv in dvs {
            b = b.domain_validation_options(dv);
        }
        b.build()
    }

    fn cert_with_arn_and_status(
        arn: &str,
        status: aws_sdk_acm::types::CertificateStatus,
        dvs: Vec<DomainValidation>,
    ) -> CertificateDetail {
        let mut b = CertificateDetail::builder()
            .domain_name("example.com")
            .certificate_arn(arn)
            .status(status);
        for dv in dvs {
            b = b.domain_validation_options(dv);
        }
        b.build()
    }

    fn expect_populated(result: ProviderResult<DvoWaitOutcome>) -> CertificateDetail {
        match result.expect("expected Ok") {
            DvoWaitOutcome::Populated(cert) => cert,
            DvoWaitOutcome::Incomplete { reason, .. } => {
                panic!("expected populated DVO, got incomplete: {reason}")
            }
        }
    }

    fn expect_incomplete(
        result: ProviderResult<DvoWaitOutcome>,
    ) -> (Option<CertificateDetail>, String) {
        match result.expect("expected Ok") {
            DvoWaitOutcome::Incomplete {
                certificate,
                reason,
            } => (certificate, reason),
            DvoWaitOutcome::Populated(_) => panic!("expected incomplete DVO outcome"),
        }
    }

    fn dns_certificate_resource() -> Resource {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".to_string().into())),
        );
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string().into())),
        );
        resource
    }

    #[test]
    fn input_validation_and_read_filter_share_dns_name_normalization() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name",
            Value::Concrete(ConcreteValue::String("Example.COM.".into())),
        );
        resource.set_attr(
            "subject_alternative_names",
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("example.com".into()),
            )])),
        );

        let error = validate_acm_certificate_inputs(&resource)
            .expect_err("case and a trailing dot must not hide the primary domain");
        assert!(error.to_string().contains("subject_alternative_names"));

        let domain_name = Value::Concrete(ConcreteValue::String("Example.COM.".into()));
        let returned = Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("EXAMPLE.com".into())),
            Value::Concrete(ConcreteValue::String("www.example.com.".into())),
        ]));
        assert_eq!(
            canonicalize_acm_subject_alternative_names(Some(&domain_name), Some(returned)),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("www.example.com.".into()),
            )]))
        );
    }

    fn assert_partial_outcome_records_arn(
        outcome: CreateCompletion,
        arn: &str,
        expected_reason_parts: &[&str],
    ) {
        let CreateCompletion::Partial {
            identifier,
            attributes,
            reason,
            missing_attributes,
        } = outcome
        else {
            panic!("expected partial create outcome");
        };
        assert_eq!(identifier.as_str(), arn);
        assert_eq!(
            attributes.get("certificate_arn"),
            Some(&Value::Concrete(ConcreteValue::String(
                arn.to_string().into()
            ))),
        );
        assert_eq!(
            attributes.get("domain_name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "example.com".to_string().into()
            ))),
        );
        assert!(
            !attributes.contains_key(DVO_MISSING_ATTRIBUTE),
            "partial state must not publish incomplete DVO as complete state",
        );
        assert_eq!(missing_attributes, &[DVO_MISSING_ATTRIBUTE.to_string()],);
        for part in expected_reason_parts {
            assert!(
                reason.contains(part),
                "diagnostic reason must contain {part:?}; got: {}",
                reason,
            );
        }
    }

    /// carina#3061: the certificate ARN must be published under the
    /// schema attribute name `certificate_arn` (see
    /// `schemas/generated/acm/certificate.rs` —
    /// `AttributeSchema::provider_populated("certificate_arn", ...)`), NOT `arn`.
    ///
    /// Pre-fix the read inserted key `"arn"`, which matches no schema
    /// attribute and no DSL reference. A `wait cert { until =
    /// cert.status == issued }` would succeed (status IS published) but
    /// the captured state lacked `certificate_arn`, so a downstream
    /// `cert_issued.certificate_arn` could never resolve — the
    /// self-contradicting "add a `wait` block" apply error reported in
    /// carina-rs/carina#3061.
    #[test]
    fn read_publishes_arn_under_certificate_arn_key_not_arn() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .certificate_arn("arn:aws:acm:us-east-1:111:certificate/abc")
            .status(aws_sdk_acm::types::CertificateStatus::Issued)
            .build();

        let fields = certificate_detail_read_fields(&cert);

        assert_eq!(
            fields.extracted.certificate_arn(),
            Some(&Value::Concrete(ConcreteValue::String(
                "arn:aws:acm:us-east-1:111:certificate/abc"
                    .to_string()
                    .into()
            ))),
            "ACM read must publish the ARN under `certificate_arn` to \
             match the schema attribute the DSL references"
        );
        // status must still be published (the wait predicate relies on
        // it; this guards against a regression that breaks the wait).
        assert_eq!(
            fields.extracted.status(),
            Some(&Value::Concrete(ConcreteValue::String(
                "ISSUED".to_string().into()
            )))
        );
    }

    #[test]
    fn state_read_emits_nested_options_map_when_logging_preference_present() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .options(
                CertificateOptions::builder()
                    .certificate_transparency_logging_preference(
                        CertificateTransparencyLoggingPreference::Enabled,
                    )
                    .build(),
            )
            .build();

        let fields = certificate_detail_read_fields(&cert);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = fields.options.as_ref() else {
            panic!("expected nested options map");
        };
        assert_eq!(
            options.get("certificate_transparency_logging_preference"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ENABLED".to_string().into()
            )))
        );
    }

    #[test]
    fn certificate_detail_read_fields_emits_nested_options_export() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .options(
                CertificateOptions::builder()
                    .export(CertificateExport::Enabled)
                    .build(),
            )
            .build();

        let fields = certificate_detail_read_fields(&cert);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = fields.options.as_ref() else {
            panic!("expected nested options map");
        };
        assert_eq!(
            options.get("export"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ENABLED".to_string().into()
            )))
        );
    }

    #[test]
    fn create_acm_certificate_passes_options_export_to_sdk_request() {
        let mut options = IndexMap::new();
        options.insert(
            "export".to_string(),
            Value::Concrete(ConcreteValue::String("ENABLED".to_string().into())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options = build_create_certificate_options(&resource, &acm_schema())
            .expect("valid enum options")
            .expect("options");
        assert_eq!(request_options.export(), Some(&CertificateExport::Enabled));
    }

    #[test]
    fn create_acm_certificate_passes_both_options_fields() {
        let mut options = IndexMap::new();
        options.insert(
            "certificate_transparency_logging_preference".to_string(),
            Value::Concrete(ConcreteValue::String("DISABLED".to_string().into())),
        );
        options.insert(
            "export".to_string(),
            Value::Concrete(ConcreteValue::String("ENABLED".to_string().into())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options = build_create_certificate_options(&resource, &acm_schema())
            .expect("valid enum options")
            .expect("options");
        assert_eq!(
            request_options.certificate_transparency_logging_preference(),
            Some(&CertificateTransparencyLoggingPreference::Disabled)
        );
        assert_eq!(request_options.export(), Some(&CertificateExport::Enabled));
    }

    #[test]
    fn update_reads_nested_options_certificate_transparency_logging_preference() {
        let mut options = IndexMap::new();
        options.insert(
            "certificate_transparency_logging_preference".to_string(),
            Value::Concrete(ConcreteValue::String("DISABLED".to_string().into())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options = build_update_certificate_options(&resource, &acm_schema())
            .expect("valid enum options")
            .expect("options");
        assert_eq!(
            request_options.certificate_transparency_logging_preference(),
            Some(&CertificateTransparencyLoggingPreference::Disabled)
        );
    }

    #[tokio::test]
    async fn wait_for_dvo_returns_immediately_when_first_fetch_is_populated() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                *calls.lock().unwrap() += 1;
                Ok(Some(cert_with(vec![dv_with_rr("example.com")])))
            },
            6,
            Duration::from_millis(0),
        )
        .await;
        let cert = expect_populated(result);
        assert_eq!(*calls.lock().unwrap(), 1, "should not retry after success");
        let dvs = cert.domain_validation_options();
        assert_eq!(dvs.len(), 1);
        assert!(dvs[0].resource_record().is_some());
    }

    #[tokio::test]
    async fn wait_for_dvo_retries_until_resource_record_appears() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                let mut c = calls.lock().unwrap();
                *c += 1;
                let attempt = *c;
                drop(c);
                if attempt < 3 {
                    Ok(Some(cert_with(vec![dv_without_rr("example.com")])))
                } else {
                    Ok(Some(cert_with(vec![dv_with_rr("example.com")])))
                }
            },
            6,
            Duration::from_millis(0),
        )
        .await;
        let cert = expect_populated(result);
        assert_eq!(
            *calls.lock().unwrap(),
            3,
            "should poll until populated, not over-poll",
        );
        assert!(
            cert.domain_validation_options()
                .iter()
                .all(|dv| dv.resource_record().is_some()),
            "returned CertificateDetail must carry the populated DVO",
        );
    }

    #[tokio::test]
    async fn wait_for_dvo_requires_every_entry_to_have_resource_record() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                let mut c = calls.lock().unwrap();
                *c += 1;
                let attempt = *c;
                drop(c);
                if attempt < 2 {
                    Ok(Some(cert_with(vec![
                        dv_with_rr("example.com"),
                        dv_without_rr("alt.example.com"),
                    ])))
                } else {
                    Ok(Some(cert_with(vec![
                        dv_with_rr("example.com"),
                        dv_with_rr("alt.example.com"),
                    ])))
                }
            },
            6,
            Duration::from_millis(0),
        )
        .await;
        let cert = expect_populated(result);
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(cert.domain_validation_options().len(), 2);
        assert!(
            cert.domain_validation_options()
                .iter()
                .all(|dv| dv.resource_record().is_some()),
        );
    }

    #[tokio::test]
    async fn wait_for_dvo_times_out_after_max_attempts_with_incomplete_outcome() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                *calls.lock().unwrap() += 1;
                Ok(Some(cert_with(vec![dv_without_rr("example.com")])))
            },
            4,
            Duration::from_millis(0),
        )
        .await;
        let (_cert, reason) = expect_incomplete(result);
        assert_eq!(
            *calls.lock().unwrap(),
            4,
            "fetched exactly max_attempts times"
        );
        assert!(
            reason.contains("domain_validation_options"),
            "reason must name the attribute; got: {reason}",
        );
        assert!(
            reason.contains("4 attempts"),
            "reason must report the attempt budget; got: {reason}",
        );
    }

    #[tokio::test]
    async fn dvo_timeout_returns_partial_create_outcome_with_arn_state() {
        let arn = "arn:aws:acm:us-east-1:111:certificate/partial";
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                *calls.lock().unwrap() += 1;
                Ok(Some(cert_with_arn_and_status(
                    arn,
                    aws_sdk_acm::types::CertificateStatus::PendingValidation,
                    vec![dv_without_rr("example.com")],
                )))
            },
            4,
            Duration::from_millis(0),
        )
        .await;

        let (cert, reason) = expect_incomplete(result);
        assert_eq!(
            *calls.lock().unwrap(),
            4,
            "timeout still consumes the configured attempt budget",
        );
        let cert = cert.expect("DVO timeout must keep the last certificate for partial state");
        assert_eq!(cert.certificate_arn(), Some(arn));
        let resource = dns_certificate_resource();
        let request_enums = resolve_certificate_request_enums(&resource, &acm_schema())
            .expect("valid request enums");
        let outcome = partial_acm_certificate_create_outcome(
            &id(),
            &applied_identifier(arn),
            "example.com",
            &resource,
            &request_enums,
            Some(&cert),
            reason,
        )
        .expect("partial ACM state must be canonical");
        assert_partial_outcome_records_arn(
            outcome,
            arn,
            &["domain_validation_options", "4 attempts"],
        );
    }

    #[tokio::test]
    async fn failed_status_short_circuits_to_partial_create_outcome_with_failure_reason() {
        let arn = "arn:aws:acm:us-east-1:111:certificate/failed";
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                *calls.lock().unwrap() += 1;
                Ok(Some(
                    CertificateDetail::builder()
                        .domain_name("example.com")
                        .certificate_arn(arn)
                        .status(aws_sdk_acm::types::CertificateStatus::Failed)
                        .failure_reason(aws_sdk_acm::types::FailureReason::DomainNotAllowed)
                        .domain_validation_options(dv_without_rr("example.com"))
                        .build(),
                ))
            },
            4,
            Duration::from_millis(0),
        )
        .await;

        let (cert, reason) = expect_incomplete(result);
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "FAILED status should short-circuit instead of burning the poll budget",
        );
        let cert = cert.expect("FAILED certificates must be returned for partial state");
        assert_eq!(cert.certificate_arn(), Some(arn));
        assert_eq!(
            cert.failure_reason().map(|reason| reason.as_str()),
            Some("DOMAIN_NOT_ALLOWED"),
        );
        let resource = dns_certificate_resource();
        let request_enums = resolve_certificate_request_enums(&resource, &acm_schema())
            .expect("valid request enums");
        let outcome = partial_acm_certificate_create_outcome(
            &id(),
            &applied_identifier(arn),
            "example.com",
            &resource,
            &request_enums,
            Some(&cert),
            reason,
        )
        .expect("partial ACM state must be canonical");
        assert_partial_outcome_records_arn(
            outcome,
            arn,
            &[
                "FAILED",
                "DOMAIN_NOT_ALLOWED",
                "retrying apply will not help",
            ],
        );
    }

    #[tokio::test]
    async fn wait_for_dvo_treats_empty_list_as_pending() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                let mut c = calls.lock().unwrap();
                *c += 1;
                let attempt = *c;
                drop(c);
                if attempt < 2 {
                    Ok(None)
                } else {
                    Ok(Some(cert_with(vec![dv_with_rr("example.com")])))
                }
            },
            6,
            Duration::from_millis(0),
        )
        .await;
        let cert = expect_populated(result);
        assert_eq!(*calls.lock().unwrap(), 2);
        assert!(
            cert.domain_validation_options()
                .iter()
                .all(|dv| dv.resource_record().is_some()),
        );
    }

    fn tag_map(pairs: &[(&str, &str)]) -> IndexMap<String, Value> {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert(
                (*k).to_string(),
                Value::Concrete(ConcreteValue::String((*v).to_string().into())),
            );
        }
        m
    }

    #[test]
    fn compute_tag_diff_initial_create_adds_all() {
        let desired = tag_map(&[("Env", "dev"), ("Project", "carina")]);
        let current = IndexMap::new();
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert_eq!(
            to_add,
            vec![
                ("Env".to_string(), "dev".to_string()),
                ("Project".to_string(), "carina".to_string()),
            ],
        );
        assert!(to_remove.is_empty());
    }

    #[test]
    fn compute_tag_diff_no_change_is_noop() {
        let desired = tag_map(&[("Env", "dev")]);
        let current = tag_map(&[("Env", "dev")]);
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert!(to_add.is_empty(), "no value change ⇒ no add: {to_add:?}");
        assert!(to_remove.is_empty());
    }

    #[test]
    fn compute_tag_diff_value_change_adds_only_changed_key() {
        let desired = tag_map(&[("Env", "prod"), ("Project", "carina")]);
        let current = tag_map(&[("Env", "dev"), ("Project", "carina")]);
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert_eq!(to_add, vec![("Env".to_string(), "prod".to_string())]);
        assert!(to_remove.is_empty());
    }

    #[test]
    fn compute_tag_diff_removed_key_goes_to_remove_list() {
        let desired = tag_map(&[("Env", "dev")]);
        let current = tag_map(&[("Env", "dev"), ("Project", "carina")]);
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert!(to_add.is_empty());
        assert_eq!(to_remove, vec!["Project".to_string()]);
    }

    #[test]
    fn compute_tag_diff_full_clear_removes_all() {
        let desired = IndexMap::new();
        let current = tag_map(&[("Env", "dev"), ("Project", "carina")]);
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert!(to_add.is_empty());
        assert_eq!(to_remove, vec!["Env".to_string(), "Project".to_string()],);
    }

    #[test]
    fn compute_tag_diff_mixed_add_remove_change() {
        let desired = tag_map(&[("Env", "prod"), ("Owner", "team")]);
        let current = tag_map(&[("Env", "dev"), ("Project", "carina")]);
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        // Env value-change + new Owner → both in add list
        assert_eq!(
            to_add,
            vec![
                ("Env".to_string(), "prod".to_string()),
                ("Owner".to_string(), "team".to_string()),
            ],
        );
        // Project absent from desired → remove
        assert_eq!(to_remove, vec!["Project".to_string()]);
    }

    /// carina-provider-aws#440: native provider-ready resources carry
    /// schema-known enum values as `CanonicalEnum`, while the guest wire
    /// carries their API spelling as `String`.
    ///
    /// The old local string extractor matched only `ConcreteValue::String`, so the
    /// `validation_method(...)` call was silently skipped on the SDK
    /// request. AWS defaulted to `EMAIL`, the read-back persisted that
    /// to state, and every subsequent plan showed a permanent
    /// `EMAIL → DNS (forces replacement)` diff. The replacement-created
    /// cert hit the same bug, so the loop never converged.
    ///
    /// The request-side extractor must therefore accept both provider-facing
    /// shapes without trying to resolve DSL spelling itself.
    ///
    /// This assertion goes through the production
    /// `helpers::optional_enum_attr` path so helper regressions are caught here.
    #[test]
    fn validation_method_canonical_enum_reaches_request() {
        use carina_core::schema::{AttributeType, enum_identity};

        let attr_type = AttributeType::enum_(
            enum_identity("ValidationMethod", Some("aws.acm.Certificate")),
            Some(vec![
                "DNS".to_string(),
                "EMAIL".to_string(),
                "HTTP".to_string(),
            ]),
            vec![
                ("DNS".to_string(), "dns".to_string()),
                ("EMAIL".to_string(), "email".to_string()),
                ("HTTP".to_string(), "http".to_string()),
            ],
            None,
            None,
        );
        let schema = crate::test_rootless_type(attr_type);

        let canonical =
            schema.canonicalize_state(Value::Concrete(ConcreteValue::String("DNS".into())));
        assert!(
            matches!(&canonical, Value::Concrete(ConcreteValue::CanonicalEnum(c))
                if c.api_value() == "DNS"),
            "state API spelling must produce CanonicalEnum(DNS); got {canonical:?}"
        );

        // The fix: extracting the validation method from a Resource whose
        // `validation_method` is a `CanonicalEnum` must yield
        // `ValidationMethod::Dns`. Pre-fix the extract→parse chain
        // silently returned None.
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr("validation_method".to_string(), canonical);
        let schema = acm_schema();
        let parsed = resolve_certificate_request_enums(&resource, &schema)
            .expect("valid request enums")
            .validation_method;
        assert_eq!(
            parsed,
            Some(ValidationMethod::Dns),
            "validation_method CanonicalEnum(DNS) must extract to \
             ValidationMethod::Dns; pre-fix the silent drop caused AWS to \
             default to EMAIL (issue #440)"
        );
    }

    /// The guest receives the same API spelling as a plain wire string.
    ///
    /// This assertion goes through the production
    /// `helpers::optional_enum_attr` path so helper regressions are caught here.
    #[test]
    fn validation_method_string_still_reaches_request() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string().into())),
        );
        let schema = acm_schema();
        let parsed = resolve_certificate_request_enums(&resource, &schema)
            .expect("valid request enums")
            .validation_method;
        assert_eq!(parsed, Some(ValidationMethod::Dns));
    }

    #[test]
    fn validation_method_http_reaches_built_request() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".into())),
        );
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("HTTP".into())),
        );
        let request = build_request_certificate(&resource, &acm_schema())
            .expect("valid RequestCertificate input");

        assert_eq!(
            request.validation_method(),
            Some(&ValidationMethod::Http),
            "a schema-valid HTTP method must be set on RequestCertificate"
        );
    }

    #[test]
    fn validation_method_dns_reaches_built_request() {
        let resource = dns_certificate_resource();
        let request = build_request_certificate(&resource, &acm_schema())
            .expect("valid RequestCertificate input");

        assert_eq!(
            request.validation_method(),
            Some(&ValidationMethod::Dns),
            "DNS must be set on the RequestCertificate input"
        );
    }

    #[test]
    fn compute_tag_diff_skips_non_string_values() {
        // Defensive: a Map<String, Value> may, in theory, hold non-string
        // entries. ACM tag values are strings, so anything else is skipped
        // rather than panicking.
        let mut desired = IndexMap::new();
        desired.insert("Count".to_string(), Value::Concrete(ConcreteValue::Int(3)));
        desired.insert(
            "Env".to_string(),
            Value::Concrete(ConcreteValue::String("dev".to_string().into())),
        );
        let current = IndexMap::new();
        let (to_add, to_remove) = compute_tag_diff(&desired, &current);
        assert_eq!(to_add, vec![("Env".to_string(), "dev".to_string())]);
        assert!(to_remove.is_empty());
    }

    #[tokio::test]
    async fn wait_for_dvo_propagates_fetch_errors_immediately() {
        let calls = Mutex::new(0u32);
        let result = wait_for_dvo_populated(
            &id(),
            || async {
                *calls.lock().unwrap() += 1;
                Err(ProviderError::api_error("DescribeCertificate boom"))
            },
            6,
            Duration::from_millis(0),
        )
        .await;
        assert!(result.is_err(), "expected propagated error");
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "should not retry after a non-empty fetch error",
        );
    }
}
