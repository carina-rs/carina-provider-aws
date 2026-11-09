//! Route 53 RecordSet service implementation.
//!
//! Uses ChangeResourceRecordSets (UPSERT/DELETE) and ListResourceRecordSets
//! for CRUD operations. Cloud Control does not support Route 53 records.

use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use carina_aws_types::sdk::route53::types::{
    AliasTarget, Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType,
};

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, ConcreteValue, NoPlaceholder, Resource, ResourceId, Value,
};
use carina_core::schema::CompiledResourceSchema;
use carina_plugin_sdk::{DesiredInputs, DesiredScalar, DesiredValue, DesiredValueMut, DesiredView};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::Route53RecordSetReadModelSupplement;
use crate::resend_safety_generated::Route53Client;
use crate::send_models_generated::{Route53RecordSetCreateInput, Route53RecordSetUpdateInput};

/// Composite identifier format: `hosted_zone_id|name|type`
fn make_identifier(hosted_zone_id: &str, name: &str, record_type: &str) -> String {
    format!("{}|{}|{}", hosted_zone_id, name, record_type)
}

fn parse_identifier(identifier: &str) -> Option<(&str, &str, &str)> {
    let mut parts = identifier.splitn(3, '|');
    let zone_id = parts.next()?;
    let name = parts.next()?;
    let record_type = parts.next()?;
    Some((zone_id, name, record_type))
}

/// Normalize a DNS name by ensuring it has a trailing dot (Route 53 convention).
fn normalize_dns_name(name: &str) -> String {
    if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{}.", name)
    }
}

/// Main's dot-less stored spelling, shared by desired inputs and read state.
///
/// Route 53 returns a root dot, lowercase letters, and octal escapes for
/// punctuation (including `\052` for `*`). Keep escaped label dots and
/// backslashes escaped so normalization cannot change label boundaries or
/// reinterpret an escaped backslash on a later pass.
/// https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/DomainNameFormat.html
// AWS: T4.Carina-588-Probe2.Example reads as t4.carina-588-probe2.example.; strip the dot for main's stored form.
fn canonical_dns_name(name: &str) -> String {
    let bytes = name.strip_suffix('.').unwrap_or(name).as_bytes();
    let mut canonical = String::with_capacity(bytes.len());
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'.' {
            canonical.push('.');
            offset += 1;
            continue;
        }
        let byte = if bytes[offset] == b'\\' {
            let Some(byte) = octal_byte(&bytes[offset + 1..]) else {
                // Invalid escapes belong to validation; never reinterpret
                // their following characters as an encoded octet.
                canonical.push('\\');
                offset += 1;
                continue;
            };
            offset += 4;
            byte
        } else {
            let byte = bytes[offset];
            offset += 1;
            byte
        }
        .to_ascii_lowercase();
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            canonical.push(char::from(byte));
        } else {
            canonical.push('\\');
            canonical.push(char::from(b'0' + byte / 64));
            canonical.push(char::from(b'0' + (byte / 8) % 8));
            canonical.push(char::from(b'0' + byte % 8));
        }
    }
    canonical
}

/// Outcome of scanning one `ListResourceRecordSets` page for an exact
/// `(name, type)` record. See [`scan_page_for_record`].
enum PageScan<'a> {
    /// Exact `(normalized name, type)` match found on this page.
    Found(&'a ResourceRecordSet),
    /// A record whose name differs from the target appeared. Because the
    /// listing is cursor-started at the target name and ordered by name,
    /// a different name means the target name's records are exhausted —
    /// the record does not exist; stop without paging further.
    StopNotFound,
    /// Every record on this page is still at the target name but none
    /// matched the type yet (the target name's records span a page
    /// boundary). Follow pagination and scan the next page.
    KeepPaging,
}

/// Scan one page of `list_resource_record_sets` results for the record
/// matching `target_name` (already trailing-dot-normalized) and
/// `target_type`.
///
/// `start_record_name` + `start_record_type` are pagination *cursors*,
/// not filters: AWS returns records at or after that position, ordered
/// by name then by a fixed per-name type rank. So a single record (the
/// old `max_items(1)` approach) is only the target when the target type
/// happens to sort first at its name; otherwise the page leads with a
/// different-typed (or, across a boundary, same-typed-different) record.
/// This walks every record on the page and classifies the page so the
/// caller can match, page on, or stop early without scanning the whole
/// zone (carina-rs/carina-provider-aws#333).
///
/// NOTE: the match is `(name, type)` only — the carina resource identity
/// (`zone|name|type`, see `make_identifier`). Routing-policy record sets
/// (weighted/latency/geo/failover) have multiple records with the same
/// `(name, type)` distinguished by `SetIdentifier`; those are not
/// modeled and the first such record is returned (aws#335).
fn scan_page_for_record<'a>(
    records: &'a [ResourceRecordSet],
    target_name: &str,
    target_type: &str,
) -> PageScan<'a> {
    let target_name = canonical_dns_name(target_name);
    for rrs in records {
        if canonical_dns_name(rrs.name()) != target_name {
            return PageScan::StopNotFound;
        }
        if rrs.r#type().as_str() == target_type {
            return PageScan::Found(rrs);
        }
    }
    PageScan::KeepPaging
}

fn extract_string(value: &Value<impl carina_core::resource::ValueKind>) -> Option<&str> {
    if let Value::Concrete(ConcreteValue::String(s)) = value {
        Some(s.expose_plaintext())
    } else {
        None
    }
}

fn build_resource_records(
    records: &[String],
    id: &ResourceId,
) -> ProviderResult<Vec<ResourceRecord>> {
    records
        .iter()
        .map(|value| {
            ResourceRecord::builder().value(value).build().map_err(|e| {
                ProviderError::api_error_caused_by("Invalid resource record", e)
                    .for_resource(id.clone())
            })
        })
        .collect()
}

fn build_alias_target_from_map(
    alias: &IndexMap<String, Value<impl carina_core::resource::ValueKind>>,
    id: &ResourceId,
) -> ProviderResult<AliasTarget> {
    let dns_name = alias
        .get("dns_name")
        .and_then(extract_string)
        .unwrap_or_default();
    let zone_id = alias
        .get("hosted_zone_id")
        .and_then(extract_string)
        .unwrap_or_default();
    let evaluate = alias
        .get("evaluate_target_health")
        .and_then(|v| {
            if let Value::Concrete(ConcreteValue::Bool(b)) = v {
                Some(*b)
            } else {
                None
            }
        })
        .unwrap_or(false);

    AliasTarget::builder()
        .dns_name(dns_name)
        .hosted_zone_id(zone_id)
        .evaluate_target_health(evaluate)
        .build()
        .map_err(|e| {
            ProviderError::api_error_caused_by("Invalid alias_target", e).for_resource(id.clone())
        })
}

/// Build an AWS SDK ResourceRecordSet from carina resource attributes.
fn build_record_set(
    id: &ResourceId,
    alias_target: Option<&Value>,
    name: &str,
    resource_records: Option<&[String]>,
    ttl: Option<i64>,
    record_type: &str,
) -> ProviderResult<ResourceRecordSet> {
    // Update's create-only name/type come from the applied identifier. Recheck
    // the outgoing values after deferred inputs have resolved.
    validate_dns_name(name).map_err(|reason| record_input_error(id, "name", reason))?;
    if record_type == "NS" && dns_name_is_wildcard(name) {
        return Err(record_input_error(
            id,
            "name",
            "NS records cannot use a wildcard",
        ));
    }
    if let Some(records) = resource_records {
        validate_record_count(id, Some(record_type), records.len())?;
        for (index, value) in records.iter().enumerate() {
            validate_record_value(id, Some(record_type), index, value)?;
        }
    }
    if let Some(Value::Concrete(ConcreteValue::Map(alias))) = alias_target
        && let Some(dns_name) = alias.get("dns_name").and_then(extract_string)
    {
        validate_dns_name(dns_name)
            .map_err(|reason| record_input_error(id, "alias_target.dns_name", reason))?;
    }
    let mut builder = ResourceRecordSet::builder()
        .name(normalize_dns_name(name))
        .r#type(RrType::from(record_type));

    if let Some(ttl) = ttl {
        builder = builder.ttl(ttl);
    }

    if let Some(records) = resource_records {
        builder = builder.set_resource_records(Some(build_resource_records(records, id)?));
    }

    if let Some(Value::Concrete(ConcreteValue::Map(alias))) = alias_target {
        builder = builder.alias_target(build_alias_target_from_map(alias, id)?);
    }

    builder.build().map_err(|e| {
        ProviderError::api_error_caused_by("Failed to build ResourceRecordSet", e)
            .for_resource(id.clone())
    })
}

fn record_input_error(id: &ResourceId, path: &str, reason: &str) -> ProviderError {
    // Values can contain sensitive DNS data. Report the path and violated rule.
    ProviderError::invalid_input(format!("route53.RecordSet attribute {path}: {reason}"))
        .for_resource(id.clone())
}

fn octal_byte(digits: &[u8]) -> Option<u8> {
    match digits {
        [a @ b'0'..=b'3', b @ b'0'..=b'7', c @ b'0'..=b'7', ..] => {
            Some((a - b'0') * 64 + (b - b'0') * 8 + (c - b'0'))
        }
        _ => None,
    }
}

fn dns_name_is_wildcard(value: &str) -> bool {
    value == "*" || value.starts_with("*.") || value == r"\052" || value.starts_with(r"\052.")
}

// DNSName's 1024-character Smithy carrier is deliberately wider than the DNS
// wire name: one length octet per label plus the final zero root octet.
// Real AWS (carina-test-004, 2026-11-05) accepts 253 decoded characters without
// a trailing dot / 254 including that dot, and rejects a 254-character name
// without the dot. Escaped dots remain label data, not label delimiters.
// https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/DomainNameFormat.html
// https://www.rfc-editor.org/rfc/rfc1035.html#section-2.3.4
fn validate_dns_name(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > 1024 {
        return Err("DNS name must contain 1..1024 characters");
    }
    let bytes = value.as_bytes();
    let mut offset = 0;
    let mut wire_length = 1; // The terminating root octet is always present.
    let mut label_length = 0;
    let mut first_label = true;
    let mut first_has_star = false;
    while offset < bytes.len() {
        if bytes[offset] == b'.' {
            if label_length == 0 {
                return Err("DNS labels must be nonempty");
            }
            if first_label && first_has_star && label_length != 1 {
                return Err("a wildcard must replace the entire leftmost label");
            }
            first_label = false;
            label_length = 0;
            offset += 1;
        } else {
            let byte = if bytes[offset] == b'\\' {
                let byte = octal_byte(&bytes[offset + 1..])
                    .ok_or("DNS escapes must be three octal digits in 000..377")?;
                offset += 4;
                byte
            } else {
                let byte = bytes[offset];
                if !(b'!'..=b'~').contains(&byte) {
                    return Err("DNS names require printable ASCII or octal escapes");
                }
                offset += 1;
                byte
            };
            first_has_star |= first_label && byte == b'*';
            if label_length == 0 {
                wire_length += 1; // This label's length octet.
            }
            label_length += 1;
            wire_length += 1;
            if label_length > 63 {
                return Err("DNS labels must not exceed 63 decoded bytes");
            }
        }
        if wire_length > 255 {
            return Err(
                "DNS name must not exceed 255 wire bytes, including label lengths and root",
            );
        }
    }
    if first_label && first_has_star && label_length != 1 {
        return Err("a wildcard must replace the entire leftmost label");
    }
    Ok(())
}

// Each repetition consumes one decoded ASCII character or a complete escape,
// so {0,255} bounds decoded chunk length. AWS preserves adjacent chunks and
// whitespace verbatim (carina-test-004); at least one quoted chunk is required.
// https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/ResourceRecordTypes.html#TXTFormat
fn validate_txt_record(value: &str) -> Result<(), &'static str> {
    static TXT_RECORD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"\A[\t\n\x0C\r ]*(?:"(?:[\x20-\x21\x23-\x5B\x5D-\x7E]|\\(?:["\\]|[0-3][0-7]{2})){0,255}"[\t\n\x0C\r ]*)+\z"#)
            .expect("valid TXT record regex")
    });
    TXT_RECORD.is_match(value).then_some(()).ok_or(
        "TXT requires quoted chunks of at most 255 decoded ASCII characters or valid escapes",
    )
}

fn validate_record_count(
    id: &ResourceId,
    record_type: Option<&str>,
    length: usize,
) -> ProviderResult<()> {
    // ResourceRecords min 1 is in route53.json; the single-value rule is in
    // https://docs.aws.amazon.com/Route53/latest/APIReference/API_ResourceRecord.html
    if length == 0 {
        return Err(record_input_error(
            id,
            "resource_records",
            "must contain at least one value",
        ));
    }
    if matches!(record_type, Some("CNAME" | "SOA")) && length != 1 {
        return Err(record_input_error(
            id,
            "resource_records",
            "CNAME and SOA require exactly one value",
        ));
    }
    Ok(())
}

fn validate_record_value(
    id: &ResourceId,
    record_type: Option<&str>,
    index: usize,
    value: &str,
) -> ProviderResult<()> {
    let path = format!("resource_records[{index}]");
    if value.chars().count() > 4000 {
        return Err(record_input_error(
            id,
            &path,
            "must not exceed 4000 characters",
        ));
    }
    // Complex MX/SRV/CAA/DS/HTTPS/NAPTR/SOA/SPF/SSHFP/SVCB/TLSA grammars
    // retain their bounded carrier. Only documented, inexpensive checks:
    // https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/ResourceRecordTypes.html
    let result = match record_type {
        Some("A") => value
            .parse::<std::net::Ipv4Addr>()
            .map(|_| ())
            .map_err(|_| "A requires an IPv4 address"),
        Some("AAAA") => value
            .parse::<std::net::Ipv6Addr>()
            .map(|_| ())
            .map_err(|_| "AAAA requires an IPv6 address"),
        // AWS probe carina-test-004: CNAME/PTR "." values read back unchanged.
        Some("CNAME" | "PTR") if value == "." => Ok(()),
        Some("CNAME" | "NS" | "PTR") => validate_dns_name(value),
        Some("TXT") => validate_txt_record(value),
        _ => Ok(()),
    };
    result.map_err(|reason| record_input_error(id, &path, reason))
}

fn known_string(value: Option<&DesiredValue>) -> Option<&str> {
    match value {
        Some(DesiredValue::Known(scalar)) => scalar.as_str(),
        _ => None,
    }
}

pub(crate) fn validate_record_set_inputs(inputs: &DesiredInputs<'_>) -> ProviderResult<()> {
    let id = inputs.id();
    let record_type = known_string(inputs.attribute("type"));
    if let Some(name) = known_string(inputs.attribute("name")) {
        validate_dns_name(name).map_err(|reason| record_input_error(id, "name", reason))?;
        if record_type == Some("NS") && dns_name_is_wildcard(name) {
            return Err(record_input_error(
                id,
                "name",
                "NS records cannot use a wildcard",
            ));
        }
    }
    if let Some(DesiredValue::Map(alias)) = inputs.attribute("alias_target")
        && let Some(name) = known_string(alias.get("dns_name"))
    {
        validate_dns_name(name)
            .map_err(|reason| record_input_error(id, "alias_target.dns_name", reason))?;
    }
    if let Some(DesiredValue::List(records)) = inputs.attribute("resource_records") {
        validate_record_count(id, record_type, records.iter().count())?;
        for (index, value) in records.iter().enumerate() {
            if let Some(value) = known_string(Some(value)) {
                validate_record_value(id, record_type, index, value)?;
            }
        }
    }
    Ok(())
}

/// Execute a ChangeResourceRecordSets UPSERT or DELETE.
async fn change_record_set(
    client: &Route53Client,
    hosted_zone_id: &str,
    action: ChangeAction,
    record_set: ResourceRecordSet,
    id: &ResourceId,
) -> ProviderResult<()> {
    let change = Change::builder()
        .action(action)
        .resource_record_set(record_set)
        .build()
        .map_err(|e| {
            ProviderError::api_error_caused_by("Failed to build change", e).for_resource(id.clone())
        })?;

    let batch = ChangeBatch::builder()
        .changes(change)
        .build()
        .map_err(|e| {
            ProviderError::api_error_caused_by("Failed to build change batch", e)
                .for_resource(id.clone())
        })?;

    client
        .change_resource_record_sets()
        .hosted_zone_id(hosted_zone_id)
        .change_batch(batch)
        .send()
        .await
        .map_err(|e| {
            api_error_with_meta("ChangeResourceRecordSets failed", e).for_resource(id.clone())
        })?;

    Ok(())
}

fn resource_records_value(rrs: &ResourceRecordSet) -> Option<Value> {
    let records: Vec<Value> = rrs
        .resource_records()
        .iter()
        .map(|r| Value::Concrete(ConcreteValue::String(r.value().to_string().into())))
        .collect();
    (!records.is_empty()).then(|| Value::Concrete(ConcreteValue::List(records)))
}

fn alias_target_value(rrs: &ResourceRecordSet) -> Option<Value> {
    rrs.alias_target().map(|alias| {
        let mut alias_map: IndexMap<String, Value> = IndexMap::new();
        alias_map.insert(
            "dns_name".to_string(),
            Value::Concrete(ConcreteValue::String(
                canonical_dns_name(alias.dns_name()).into(),
            )),
        );
        alias_map.insert(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String(
                alias.hosted_zone_id().to_string().into(),
            )),
        );
        alias_map.insert(
            "evaluate_target_health".to_string(),
            Value::Concrete(ConcreteValue::Bool(alias.evaluate_target_health())),
        );
        Value::Concrete(ConcreteValue::Map(alias_map))
    })
}

impl AwsProvider {
    pub(crate) async fn read_route53_record_set(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();
        let Some((zone_id, name, record_type)) = parse_identifier(identifier) else {
            return Ok(ReadOutcome::not_found(id.clone()));
        };

        let normalized_name = normalize_dns_name(name);

        // `start_record_*` are pagination cursors, not filters: AWS
        // returns records at or after that (name, type) position, ordered
        // by name then per-name type rank. Page from the cursor and scan
        // for the exact (name, type); stop as soon as the listing moves
        // past the target name. The previous `max_items(1)` approach only
        // worked when the target type sorted first at its name and
        // silently returned `not_found` otherwise (aws#333).
        let mut start_name = Some(normalized_name.clone());
        let mut start_type = Some(RrType::from(record_type));
        let mut start_identifier: Option<String> = None;

        loop {
            let mut request = self
                .route53_client
                .list_resource_record_sets()
                .hosted_zone_id(zone_id)
                .set_start_record_name(start_name.take())
                .set_start_record_type(start_type.take());
            if let Some(sid) = start_identifier.take() {
                request = request.start_record_identifier(sid);
            }

            let result = request.send().await.map_err(|e| {
                api_error_with_meta("Failed to list record sets", e).for_resource(id.clone())
            })?;

            match scan_page_for_record(result.resource_record_sets(), &normalized_name, record_type)
            {
                PageScan::Found(rrs) => {
                    let fields =
                        Self::extract_route53_record_set_attributes(rrs).map_name(|name| {
                            name.map(|value| match value {
                                Value::Concrete(ConcreteValue::String(name)) => {
                                    Value::Concrete(ConcreteValue::String(
                                        canonical_dns_name(name.expose_plaintext()).into(),
                                    ))
                                }
                                value => value,
                            })
                        });
                    let (_, model) = fields.into_model(
                        id,
                        Route53RecordSetReadModelSupplement {
                            alias_target: alias_target_value(rrs),
                            resource_records: resource_records_value(rrs),
                            // HostedZoneId is the request target, not a member of
                            // ResourceRecordSet.
                            hosted_zone_id: Value::Concrete(ConcreteValue::String(
                                zone_id.to_string().into(),
                            )),
                        },
                    )?;
                    return found_managed_read(id, applied_identifier, model);
                }
                PageScan::StopNotFound => return Ok(ReadOutcome::not_found(id.clone())),
                PageScan::KeepPaging => {}
            }

            // Only a `next_record_name` lets the scan continue from where
            // it left off. Treating "no next cursor" as not-found (rather
            // than re-sending with `start_record_name = None`, which would
            // silently restart the listing from the beginning of the zone)
            // keeps the loop bounded to the target name's records.
            match result.next_record_name() {
                Some(next_name) if result.is_truncated() => {
                    start_name = Some(next_name.to_string());
                    start_type = result.next_record_type().cloned();
                    start_identifier = result.next_record_identifier().map(str::to_string);
                }
                _ => return Ok(ReadOutcome::not_found(id.clone())),
            }
        }
    }

    pub(crate) async fn create_route53_record_set(
        &self,
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = Route53RecordSetCreateInput::decode(resource, _schema)?;
        let sent = input.sent();
        let crate::send_models_generated::Route53RecordSetCreateParts {
            alias_target,
            hosted_zone_id,
            name,
            resource_records,
            ttl,
            r#type: record_type,
        } = input.into_parts();
        let alias_target = alias_target.into_inner();
        let hosted_zone_id = hosted_zone_id.into_inner();
        let name = name.into_inner();
        let resource_records = resource_records.into_inner();
        let ttl = ttl.into_inner();
        let record_type = record_type.into_inner();

        let record_set = build_record_set(
            &resource.id,
            alias_target.as_ref(),
            &name,
            resource_records.as_deref(),
            ttl,
            &record_type,
        )?;
        change_record_set(
            &self.route53_client,
            &hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &resource.id,
        )
        .await?;

        let identifier = make_identifier(&hosted_zone_id, &name, &record_type);
        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "route53.ChangeResourceRecordSets",
            identifier,
        )?;
        let outcome = self
            .read_route53_record_set(&resource.id, &identifier)
            .await;
        Ok((
            sent,
            self.create_outcome_from_read(
                &resource.id,
                identifier,
                resource.resolved_attributes(),
                outcome,
            ),
        ))
    }

    pub(crate) async fn update_route53_record_set(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: &crate::helpers::UpdateResource,
        _schema: &CompiledResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Route53RecordSetUpdateInput::decode(to)?;
        let crate::send_models_generated::Route53RecordSetUpdateParts {
            alias_target,
            resource_records,
            ttl,
        } = input.into_parts();
        let alias_target = alias_target.into_inner();
        let resource_records = resource_records.into_inner();
        let ttl = ttl.into_inner();
        let Some((hosted_zone_id, name, record_type)) = parse_identifier(identifier.as_str())
        else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid record set identifier: {}",
                identifier.as_str()
            ))
            .for_resource(id));
        };

        let record_set = build_record_set(
            &id,
            alias_target.as_ref(),
            name,
            resource_records.as_deref(),
            ttl,
            record_type,
        )?;
        change_record_set(
            &self.route53_client,
            hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &id,
        )
        .await?;

        let identifier = make_identifier(hosted_zone_id, name, record_type);
        let identifier = crate::helpers::applied_identifier(
            &id,
            "route53.ChangeResourceRecordSets",
            identifier,
        )?;
        Ok(self.read_route53_record_set(&id, &identifier).await)
    }

    pub(crate) async fn delete_route53_record_set(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();
        let Some((zone_id, name, record_type)) = parse_identifier(identifier) else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid record set identifier: {}",
                identifier
            ))
            .for_resource(id));
        };

        // Read current state to get the exact record for deletion.
        // Route 53 DELETE requires the record to match exactly.
        let ReadOutcome::Found { state: current } = self
            .read_route53_record_set(&id, applied_identifier)
            .await?
        else {
            return Ok(());
        };
        let current_attributes = current.attributes();

        let normalized_name = normalize_dns_name(name);
        let mut builder = ResourceRecordSet::builder()
            .name(&normalized_name)
            .r#type(RrType::from(record_type));

        if let Some(Value::Concrete(ConcreteValue::Int(ttl))) = current_attributes.get("ttl") {
            builder = builder.ttl(*ttl);
        }

        if let Some(Value::Concrete(ConcreteValue::List(records))) =
            current_attributes.get("resource_records")
        {
            let records = records
                .iter()
                .map(|record| match record {
                    Value::Concrete(ConcreteValue::String(record)) => {
                        Ok(record.expose_plaintext().to_string())
                    }
                    _ => Err(ProviderError::internal(
                        "read state resource_records contained a non-string value",
                    )
                    .for_resource(id.clone())),
                })
                .collect::<ProviderResult<Vec<_>>>()?;
            builder = builder.set_resource_records(Some(build_resource_records(&records, &id)?));
        }

        if let Some(Value::Concrete(ConcreteValue::Map(alias))) =
            current_attributes.get("alias_target")
        {
            builder = builder.alias_target(build_alias_target_from_map(alias, &id)?);
        }

        let record_set = builder.build().map_err(|e| {
            ProviderError::api_error_caused_by("Failed to build record set for deletion", e)
                .for_resource(id.clone())
        })?;

        change_record_set(
            &self.route53_client,
            zone_id,
            ChangeAction::Delete,
            record_set,
            &id,
        )
        .await
    }
}

/// Use the same DNS comparison spelling as record lookup and read projection.
/// This covers case, root dots, escaped letters and `\052` versus `*`, while
/// leaving deferred leaves opaque. Extends the dot-only symmetry from aws#300.
pub(crate) fn normalize_record_set_dns_names(input: &mut DesiredView<'_>) -> ProviderResult<()> {
    if let Some(DesiredValueMut::Known(DesiredScalar::String(name))) = input.attribute_mut("name") {
        *name = canonical_dns_name(name.expose_plaintext()).into();
    }

    if let Some(DesiredValueMut::Map(alias)) = input.attribute_mut("alias_target")
        && let Some(DesiredValueMut::Known(DesiredScalar::String(dns_name))) =
            alias.get_mut("dns_name")
    {
        *dns_name = canonical_dns_name(dns_name.expose_plaintext()).into();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::Resource;

    fn normalize(resource: &mut Resource) {
        let mut desired =
            carina_plugin_sdk::NativeDesiredNormalization::from_core(resource).unwrap();
        normalize_record_set_dns_names(&mut desired.view()).unwrap();
        desired.finish().unwrap();
    }

    fn record_set(name: &str) -> Resource {
        let mut r = Resource::with_provider("aws", "route53.RecordSet", "test-rec", None);
        r.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String(name.to_string().into())),
        );
        r
    }

    #[test]
    fn strips_trailing_dot_on_name() {
        let mut resource = record_set("_abc.example.com.");
        normalize(&mut resource);
        assert_eq!(
            resource.get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string().into()
            )))
        );
    }

    #[test]
    fn leaves_name_without_trailing_dot_untouched() {
        let mut resource = record_set("_abc.example.com");
        normalize(&mut resource);
        assert_eq!(
            resource.get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string().into()
            )))
        );
    }

    #[test]
    fn build_record_set_accepts_canonical_enum_type() {
        use carina_core::schema::{AttributeType, enum_identity};

        let attr_type = AttributeType::enum_(
            enum_identity("Type", Some("aws.route53.RecordSet")),
            Some(vec!["A".to_string(), "AAAA".to_string()]),
            vec![
                ("A".to_string(), "a".to_string()),
                ("AAAA".to_string(), "aaaa".to_string()),
            ],
            None,
            None,
        );
        let schema = crate::test_rootless_type(attr_type);
        let canonical = schema
            .canonicalize_state(Value::Concrete(ConcreteValue::String("AAAA".into())))
            .into_present()
            .expect("record-type fixture must remain present");

        let mut r = record_set("registry.example.com");
        r.set_attr(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String("Z123".to_string().into())),
        );
        r.set_attr("type".to_string(), canonical);
        let input = Route53RecordSetCreateInput::decode(
            &r,
            crate::test_resource_schema("route53.RecordSet"),
        )
        .expect("decode typed create input");
        let crate::send_models_generated::Route53RecordSetCreateParts {
            alias_target,
            hosted_zone_id,
            name,
            resource_records,
            ttl,
            r#type: record_type,
        } = input.into_parts();
        let alias_target = alias_target.into_inner();
        let hosted_zone_id = hosted_zone_id.into_inner();
        let name = name.into_inner();
        let resource_records = resource_records.into_inner();
        let ttl = ttl.into_inner();
        let record_type = record_type.into_inner();
        assert_eq!(hosted_zone_id, "Z123");
        let record_set = build_record_set(
            &r.id,
            alias_target.as_ref(),
            &name,
            resource_records.as_deref(),
            ttl,
            &record_type,
        )
        .expect("canonical enum type should build");

        assert_eq!(record_set.r#type().as_str(), "AAAA");
    }

    #[test]
    fn strips_trailing_dot_on_alias_target_dns_name() {
        let mut r = record_set("alias.example.com");
        let mut alias: IndexMap<String, Value> = IndexMap::new();
        alias.insert(
            "dns_name".to_string(),
            Value::Concrete(ConcreteValue::String(
                "dualstack.elb.amazonaws.com.".to_string().into(),
            )),
        );
        alias.insert(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String("Z35SXDOTRQ7X7K".to_string().into())),
        );
        alias.insert(
            "evaluate_target_health".to_string(),
            Value::Concrete(ConcreteValue::Bool(false)),
        );
        r.set_attr(
            "alias_target".to_string(),
            Value::Concrete(ConcreteValue::Map(alias)),
        );
        normalize(&mut r);
        let Some(Value::Concrete(ConcreteValue::Map(out))) = r.get_attr("alias_target") else {
            panic!("expected alias_target map");
        };
        assert_eq!(
            out.get("dns_name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "dualstack.elb.amazonaws.com".to_string().into()
            )))
        );
    }

    /// Build a minimal SDK `ResourceRecordSet` (name + type) for scan tests.
    fn rrs(name: &str, ty: &str) -> ResourceRecordSet {
        ResourceRecordSet::builder()
            .name(name)
            .r#type(RrType::from(ty))
            .build()
            .expect("name and type set")
    }

    /// The carina#3106 / aws#333 failure shape: an AAAA record exists at a
    /// name that also has A and NS records. Route53 orders A before NS
    /// before AAAA at the same name, so a page started at the AAAA cursor
    /// can still lead with the earlier-sorting types — the old
    /// `max_items(1)` scan returned the wrong single record and reported
    /// `not_found`. The page scan must find the AAAA.
    #[test]
    fn finds_target_type_that_does_not_sort_first_at_its_name() {
        let page = vec![
            rrs("registry-dev.carina-rs.dev.", "A"),
            rrs("registry-dev.carina-rs.dev.", "NS"),
            rrs("registry-dev.carina-rs.dev.", "AAAA"),
        ];
        match scan_page_for_record(&page, "registry-dev.carina-rs.dev.", "AAAA") {
            PageScan::Found(found) => {
                assert_eq!(found.name(), "registry-dev.carina-rs.dev.");
                assert_eq!(found.r#type().as_str(), "AAAA");
            }
            _ => panic!("AAAA exists on the page and must be Found"),
        }
    }

    /// A record absent from the zone: the listing started at the target
    /// name's cursor returns records for the *next* name. A different name
    /// means the target name is exhausted — stop, do not page the rest of
    /// the zone.
    #[test]
    fn stops_without_paging_once_name_sorts_past_target() {
        let page = vec![
            rrs("other.carina-rs.dev.", "A"),
            rrs("other.carina-rs.dev.", "AAAA"),
        ];
        assert!(matches!(
            scan_page_for_record(&page, "registry-dev.carina-rs.dev.", "AAAA"),
            PageScan::StopNotFound
        ));
    }

    /// Every record on the page is still the target name but the target
    /// type has not appeared yet (the name's records span a page
    /// boundary): keep paging.
    #[test]
    fn keeps_paging_when_target_name_records_span_a_page() {
        let page = vec![
            rrs("registry-dev.carina-rs.dev.", "A"),
            rrs("registry-dev.carina-rs.dev.", "NS"),
        ];
        assert!(matches!(
            scan_page_for_record(&page, "registry-dev.carina-rs.dev.", "AAAA"),
            PageScan::KeepPaging
        ));
    }

    /// An empty page (no records at or after the cursor) is not a match
    /// and nothing follows — treated as keep-paging; the caller's
    /// `is_truncated` check ends the loop.
    #[test]
    fn empty_page_keeps_paging() {
        assert!(matches!(
            scan_page_for_record(&[], "registry-dev.carina-rs.dev.", "AAAA"),
            PageScan::KeepPaging
        ));
    }
}
