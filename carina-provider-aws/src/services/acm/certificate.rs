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

use aws_sdk_acm::types::{CertificateStatus, ValidationMethod};
use indexmap::IndexMap;

use carina_core::provider::{CreateOutcome, ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    optional_enum_attr, optional_enum_struct_field, require_string_attr, sdk_error_message,
};

const DVO_MISSING_ATTRIBUTE: &str = "domain_validation_options";

fn extract_string_list(value: &Value) -> Vec<String> {
    if let Value::Concrete(ConcreteValue::List(items)) = value {
        items
            .iter()
            .filter_map(|v| {
                if let Value::Concrete(ConcreteValue::String(s)) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect()
    } else if let Value::Concrete(ConcreteValue::StringList(items)) = value {
        items.clone()
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
            to_add.push((key.clone(), val.clone()));
        }
    }
    let to_remove: Vec<String> = current
        .keys()
        .filter(|k| !desired.contains_key(*k))
        .cloned()
        .collect();
    (to_add, to_remove)
}

fn parse_validation_method(input: &str) -> Option<ValidationMethod> {
    match input.to_ascii_uppercase().as_str() {
        "DNS" => Some(ValidationMethod::Dns),
        "EMAIL" => Some(ValidationMethod::Email),
        _ => None,
    }
}

fn build_update_certificate_options(
    resource: &Resource,
    schema: &ResourceSchema,
) -> Option<aws_sdk_acm::types::CertificateOptions> {
    let pref = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "certificate_transparency_logging_preference",
    )?;
    Some(
        aws_sdk_acm::types::CertificateOptions::builder()
            .certificate_transparency_logging_preference(pref.into())
            .build(),
    )
}

fn build_create_certificate_options(
    resource: &Resource,
    schema: &ResourceSchema,
) -> Option<aws_sdk_acm::types::CertificateOptions> {
    use aws_sdk_acm::types::{CertificateExport, CertificateTransparencyLoggingPreference};

    let mut options = aws_sdk_acm::types::CertificateOptions::builder();
    let mut has_options = false;

    if let Some(pref) = optional_enum_struct_field(
        resource,
        schema,
        "options",
        "certificate_transparency_logging_preference",
    ) {
        options = options.certificate_transparency_logging_preference(
            CertificateTransparencyLoggingPreference::from(pref),
        );
        has_options = true;
    }
    if let Some(export) = optional_enum_struct_field(resource, schema, "options", "export") {
        options = options.export(CertificateExport::from(export));
        has_options = true;
    }

    has_options.then(|| options.build())
}

/// Map a `DescribeCertificate` `CertificateDetail` to Carina resource
/// attributes. Pure (no AWS calls) so the attribute-key contract is
/// unit-testable offline — tags are fetched separately by the caller.
///
/// Attribute keys MUST match the codegen schema
/// (`schemas/generated/acm/certificate.rs`). In particular the ARN is
/// published under `certificate_arn` (the schema attribute / DSL
/// reference), not `arn`: a `wait cert { until = cert.status ==
/// issued }` polls this read, succeeds on `status`, and a downstream
/// `cert_issued.certificate_arn` resolves against the captured state —
/// emitting `arn` instead silently broke that chain (carina#3061).
fn certificate_detail_to_attributes(
    cert: &aws_sdk_acm::types::CertificateDetail,
) -> HashMap<String, Value> {
    let mut attributes: HashMap<String, Value> = HashMap::new();
    if let Some(v) = cert.domain_name() {
        attributes.insert(
            "domain_name".to_string(),
            Value::Concrete(ConcreteValue::String(v.to_string())),
        );
    }
    if let Some(v) = cert.certificate_arn() {
        attributes.insert(
            "certificate_arn".to_string(),
            Value::Concrete(ConcreteValue::String(v.to_string())),
        );
    }
    if let Some(v) = cert.status() {
        attributes.insert(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
        );
    }
    if let Some(v) = cert.r#type() {
        attributes.insert(
            "type".to_string(),
            Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
        );
    }
    if let Some(v) = cert.key_algorithm() {
        attributes.insert(
            "key_algorithm".to_string(),
            Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
        );
    }
    if let Some(v) = cert.renewal_eligibility() {
        attributes.insert(
            "renewal_eligibility".to_string(),
            Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
        );
    }
    if let Some(opts) = cert.options() {
        let mut m: IndexMap<String, Value> = IndexMap::new();
        if let Some(v) = opts.certificate_transparency_logging_preference() {
            m.insert(
                "certificate_transparency_logging_preference".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if let Some(v) = opts.export() {
            m.insert(
                "export".to_string(),
                Value::Concrete(ConcreteValue::String(v.as_str().to_string())),
            );
        }
        if !m.is_empty() {
            attributes.insert(
                "options".to_string(),
                Value::Concrete(ConcreteValue::Map(m)),
            );
        }
    }
    let sans = cert.subject_alternative_names();
    if !sans.is_empty() {
        attributes.insert(
            "subject_alternative_names".to_string(),
            Value::Concrete(ConcreteValue::List(
                sans.iter()
                    .map(|s| Value::Concrete(ConcreteValue::String(s.clone())))
                    .collect(),
            )),
        );
    }
    // domain_validation_options carries the DNS records the user
    // needs to publish to satisfy DNS validation — the wait
    // construct's primary use case.
    let dvs = cert.domain_validation_options();
    if !dvs.is_empty() {
        // Emit the nested `resource_record: { name, type, value }`
        // shape that matches the codegen-generated schema (#296).
        // The DSL chained access is
        // `cert.domain_validation_options[0].resource_record.value`,
        // not the older flat `resource_record_value`.
        let mut list: Vec<Value> = Vec::with_capacity(dvs.len());
        for dv in dvs {
            let mut m: IndexMap<String, Value> = IndexMap::new();
            m.insert(
                "domain_name".to_string(),
                Value::Concrete(ConcreteValue::String(dv.domain_name().to_string())),
            );
            if let Some(rr) = dv.resource_record() {
                let mut rr_map: IndexMap<String, Value> = IndexMap::new();
                rr_map.insert(
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String(rr.name().to_string())),
                );
                rr_map.insert(
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::String(rr.r#type().as_str().to_string())),
                );
                rr_map.insert(
                    "value".to_string(),
                    Value::Concrete(ConcreteValue::String(rr.value().to_string())),
                );
                m.insert(
                    "resource_record".to_string(),
                    Value::Concrete(ConcreteValue::Map(rr_map)),
                );
            }
            if let Some(status) = dv.validation_status() {
                m.insert(
                    "validation_status".to_string(),
                    Value::Concrete(ConcreteValue::String(status.as_str().to_string())),
                );
            }
            if let Some(method) = dv.validation_method() {
                m.insert(
                    "validation_method".to_string(),
                    Value::Concrete(ConcreteValue::String(method.as_str().to_string())),
                );
            }
            list.push(Value::Concrete(ConcreteValue::Map(m)));
        }
        attributes.insert(
            "domain_validation_options".to_string(),
            Value::Concrete(ConcreteValue::List(list)),
        );
    }
    // The user-supplied validation_method is preserved from the
    // request side (DescribeCertificate doesn't echo the top-level
    // request validation method); fall through to the option-level
    // method if the certificate has at least one validation entry.
    if !attributes.contains_key("validation_method")
        && let Some(first_opt) = cert.domain_validation_options().first()
        && let Some(method) = first_opt.validation_method()
    {
        attributes.insert(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String(method.as_str().to_string())),
        );
    }
    attributes
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
    arn: &str,
    domain_name: &str,
    resource: &Resource,
    schema: &ResourceSchema,
    cert: Option<&aws_sdk_acm::types::CertificateDetail>,
) -> State {
    let mut attributes = cert
        .map(certificate_detail_to_attributes)
        .unwrap_or_default();

    attributes.insert(
        "certificate_arn".to_string(),
        Value::Concrete(ConcreteValue::String(arn.to_string())),
    );
    attributes
        .entry("domain_name".to_string())
        .or_insert_with(|| Value::Concrete(ConcreteValue::String(domain_name.to_string())));

    attributes.remove(DVO_MISSING_ATTRIBUTE);

    if let Some(validation_method) = optional_enum_attr(resource, schema, "validation_method") {
        attributes
            .entry("validation_method".to_string())
            .or_insert_with(|| {
                Value::Concrete(ConcreteValue::String(validation_method.to_string()))
            });
    }
    if let Some(key_algorithm) = optional_enum_attr(resource, schema, "key_algorithm") {
        attributes
            .entry("key_algorithm".to_string())
            .or_insert_with(|| Value::Concrete(ConcreteValue::String(key_algorithm.to_string())));
    }
    if let Some(value) = resource.get_attr("subject_alternative_names") {
        let sans = extract_string_list(value);
        if !sans.is_empty() {
            attributes
                .entry("subject_alternative_names".to_string())
                .or_insert_with(|| {
                    Value::Concrete(ConcreteValue::List(
                        sans.into_iter()
                            .map(|san| Value::Concrete(ConcreteValue::String(san)))
                            .collect(),
                    ))
                });
        }
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

    State::existing(id.clone(), attributes).with_identifier(arn)
}

fn partial_acm_certificate_create_outcome(
    id: &ResourceId,
    arn: &str,
    domain_name: &str,
    resource: &Resource,
    schema: &ResourceSchema,
    cert: Option<&aws_sdk_acm::types::CertificateDetail>,
    reason: String,
) -> CreateOutcome {
    let state = partial_acm_certificate_state(id, arn, domain_name, resource, schema, cert);
    CreateOutcome::partial_success(state, reason, vec![DVO_MISSING_ATTRIBUTE.to_string()])
}

impl AwsProvider {
    /// Create an ACM certificate by issuing a `RequestCertificate`
    /// call. The returned state carries the cert's ARN as identifier;
    /// the validation status is `PENDING_VALIDATION` until the user
    /// satisfies the DNS / EMAIL challenge — typically wired via a
    /// `wait` construct on `cert.status`.
    pub(crate) async fn create_acm_certificate(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateOutcome> {
        let id = resource.id.clone();
        let domain_name = require_string_attr(resource, "domain_name")?;

        let mut req = self
            .acm_client
            .request_certificate()
            .domain_name(domain_name.clone());

        if let Some(method) = optional_enum_attr(resource, schema, "validation_method")
            .and_then(parse_validation_method)
        {
            req = req.validation_method(method);
        }
        let sans = resource
            .get_attr("subject_alternative_names")
            .map(extract_string_list)
            .unwrap_or_default();
        for san in sans {
            req = req.subject_alternative_names(san);
        }
        if let Some(key_algorithm) = optional_enum_attr(resource, schema, "key_algorithm") {
            // Pass the AWS canonical form through verbatim — schema
            // narrowing has already validated it.
            req = req.key_algorithm(key_algorithm.into());
        }
        if let Some(options) = build_create_certificate_options(resource, schema) {
            req = req.options(options);
        }
        // RequestCertificate accepts tags inline, avoiding a follow-up
        // AddTagsToCertificate round-trip on the create path.
        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = resource.get_attr("tags") {
            for (key, value) in tag_map {
                if let Value::Concrete(ConcreteValue::String(val)) = value {
                    let tag = aws_sdk_acm::types::Tag::builder()
                        .key(key)
                        .value(val)
                        .build()
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message(
                                "Failed to build ACM tag",
                                &e,
                            ))
                            .for_resource(id.clone())
                        })?;
                    req = req.tags(tag);
                }
            }
        }
        // The previous request-side `domain_validation_options`
        // (per-SAN email validation domain override) has been
        // dropped: the codegen schema now models DVO as the
        // *response* shape (with `resource_record`, `validation_status`,
        // ...) rather than the request shape (just `domain_name` +
        // `validation_domain`), so the user cannot set it from DSL
        // anymore. The override was a rare-use email-validation
        // feature; the common DNS-validation path is unaffected.
        // carina-rs/carina-provider-aws#296.

        let output = req.send().await.map_err(|e| {
            api_error_with_meta("RequestCertificate failed", "acm.RequestCertificate", e)
                .for_resource(id.clone())
        })?;

        let arn = output
            .certificate_arn()
            .ok_or_else(|| {
                ProviderError::api_error("RequestCertificate returned no certificate_arn")
                    .for_resource(id.clone())
            })?
            .to_string();

        let validation_method = optional_enum_attr(resource, schema, "validation_method")
            .and_then(parse_validation_method);

        // DNS validation populates DomainValidationOptions[].ResourceRecord
        // asynchronously after RequestCertificate. Wait so the post-create
        // read-back carries the records downstream resources reference.
        // carina-rs/carina-provider-aws#298.
        if matches!(validation_method, Some(ValidationMethod::Dns)) {
            let dvo_outcome = wait_for_dvo_populated(
                &id,
                || async {
                    self.acm_client
                        .describe_certificate()
                        .certificate_arn(arn.as_str())
                        .send()
                        .await
                        .map_err(|e| {
                            api_error_with_meta(
                                "DescribeCertificate failed",
                                "acm.DescribeCertificate",
                                e,
                            )
                            .for_resource(id.clone())
                        })
                        .map(|out| out.certificate().cloned())
                },
                6,
                std::time::Duration::from_secs(5),
            )
            .await?;
            return match dvo_outcome {
                DvoWaitOutcome::Populated(cert) => {
                    let state = self
                        .read_acm_certificate_from_detail(&id, &arn, Some(&cert))
                        .await?;
                    Ok(CreateOutcome::Success { state })
                }
                DvoWaitOutcome::Incomplete {
                    certificate,
                    reason,
                } => Ok(partial_acm_certificate_create_outcome(
                    &id,
                    &arn,
                    &domain_name,
                    resource,
                    schema,
                    certificate.as_ref(),
                    reason,
                )),
            };
        }

        // Read back so the State carries every server-side attribute the
        // user might wait on (`status`, `domain_validation_options`, ...).
        let state = self.read_acm_certificate(&id, Some(&arn)).await?;
        Ok(CreateOutcome::Success { state })
    }

    /// Read an ACM certificate via `DescribeCertificate`.
    pub(crate) async fn read_acm_certificate(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(arn) = identifier else {
            return Ok(State::not_found(id.clone()));
        };
        let output = self
            .acm_client
            .describe_certificate()
            .certificate_arn(arn)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("DescribeCertificate failed", "acm.DescribeCertificate", e)
                    .for_resource(id.clone())
            })?;
        self.read_acm_certificate_from_detail(id, arn, output.certificate())
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
        arn: &str,
        cert: Option<&aws_sdk_acm::types::CertificateDetail>,
    ) -> ProviderResult<State> {
        let Some(cert) = cert else {
            return Ok(State::not_found(id.clone()));
        };

        let mut attributes = certificate_detail_to_attributes(cert);

        // Tags are fetched separately via `ListTagsForCertificate`.
        let tag_output = self
            .acm_client
            .list_tags_for_certificate()
            .certificate_arn(arn)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "ListTagsForCertificate failed",
                    "acm.ListTagsForCertificate",
                    e,
                )
                .for_resource(id.clone())
            })?;
        let tags = tag_output.tags();
        if !tags.is_empty() {
            let mut tag_map = IndexMap::new();
            for tag in tags {
                if let Some(k) = Some(tag.key()) {
                    let v = tag.value().unwrap_or("").to_string();
                    tag_map.insert(k.to_string(), Value::Concrete(ConcreteValue::String(v)));
                }
            }
            attributes.insert(
                "tags".to_string(),
                Value::Concrete(ConcreteValue::Map(tag_map)),
            );
        }

        Ok(State::existing(id.clone(), attributes).with_identifier(arn))
    }

    /// Update an ACM certificate. Only `CertificateTransparencyLoggingPreference`
    /// is mutable in-place via `UpdateCertificateOptions`. Tag changes
    /// flow through `AddTagsToCertificate` / `RemoveTagsFromCertificate`.
    /// Any other field change requires resource replacement.
    pub(crate) async fn update_acm_certificate(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        if let Some(options) = build_update_certificate_options(&to, schema) {
            self.acm_client
                .update_certificate_options()
                .certificate_arn(identifier)
                .options(options)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta(
                        "UpdateCertificateOptions failed",
                        "acm.UpdateCertificateOptions",
                        e,
                    )
                    .for_resource(id.clone())
                })?;
        }
        self.apply_acm_tags(&id, identifier, &to, from).await?;

        self.read_acm_certificate(&id, Some(identifier)).await
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
        to: &Resource,
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
                api_error_with_meta("AddTagsToCertificate failed", "acm.AddTagsToCertificate", e)
                    .for_resource(id.clone())
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
                api_error_with_meta(
                    "RemoveTagsFromCertificate failed",
                    "acm.RemoveTagsFromCertificate",
                    e,
                )
                .for_resource(id.clone())
            })?;
        }

        Ok(())
    }

    /// Delete an ACM certificate via `DeleteCertificate`.
    pub(crate) async fn delete_acm_certificate(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        self.acm_client
            .delete_certificate()
            .certificate_arn(identifier)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("DeleteCertificate failed", "acm.DeleteCertificate", e)
                    .for_resource(id)
            })?;
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
            Value::Concrete(ConcreteValue::String("example.com".to_string())),
        );
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string())),
        );
        resource
    }

    fn assert_partial_outcome_records_arn(
        outcome: CreateOutcome,
        arn: &str,
        expected_reason_parts: &[&str],
    ) {
        let CreateOutcome::PartialSuccess { state, diagnostic } = outcome else {
            panic!("expected partial create outcome");
        };
        assert_eq!(state.identifier.as_deref(), Some(arn));
        assert_eq!(
            state.attributes.get("certificate_arn"),
            Some(&Value::Concrete(ConcreteValue::String(arn.to_string()))),
        );
        assert_eq!(
            state.attributes.get("domain_name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "example.com".to_string()
            ))),
        );
        assert!(
            !state.attributes.contains_key(DVO_MISSING_ATTRIBUTE),
            "partial state must not publish incomplete DVO as complete state",
        );
        assert_eq!(
            diagnostic.missing_attributes(),
            &[DVO_MISSING_ATTRIBUTE.to_string()],
        );
        for part in expected_reason_parts {
            assert!(
                diagnostic.reason().contains(part),
                "diagnostic reason must contain {part:?}; got: {}",
                diagnostic.reason(),
            );
        }
    }

    /// carina#3061: the certificate ARN must be published under the
    /// schema attribute name `certificate_arn` (see
    /// `schemas/generated/acm/certificate.rs` —
    /// `AttributeSchema::new("certificate_arn", ...)`), NOT `arn`.
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

        let attrs = certificate_detail_to_attributes(&cert);

        assert_eq!(
            attrs.get("certificate_arn"),
            Some(&Value::Concrete(ConcreteValue::String(
                "arn:aws:acm:us-east-1:111:certificate/abc".to_string()
            ))),
            "ACM read must publish the ARN under `certificate_arn` to \
             match the schema attribute the DSL references"
        );
        assert!(
            !attrs.contains_key("arn"),
            "the legacy `arn` key matches no schema attribute and must \
             not be emitted (carina#3061)"
        );
        // status must still be published (the wait predicate relies on
        // it; this guards against a regression that breaks the wait).
        assert_eq!(
            attrs.get("status"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ISSUED".to_string()
            )))
        );
    }

    #[test]
    fn read_normalizes_key_algorithm_response_spelling_to_schema_enum() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .key_algorithm(aws_sdk_acm::types::KeyAlgorithm::from("RSA-2048"))
            .build();

        let attrs = certificate_detail_to_attributes(&cert);
        let state = crate::helpers::normalize_read_state_enum_values(
            &acm_schema(),
            State::existing(id(), attrs),
        );

        assert_eq!(
            state.attributes.get("key_algorithm"),
            Some(&Value::Concrete(ConcreteValue::String(
                "RSA_2048".to_string()
            ))),
            "ACM DescribeCertificate returns RSA-2048, but state must use \
             the schema enum value RSA_2048"
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

        let attrs = certificate_detail_to_attributes(&cert);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = attrs.get("options") else {
            panic!("expected nested options map, got {attrs:?}");
        };
        assert_eq!(
            options.get("certificate_transparency_logging_preference"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ENABLED".to_string()
            )))
        );
    }

    #[test]
    fn certificate_detail_to_attributes_emits_nested_options_export() {
        let cert = CertificateDetail::builder()
            .domain_name("registry.example.com")
            .options(
                CertificateOptions::builder()
                    .export(CertificateExport::Enabled)
                    .build(),
            )
            .build();

        let attrs = certificate_detail_to_attributes(&cert);

        let Some(Value::Concrete(ConcreteValue::Map(options))) = attrs.get("options") else {
            panic!("expected nested options map, got {attrs:?}");
        };
        assert_eq!(
            options.get("export"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ENABLED".to_string()
            )))
        );
    }

    #[test]
    fn create_acm_certificate_passes_options_export_to_sdk_request() {
        let mut options = IndexMap::new();
        options.insert(
            "export".to_string(),
            Value::Concrete(ConcreteValue::String("ENABLED".to_string())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options =
            build_create_certificate_options(&resource, &acm_schema()).expect("options");
        assert_eq!(request_options.export(), Some(&CertificateExport::Enabled));
    }

    #[test]
    fn create_acm_certificate_passes_both_options_fields() {
        let mut options = IndexMap::new();
        options.insert(
            "certificate_transparency_logging_preference".to_string(),
            Value::Concrete(ConcreteValue::String("DISABLED".to_string())),
        );
        options.insert(
            "export".to_string(),
            Value::Concrete(ConcreteValue::String("ENABLED".to_string())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options =
            build_create_certificate_options(&resource, &acm_schema()).expect("options");
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
            Value::Concrete(ConcreteValue::String("DISABLED".to_string())),
        );
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "options".to_string(),
            Value::Concrete(ConcreteValue::Map(options)),
        );

        let request_options =
            build_update_certificate_options(&resource, &acm_schema()).expect("options");
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
        let outcome = partial_acm_certificate_create_outcome(
            &id(),
            arn,
            "example.com",
            &resource,
            &acm_schema(),
            Some(&cert),
            reason,
        );
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
        let outcome = partial_acm_certificate_create_outcome(
            &id(),
            arn,
            "example.com",
            &resource,
            &acm_schema(),
            Some(&cert),
            reason,
        );
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
                Value::Concrete(ConcreteValue::String((*v).to_string())),
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

    /// carina-provider-aws#440: `validation_method = dns` was reaching AWS
    /// as `EMAIL` (default) because the host canonicalize pass wraps the
    /// schema-typed enum string as `CanonicalEnum("DNS")` before it crosses
    /// the WIT boundary. Inside the provider, `apply_desired_normalization`
    /// runs `canonicalize_resources_with_schemas` again, so by the time
    /// `create_acm_certificate` reads `validation_method`, the value is
    /// `ConcreteValue::CanonicalEnum`, not `ConcreteValue::String`.
    ///
    /// The old local string extractor matched only `ConcreteValue::String`, so the
    /// `validation_method(...)` call was silently skipped on the SDK
    /// request. AWS defaulted to `EMAIL`, the read-back persisted that
    /// to state, and every subsequent plan showed a permanent
    /// `EMAIL → DNS (forces replacement)` diff. The replacement-created
    /// cert hit the same bug, so the loop never converged.
    ///
    /// The fix is at the helper layer: a request-side extractor for an
    /// enum-typed attribute must accept the three variants the
    /// canonicalize pipeline can produce — `String` (e.g. via a quoted
    /// DSL spelling `validation_method = 'DNS'`), `EnumIdentifier`
    /// (raw, when canonicalize didn't resolve), and `CanonicalEnum`
    /// (the typed witness the canonicalize pass produces for schema-known
    /// values). This test pins all three.
    ///
    /// This assertion goes through the production
    /// `helpers::optional_enum_attr` path so helper regressions are caught here.
    #[test]
    fn validation_method_canonical_enum_reaches_request() {
        use carina_core::schema::{AttributeType, Schema, enum_identity};

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
        let schema = Schema::flat(attr_type);

        // DSL-side enum identifier `validation_method = dns` reaches the
        // schema-typed canonicalize pass as `EnumIdentifier("dns")`.
        let canonical = schema.canonicalize(Value::Concrete(ConcreteValue::enum_identifier("dns")));
        assert!(
            matches!(&canonical, Value::Concrete(ConcreteValue::CanonicalEnum(c))
                if c.api_value() == "DNS"),
            "schema canonicalize must produce CanonicalEnum(DNS) from \
             EnumIdentifier(\"dns\"); got {canonical:?}"
        );

        // The fix: extracting the validation method from a Resource whose
        // `validation_method` is a `CanonicalEnum` must yield
        // `ValidationMethod::Dns`. Pre-fix the extract→parse chain
        // silently returned None.
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr("validation_method".to_string(), canonical);
        let schema = acm_schema();
        let parsed = crate::helpers::optional_enum_attr(&resource, &schema, "validation_method")
            .and_then(parse_validation_method);
        assert_eq!(
            parsed,
            Some(ValidationMethod::Dns),
            "validation_method CanonicalEnum(DNS) must extract to \
             ValidationMethod::Dns; pre-fix the silent drop caused AWS to \
             default to EMAIL (issue #440)"
        );
    }

    /// Sibling shape: post-#3463 raw enum identifier carrying the bare
    /// DSL alias spelling (`dns`) may reach the provider without
    /// canonicalization when host-side resolution fails for any reason.
    /// The extract→parse chain must still recover the validation method
    /// rather than silently dropping it.
    ///
    /// This assertion goes through the production
    /// `helpers::optional_enum_attr` path so helper regressions are caught here.
    #[test]
    fn validation_method_enum_identifier_reaches_request() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::enum_identifier("dns")),
        );
        let schema = acm_schema();
        let parsed = crate::helpers::optional_enum_attr(&resource, &schema, "validation_method")
            .and_then(parse_validation_method);
        assert_eq!(
            parsed,
            Some(ValidationMethod::Dns),
            "validation_method EnumIdentifier(\"dns\") must extract to \
             ValidationMethod::Dns; the parser-emitted enum identifier \
             shape must not be silently dropped"
        );
    }

    /// Quoted-string DSL form `validation_method = 'DNS'` continues to
    /// work — pin it so the enum-variant additions above can't regress
    /// the documented example in `examples/acm_certificate/main.crn`.
    ///
    /// This assertion goes through the production
    /// `helpers::optional_enum_attr` path so helper regressions are caught here.
    #[test]
    fn validation_method_string_still_reaches_request() {
        let mut resource = Resource::with_provider("aws", "acm.Certificate", "test-cert", None);
        resource.set_attr(
            "validation_method".to_string(),
            Value::Concrete(ConcreteValue::String("DNS".to_string())),
        );
        let schema = acm_schema();
        let parsed = crate::helpers::optional_enum_attr(&resource, &schema, "validation_method")
            .and_then(parse_validation_method);
        assert_eq!(parsed, Some(ValidationMethod::Dns));
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
            Value::Concrete(ConcreteValue::String("dev".to_string())),
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
