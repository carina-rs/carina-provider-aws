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

use carina_core::provider::{AppliedIdentifier, Observation, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::CompiledResourceSchema;
use carina_plugin_sdk::{DesiredScalar, DesiredValue, DesiredValueMut, DesiredView};

use crate::AwsProvider;
use crate::aws_operation::ClientToken;
use crate::error_helpers::{SendFailure, api_error_with_meta};
use crate::helpers::{enum_attr_error, sdk_error_message, send_aws_operation_with_token};
use crate::provider::CreateCompletion;
use crate::provider_generated::AcmCertificateReadFields;
use crate::read_back::{ReadBackKind, ReadBackRequirements};
use crate::read_model::found_managed_read;
use crate::read_models_generated::{AcmCertificateReadModel, AcmCertificateReadModelSupplement};
use crate::send_models_generated::{AcmCertificateCreateInput, AcmCertificateUpdateInput};

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

fn normalize_acm_dns_name(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

fn canonical_acm_san(name: &str, primary_domain: Option<&str>) -> Option<String> {
    let canonical = normalize_acm_dns_name(name);
    if primary_domain.is_some_and(|primary| canonical == normalize_acm_dns_name(primary)) {
        None
    } else {
        Some(canonical)
    }
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
                .filter_map(|name| match name {
                    Value::Concrete(ConcreteValue::String(name)) => {
                        canonical_acm_san(name.expose_plaintext(), primary_domain)
                            .map(|name| Value::Concrete(ConcreteValue::String(name.into())))
                    }
                    other => Some(other),
                })
                .collect(),
        )),
        Some(value) => value,
        None => Value::Concrete(ConcreteValue::List(Vec::new())),
    }
}

/// Canonicalize authored SANs to the same representation returned by ACM.
/// ACM accepts the apex in the request and adds it to the response, so the
/// provider models only additional names and removes the apex on both sides.
pub(crate) fn normalize_acm_certificate_inputs(input: &mut DesiredView<'_>) {
    if !input.is_resource("aws", "acm.Certificate") {
        return;
    }

    let primary_domain = match input.attribute("domain_name") {
        Some(DesiredValue::Known(value)) => value.as_str().map(str::to_owned),
        _ => None,
    };
    let Some(DesiredValueMut::List(names)) = input.attribute_mut("subject_alternative_names")
    else {
        return;
    };
    names.retain_known(|scalar| {
        let DesiredScalar::String(name) = scalar else {
            return true;
        };
        match canonical_acm_san(name.expose_plaintext(), primary_domain.as_deref()) {
            Some(canonical) => {
                *name = canonical.into();
                true
            }
            None => false,
        }
    });
}

#[cfg(test)]
fn build_request_certificate(
    resource: &Resource<impl Sized>,
    schema: &CompiledResourceSchema,
) -> ProviderResult<RequestCertificateInput> {
    let input = AcmCertificateCreateInput::decode(resource, schema)?;
    build_request_certificate_input(resource, schema, input)
}

fn build_request_certificate_input(
    resource: &Resource<impl Sized>,
    schema: &CompiledResourceSchema,
    input: AcmCertificateCreateInput,
) -> ProviderResult<RequestCertificateInput> {
    let crate::send_models_generated::AcmCertificateCreateParts {
        domain_name,
        key_algorithm,
        options,
        subject_alternative_names,
        tags,
        validation_method,
    } = input.into_parts();
    let domain_name = domain_name.into_inner();
    let key_algorithm = key_algorithm.into_inner();
    let options = options.into_inner();
    let subject_alternative_names = subject_alternative_names.into_inner();
    let tags = tags.into_inner();
    let validation_method = validation_method.into_inner();
    let request_enums = CertificateRequestEnums {
        validation_method: validation_method.as_deref().map(ValidationMethod::from),
        key_algorithm: key_algorithm.as_deref().map(KeyAlgorithm::from),
    };

    let sans = subject_alternative_names
        .map(|names| {
            names
                .into_iter()
                .filter_map(|name| canonical_acm_san(&name, Some(&domain_name)))
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty());
    let options = build_create_certificate_options(&resource.id, options.as_ref(), schema)?;

    let mut sdk_tags = Vec::new();
    if let Some(tag_map) = tags {
        for (key, value) in tag_map {
            sdk_tags.push(
                aws_sdk_acm::types::Tag::builder()
                    .key(key)
                    .value(value)
                    .build()
                    .map_err(|e| {
                        ProviderError::api_error(sdk_error_message("Failed to build ACM tag", &e))
                            .for_resource(resource.id.clone())
                    })?,
            );
        }
    }

    RequestCertificateInput::builder()
        .domain_name(domain_name)
        .set_validation_method(request_enums.validation_method)
        .set_subject_alternative_names(sans)
        .set_options(options)
        .set_tags((!sdk_tags.is_empty()).then_some(sdk_tags))
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

fn build_create_certificate_options(
    id: &ResourceId,
    value: Option<&Value>,
    schema: &CompiledResourceSchema,
) -> ProviderResult<Option<aws_sdk_acm::types::CertificateOptions>> {
    use aws_sdk_acm::types::CertificateExport;

    let mut options = aws_sdk_acm::types::CertificateOptions::builder();
    let mut has_options = false;

    let field = |name| {
        let value = match value {
            Some(Value::Concrete(ConcreteValue::Map(fields))) => fields.get(name),
            Some(_) => value,
            None => None,
        };
        carina_core::resource::enum_attr_at_schema_path(schema, "options", &[name], value)
            .map_err(|error| enum_attr_error(id, error))
    };

    if let Some(pref) = field("certificate_transparency_logging_preference")? {
        #[allow(deprecated)]
        let configured = options.certificate_transparency_logging_preference(
            aws_sdk_acm::types::CertificateTransparencyLoggingPreference::from(pref.api_value()),
        );
        options = configured;
        has_options = true;
    }
    if let Some(export) = field("export")? {
        options = options.export(CertificateExport::from(export.api_value()));
        has_options = true;
    }
    if let Some(validation_method) = field("validation_method")? {
        options = options.validation_method(ValidationMethod::from(validation_method.api_value()));
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
        #[allow(deprecated)]
        let logging_preference = options.certificate_transparency_logging_preference();
        if let Some(value) = logging_preference {
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

#[derive(Debug, PartialEq, Eq)]
enum DvoReadiness {
    Populated,
    Incomplete,
    Failed(String),
}

fn classify_dvo(cert: Option<&aws_sdk_acm::types::CertificateDetail>) -> DvoReadiness {
    match cert {
        Some(cert) => {
            if let Some(reason) = failed_certificate_reason(cert) {
                return DvoReadiness::Failed(reason);
            }
            if dvo_resource_records_populated(cert) {
                DvoReadiness::Populated
            } else {
                DvoReadiness::Incomplete
            }
        }
        None => DvoReadiness::Incomplete,
    }
}

impl AwsProvider {
    /// Issue one `RequestCertificate` call with the caller's stable token.
    pub(crate) async fn request_acm_certificate(
        &self,
        resource: &Resource<impl Sized>,
        schema: &CompiledResourceSchema,
        client_token: &ClientToken,
    ) -> ProviderResult<(
        carina_core::provider::Sent,
        Result<AppliedIdentifier, SendFailure<RequestCertificateError>>,
    )> {
        let id = resource.id.clone();
        let input = AcmCertificateCreateInput::decode(resource, schema)?;
        let sent = input.sent();
        let request = build_request_certificate_input(resource, schema, input)?;

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
            Err(error) => return Ok((sent, Err(error))),
        };

        let arn = output.certificate_arn().ok_or_else(|| {
            ProviderError::api_error("RequestCertificate returned no certificate_arn")
                .for_resource(id.clone())
        })?;
        let arn = crate::helpers::applied_identifier(&id, "acm.RequestCertificate", arn)?;
        Ok((sent, Ok(arn)))
    }

    /// Continue an issued ACM create through its normal read-back path.
    pub(crate) async fn continue_acm_certificate_create(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &CompiledResourceSchema,
        arn: AppliedIdentifier,
    ) -> ProviderResult<CreateCompletion> {
        let attributes = resource.resolved_attributes();
        let requirements = self.declared_read_back_requirements(&resource.id, &attributes)?;
        Ok(self
            .observe_read_back(
                &resource.id,
                arn,
                attributes,
                requirements,
                ReadBackKind::Create,
            )
            .await)
    }

    /// Fetch once and classify the declared DNS read-back requirement using
    /// the same SDK predicate as the former DVO waiter. Reuse the detail for state.
    pub(crate) async fn read_acm_certificate_back(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        mut fallback: HashMap<String, Value>,
        requirements: ReadBackRequirements,
        attribute: &str,
        kind: ReadBackKind,
    ) -> CreateCompletion {
        fallback.insert(
            "certificate_arn".to_string(),
            Value::Concrete(ConcreteValue::String(
                identifier.as_str().to_string().into(),
            )),
        );
        let output = self
            .acm_client
            .describe_certificate()
            .certificate_arn(identifier.as_str())
            .send()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                return self.classify_read_back(
                    id,
                    identifier,
                    fallback,
                    Err(api_error_with_meta("DescribeCertificate failed", error)
                        .for_resource(id.clone())),
                    requirements,
                );
            }
        };
        let readiness = match kind {
            ReadBackKind::Create => classify_dvo(output.certificate()),
            ReadBackKind::Update => {
                if output
                    .certificate()
                    .is_some_and(dvo_resource_records_populated)
                {
                    DvoReadiness::Populated
                } else {
                    DvoReadiness::Incomplete
                }
            }
        };
        let mut outcome = self
            .read_acm_certificate_from_detail(id, &identifier, output.certificate())
            .await;
        if let DvoReadiness::Failed(reason) = readiness {
            let observation = match outcome {
                Ok(read) => Observation::Read(read),
                Err(error) => Observation::ReadFailed(error),
            };
            return CreateCompletion::Failed {
                identifier,
                observation,
                error: ProviderError::api_error(reason).for_resource(id.clone()),
            };
        }
        if readiness == DvoReadiness::Incomplete
            && let Ok(ReadOutcome::Found { state }) = outcome
        {
            let (state_id, state_identifier, mut attributes) = state.into_parts();
            attributes.remove(attribute);
            outcome = Ok(ReadOutcome::Found {
                state: crate::read_model::partial_applied_state(
                    state_id,
                    state_identifier,
                    attributes,
                ),
            });
        }
        self.classify_read_back(id, identifier, fallback, outcome, requirements)
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
    /// host-driven read-back). Still calls `ListTagsForCertificate`
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

    /// Update an ACM certificate's tags. Every other writable schema field is
    /// create-only and therefore reaches this path only through replacement.
    pub(crate) async fn update_acm_certificate(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<()> {
        let input = AcmCertificateUpdateInput::decode(&to)?;
        let crate::send_models_generated::AcmCertificateUpdateParts { tags } = input.into_parts();
        let tags = tags.into_inner();
        let tags = crate::send_model::optional_tags_value(tags);
        self.apply_acm_tags(&id, identifier.as_str(), tags.as_ref(), from)
            .await?;

        Ok(())
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
        desired_tags: Option<&Value>,
        from: &State,
    ) -> ProviderResult<()> {
        let empty = IndexMap::new();
        let desired = match desired_tags {
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

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_acm::error::SdkError;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    fn acm_schema() -> &'static CompiledResourceSchema {
        crate::test_resource_schema("acm.Certificate")
    }
    use aws_sdk_acm::types::{
        CertificateDetail, CertificateExport, CertificateOptions, DomainValidation, RecordType,
        ResourceRecord,
    };

    fn id() -> ResourceId {
        ResourceId::with_provider_identity("aws", "acm.Certificate", "test-cert", None)
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
            SendFailure::new(
                carina_aws_types::SendClassification::MaybeSent {
                    final_attempt: carina_aws_types::FinalAttempt::Sent,
                },
                error,
            ),
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

    fn cert_with(dvs: Vec<DomainValidation>) -> CertificateDetail {
        let mut b = CertificateDetail::builder().domain_name("example.com");
        for dv in dvs {
            b = b.domain_validation_options(dv);
        }
        b.build()
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

        let mut desired =
            carina_plugin_sdk::NativeDesiredNormalization::from_core(&mut resource).unwrap();
        normalize_acm_certificate_inputs(&mut desired.view());
        desired.finish().unwrap();
        assert_eq!(
            resource.get_attr("subject_alternative_names"),
            Some(&Value::Concrete(ConcreteValue::List(Vec::new())))
        );

        let domain_name = Value::Concrete(ConcreteValue::String("Example.COM.".into()));
        let returned = Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("EXAMPLE.com".into())),
            Value::Concrete(ConcreteValue::String("www.example.com.".into())),
        ]));
        assert_eq!(
            canonicalize_acm_subject_alternative_names(Some(&domain_name), Some(returned)),
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("www.example.com".into()),
            )]))
        );
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
        #[allow(deprecated)]
        let options = CertificateOptions::builder()
            .certificate_transparency_logging_preference(
                aws_sdk_acm::types::CertificateTransparencyLoggingPreference::Enabled,
            )
            .build();
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .options(options)
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
    fn certificate_detail_read_fields_omits_nested_options_validation_method() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .options(
                CertificateOptions::builder()
                    .export(CertificateExport::Disabled)
                    .validation_method(ValidationMethod::Dns)
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
                "DISABLED".to_string().into()
            )))
        );
        assert!(!options.contains_key("validation_method"));
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

        let request_options = build_create_certificate_options(
            &resource.id,
            resource.get_attr("options"),
            acm_schema(),
        )
        .expect("valid enum options")
        .expect("options");
        assert_eq!(request_options.export(), Some(&CertificateExport::Enabled));
    }

    #[test]
    fn create_acm_certificate_passes_all_options_fields() {
        let mut options = IndexMap::new();
        options.insert(
            "certificate_transparency_logging_preference".to_string(),
            Value::Concrete(ConcreteValue::String("DISABLED".to_string().into())),
        );
        options.insert(
            "export".to_string(),
            Value::Concrete(ConcreteValue::String("ENABLED".to_string().into())),
        );
        options.insert(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string().into())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options = build_create_certificate_options(
            &resource.id,
            resource.get_attr("options"),
            acm_schema(),
        )
        .expect("valid enum options")
        .expect("options");
        #[allow(deprecated)]
        let logging_preference = request_options
            .certificate_transparency_logging_preference()
            .map(|value| value.as_str());
        assert_eq!(logging_preference, Some("DISABLED"));
        assert_eq!(request_options.export(), Some(&CertificateExport::Enabled));
        assert_eq!(
            request_options.validation_method(),
            Some(&ValidationMethod::Dns)
        );
    }

    #[test]
    fn create_reads_nested_options_certificate_transparency_logging_preference() {
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

        let request_options = build_create_certificate_options(
            &resource.id,
            resource.get_attr("options"),
            acm_schema(),
        )
        .expect("valid enum options")
        .expect("options");
        #[allow(deprecated)]
        let logging_preference = request_options
            .certificate_transparency_logging_preference()
            .map(|value| value.as_str());
        assert_eq!(logging_preference, Some("DISABLED"));
    }

    #[test]
    fn dvo_readiness_classifies_one_observation() {
        assert_eq!(classify_dvo(None), DvoReadiness::Incomplete);
        assert_eq!(
            classify_dvo(Some(&cert_with(vec![]))),
            DvoReadiness::Incomplete
        );
        assert_eq!(
            classify_dvo(Some(&cert_with(vec![dv_without_rr("example.com")]))),
            DvoReadiness::Incomplete
        );
        assert_eq!(
            classify_dvo(Some(&cert_with(vec![
                dv_with_rr("example.com"),
                dv_without_rr("alt.example.com")
            ]))),
            DvoReadiness::Incomplete
        );
        assert_eq!(
            classify_dvo(Some(&cert_with(vec![dv_with_rr("example.com")]))),
            DvoReadiness::Populated
        );
    }

    #[test]
    fn dvo_failed_is_terminal_and_keeps_failure_reason() {
        let cert = CertificateDetail::builder()
            .status(CertificateStatus::Failed)
            .failure_reason(aws_sdk_acm::types::FailureReason::DomainNotAllowed)
            .build();
        let DvoReadiness::Failed(reason) = classify_dvo(Some(&cert)) else {
            panic!("FAILED must be terminal")
        };
        assert!(reason.contains("FAILED"));
        assert!(reason.contains("DOMAIN_NOT_ALLOWED"));
        assert!(!reason.contains("budget"));
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

        let canonical = schema
            .canonicalize_state(Value::Concrete(ConcreteValue::String("DNS".into())))
            .into_present()
            .expect("validation-method fixture must remain present");
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
        resource.set_attr(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".into())),
        );
        resource.set_attr("validation_method".to_string(), canonical);
        let request =
            build_request_certificate(&resource, acm_schema()).expect("valid request enums");
        assert_eq!(
            request.validation_method().cloned(),
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
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String("example.com".into())),
        );
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string().into())),
        );
        let request =
            build_request_certificate(&resource, acm_schema()).expect("valid request enums");
        assert_eq!(request.validation_method(), Some(&ValidationMethod::Dns));
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
        let request = build_request_certificate(&resource, acm_schema())
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
        let request = build_request_certificate(&resource, acm_schema())
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
}
