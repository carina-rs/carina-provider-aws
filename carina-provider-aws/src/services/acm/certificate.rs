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
            ProviderError::api_error(sdk_error_message("RequestCertificate failed", &e))
                .for_resource(id.clone())
        })?;

        let arn = output.certificate_arn().ok_or_else(|| {
            ProviderError::api_error("RequestCertificate returned no certificate_arn")
                .for_resource(id.clone())
        })?;

        let validation_method = resource
            .get_attr("validation_method")
            .and_then(extract_string)
            .and_then(parse_validation_method);

        // DNS validation populates DomainValidationOptions[].ResourceRecord
        // asynchronously after RequestCertificate. Wait so the post-create
        // read-back carries the records downstream resources reference.
        // carina-rs/carina-provider-aws#298.
        if matches!(validation_method, Some(ValidationMethod::Dns)) {
            let cert = wait_for_dvo_populated(
                &id,
                || async {
                    self.acm_client
                        .describe_certificate()
                        .certificate_arn(arn)
                        .send()
                        .await
                        .map_err(|e| {
                            ProviderError::api_error(sdk_error_message(
                                "DescribeCertificate failed",
                                &e,
                            ))
                            .for_resource(id.clone())
                        })
                        .map(|out| out.certificate().cloned())
                },
                6,
                std::time::Duration::from_secs(5),
            )
            .await?;
            return self
                .read_acm_certificate_from_detail(&id, arn, Some(&cert))
                .await;
        }

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
        from: &State,
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
                ProviderError::api_error(sdk_error_message("AddTagsToCertificate failed", &e))
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
                ProviderError::api_error(sdk_error_message("RemoveTagsFromCertificate failed", &e))
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
                ProviderError::api_error(sdk_error_message("DeleteCertificate failed", &e))
                    .for_resource(id)
            })?;
        Ok(())
    }
}

/// Poll a fetch closure that returns an ACM `CertificateDetail`
/// until every `domain_validation_options[]` entry has its
/// `resource_record` populated, or the attempt budget is exhausted.
/// Returns the last fetched `CertificateDetail` so callers can build
/// state from it without a redundant `DescribeCertificate`.
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
/// last). On timeout, returns a `ProviderError::timeout` naming the
/// resource so the operator can decide between retry and an AWS
/// support escalation.
pub(crate) async fn wait_for_dvo_populated<F, Fut>(
    id: &ResourceId,
    mut fetch: F,
    max_attempts: u32,
    interval: std::time::Duration,
) -> ProviderResult<aws_sdk_acm::types::CertificateDetail>
where
    F: FnMut() -> Fut,
    Fut:
        std::future::Future<Output = ProviderResult<Option<aws_sdk_acm::types::CertificateDetail>>>,
{
    for attempt in 0..max_attempts {
        if let Some(cert) = fetch().await? {
            let dvs = cert.domain_validation_options();
            if !dvs.is_empty() && dvs.iter().all(|dv| dv.resource_record().is_some()) {
                return Ok(cert);
            }
        }
        if attempt + 1 < max_attempts {
            tokio::time::sleep(interval).await;
        }
    }
    Err(ProviderError::timeout(format!(
        "ACM did not populate domain_validation_options[].resource_record within \
         {} attempts (~{:?}); the certificate exists but cannot be referenced by \
         downstream resources yet. Retry the apply, or check the ACM console \
         for the cert ARN.",
        max_attempts,
        interval.saturating_mul(max_attempts),
    ))
    .for_resource(id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_acm::types::{CertificateDetail, DomainValidation, RecordType, ResourceRecord};
    use std::sync::Mutex;
    use std::time::Duration;

    fn id() -> ResourceId {
        ResourceId::with_provider("aws", "acm.Certificate", "test-cert")
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
        let cert = result.expect("expected Ok");
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
        let cert = result.expect("expected Ok");
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
        let cert = result.expect("expected Ok");
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(cert.domain_validation_options().len(), 2);
        assert!(
            cert.domain_validation_options()
                .iter()
                .all(|dv| dv.resource_record().is_some()),
        );
    }

    #[tokio::test]
    async fn wait_for_dvo_times_out_after_max_attempts() {
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
        let err = result.expect_err("expected timeout error");
        assert_eq!(
            *calls.lock().unwrap(),
            4,
            "fetched exactly max_attempts times"
        );
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("domain_validation_options"),
            "error message must name the attribute; got: {msg}",
        );
        assert!(
            msg.contains("4 attempts"),
            "error must report the attempt budget; got: {msg}",
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
        let cert = result.expect("expected Ok");
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
