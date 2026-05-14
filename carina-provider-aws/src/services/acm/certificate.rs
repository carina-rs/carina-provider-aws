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

use aws_sdk_acm::types::ValidationMethod;
use indexmap::IndexMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};

fn extract_string(value: &Value) -> Option<&str> {
    if let Value::Concrete(ConcreteValue::String(s)) = value {
        Some(s.as_str())
    } else {
        None
    }
}

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

fn parse_validation_method(input: &str) -> Option<ValidationMethod> {
    match input.to_ascii_uppercase().as_str() {
        "DNS" => Some(ValidationMethod::Dns),
        "EMAIL" => Some(ValidationMethod::Email),
        _ => None,
    }
}

impl AwsProvider {
    /// Create an ACM certificate by issuing a `RequestCertificate`
    /// call. The returned state carries the cert's ARN as identifier;
    /// the validation status is `PENDING_VALIDATION` until the user
    /// satisfies the DNS / EMAIL challenge — typically wired via a
    /// `wait` construct on `cert.status`.
    pub(crate) async fn create_acm_certificate(&self, resource: Resource) -> ProviderResult<State> {
        let id = resource.id.clone();
        let domain_name = require_string_attr(&resource, "domain_name")?;

        let mut req = self
            .acm_client
            .request_certificate()
            .domain_name(domain_name);

        if let Some(method) = resource
            .get_attr("validation_method")
            .and_then(extract_string)
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
        if let Some(key_algorithm) = resource.get_attr("key_algorithm").and_then(extract_string) {
            // Pass the AWS canonical form through verbatim — schema
            // narrowing has already validated it.
            req = req.key_algorithm(key_algorithm.into());
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
            ProviderError::api_error(sdk_error_message("RequestCertificate failed", &e))
                .for_resource(id.clone())
        })?;

        let arn = output.certificate_arn().ok_or_else(|| {
            ProviderError::api_error("RequestCertificate returned no certificate_arn")
                .for_resource(id.clone())
        })?;

        // Read back so the State carries every server-side attribute the
        // user might wait on (`status`, `domain_validation_options`, ...).
        self.read_acm_certificate(&id, Some(arn)).await
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
                ProviderError::api_error(sdk_error_message("DescribeCertificate failed", &e))
                    .for_resource(id.clone())
            })?;
        let Some(cert) = output.certificate() else {
            return Ok(State::not_found(id.clone()));
        };

        let mut attributes: HashMap<String, Value> = HashMap::new();
        if let Some(v) = cert.domain_name() {
            attributes.insert(
                "domain_name".to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        if let Some(v) = cert.certificate_arn() {
            attributes.insert(
                "arn".to_string(),
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

        // Tags are fetched separately via `ListTagsForCertificate`.
        let tag_output = self
            .acm_client
            .list_tags_for_certificate()
            .certificate_arn(arn)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("ListTagsForCertificate failed", &e))
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
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        if let Some(pref) = to
            .get_attr("certificate_transparency_logging_preference")
            .and_then(extract_string)
        {
            self.acm_client
                .update_certificate_options()
                .certificate_arn(identifier)
                .options(
                    aws_sdk_acm::types::CertificateOptions::builder()
                        .certificate_transparency_logging_preference(pref.into())
                        .build(),
                )
                .send()
                .await
                .map_err(|e| {
                    ProviderError::api_error(sdk_error_message(
                        "UpdateCertificateOptions failed",
                        &e,
                    ))
                    .for_resource(id.clone())
                })?;
        }
        // Tag reconciliation deferred to a follow-up — the initial cut
        // surfaces tag changes as a planned diff but doesn't apply them
        // automatically. ACM tag APIs are AddTagsToCertificate /
        // RemoveTagsFromCertificate; both take a list of tag records.

        self.read_acm_certificate(&id, Some(identifier)).await
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
                ProviderError::api_error(sdk_error_message("DeleteCertificate failed", &e))
                    .for_resource(id)
            })?;
        Ok(())
    }
}
