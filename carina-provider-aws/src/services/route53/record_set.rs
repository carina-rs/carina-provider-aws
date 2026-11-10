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

/// DNS comparison spelling; raw names must be canonicalized before comparison.
#[derive(Debug, PartialEq, Eq)]
struct CanonicalDnsName(String);

impl CanonicalDnsName {
    fn new(name: &str) -> Self {
        Self(canonical_dns_name(name))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// A simple-routing record set, identified on disk by `hosted_zone_id|name|type`.
#[derive(Debug)]
struct RecordSetIdentity {
    hosted_zone_id: String,
    name: CanonicalDnsName,
    record_type: RrType,
    // Preserve the stored spelling for identifiers and Change requests. Lookup
    // uses `name`; sending its canonical spelling would change existing requests.
    stored_name: String,
}

impl RecordSetIdentity {
    fn try_new(
        hosted_zone_id: &str,
        name: &str,
        record_type: &str,
    ) -> Result<Self, RecordSetIdentityError> {
        for (part, value) in [
            ("hosted_zone_id", hosted_zone_id),
            ("name", name),
            ("type", record_type),
        ] {
            if value.is_empty() {
                return Err(RecordSetIdentityError::EmptyPart(part));
            }
            if value.contains('|') {
                return Err(RecordSetIdentityError::DelimiterInPart(part));
            }
        }
        Ok(Self {
            hosted_zone_id: hosted_zone_id.to_string(),
            name: CanonicalDnsName::new(name),
            record_type: RrType::from(record_type),
            stored_name: name.to_string(),
        })
    }

    fn parse(identifier: &AppliedIdentifier) -> Result<Self, RecordSetIdentityError> {
        let mut parts = identifier.as_str().split('|');
        let (Some(zone_id), Some(name), Some(record_type), extra) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(RecordSetIdentityError::InvalidFormat);
        };
        if extra.is_some() {
            return Err(RecordSetIdentityError::UnsupportedRoutingPolicy);
        }
        Self::try_new(zone_id, name, record_type)
    }

    fn to_identifier(&self) -> String {
        format!(
            "{}|{}|{}",
            self.hosted_zone_id,
            self.stored_name,
            self.record_type.as_str()
        )
    }

    fn wire_name(&self) -> String {
        normalize_dns_name(&self.stored_name)
    }

    fn matches(&self, record: &ResourceRecordSet) -> bool {
        self.name == CanonicalDnsName::new(record.name())
            && &self.record_type == record.r#type()
            && record.set_identifier().is_none()
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
enum RecordSetIdentityError {
    #[error("expected a record set identifier with exactly three parts: hosted_zone_id|name|type")]
    InvalidFormat,
    #[error(
        "routing-policy record sets are not supported (SetIdentifier); expected hosted_zone_id|name|type"
    )]
    UnsupportedRoutingPolicy,
    #[error("record set identifier {0} must not be empty")]
    EmptyPart(&'static str),
    #[error(r"record set {0} must not contain '|'; write it as \174")]
    DelimiterInPart(&'static str),
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
/// simple-routing `(name, type)` record. See [`scan_page_for_record`].
enum PageScan<'a> {
    /// Exact `(canonical name, type, absent SetIdentifier)` match on this page.
    Found(&'a ResourceRecordSet),
    /// A record whose name differs from the target appeared. Because the
    /// listing is cursor-started at the target name and ordered by name,
    /// a different name means the target name's records are exhausted —
    /// the record does not exist; stop without paging further.
    StopNotFound,
    /// Every record on this page is still at the target name but none
    /// matched the simple-routing identity yet (the name's records span a page
    /// boundary). Follow pagination and scan the next page.
    KeepPaging,
}

/// Scan one page of `list_resource_record_sets` results for the record
/// matching the simple-routing `identity`.
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
/// NOTE: the on-disk identity remains `zone|name|type`. Routing-policy record sets
/// (weighted/latency/geo/failover) have multiple records with the same
/// `(name, type)` distinguished by `SetIdentifier`. Carina manages only the
/// member with no `SetIdentifier`; skip routing members and keep scanning
/// until a simple record is found or the target name is exhausted (aws#335).
fn scan_page_for_record<'a>(
    records: &'a [ResourceRecordSet],
    identity: &RecordSetIdentity,
) -> PageScan<'a> {
    for rrs in records {
        if CanonicalDnsName::new(rrs.name()) != identity.name {
            return PageScan::StopNotFound;
        }
        if identity.matches(rrs) {
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
    identity: &RecordSetIdentity,
    resource_records: Option<&[String]>,
    ttl: Option<i64>,
) -> ProviderResult<ResourceRecordSet> {
    // Update's create-only name/type come from the applied identifier. Recheck
    // the outgoing values after deferred inputs have resolved.
    let name = &identity.stored_name;
    let record_type = identity.record_type.as_str();
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
        .name(identity.wire_name())
        .r#type(identity.record_type.clone());

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
        if name.contains('|') {
            return Err(record_input_error(
                id,
                "name",
                r"must not contain '|'; write it as \174",
            ));
        }
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
                CanonicalDnsName::new(alias.dns_name()).as_str().into(),
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
        let identity = RecordSetIdentity::parse(identifier).map_err(|error| {
            ProviderError::invalid_input_caused_by("Invalid record set identifier", error)
                .for_resource(id.clone())
        })?;

        // `start_record_*` are pagination cursors, not filters: AWS
        // returns records at or after that (name, type) position, ordered
        // by name then per-name type rank. Page from the cursor and scan
        // for the simple-routing identity; stop as soon as the listing moves
        // past the target name. The previous `max_items(1)` approach only
        // worked when the target type sorted first at its name and
        // silently returned `not_found` otherwise (aws#333).
        let mut start_name = Some(identity.wire_name());
        let mut start_type = Some(identity.record_type.clone());
        let mut start_identifier: Option<String> = None;

        loop {
            let mut request = self
                .route53_client
                .list_resource_record_sets()
                .hosted_zone_id(&identity.hosted_zone_id)
                .set_start_record_name(start_name.take())
                .set_start_record_type(start_type.take());
            if let Some(sid) = start_identifier.take() {
                request = request.start_record_identifier(sid);
            }

            let result = request.send().await.map_err(|e| {
                api_error_with_meta("Failed to list record sets", e).for_resource(id.clone())
            })?;

            match scan_page_for_record(result.resource_record_sets(), &identity) {
                PageScan::Found(rrs) => {
                    let fields =
                        Self::extract_route53_record_set_attributes(rrs).map_name(|name| {
                            name.map(|value| match value {
                                Value::Concrete(ConcreteValue::String(name)) => {
                                    Value::Concrete(ConcreteValue::String(
                                        CanonicalDnsName::new(name.expose_plaintext())
                                            .as_str()
                                            .into(),
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
                                identity.hosted_zone_id.clone().into(),
                            )),
                        },
                    )?;
                    return found_managed_read(id, identifier, model);
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
        let identity =
            RecordSetIdentity::try_new(&hosted_zone_id, &name, &record_type).map_err(|error| {
                ProviderError::invalid_input_caused_by("Invalid record set identifier", error)
                    .for_resource(resource.id.clone())
            })?;

        let record_set = build_record_set(
            &resource.id,
            alias_target.as_ref(),
            &identity,
            resource_records.as_deref(),
            ttl,
        )?;
        change_record_set(
            &self.route53_client,
            &identity.hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &resource.id,
        )
        .await?;

        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "route53.ChangeResourceRecordSets",
            identity.to_identifier(),
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
        let identity = RecordSetIdentity::parse(identifier).map_err(|error| {
            ProviderError::invalid_input_caused_by("Invalid record set identifier", error)
                .for_resource(id.clone())
        })?;
        let input = Route53RecordSetUpdateInput::decode(to)?;
        let crate::send_models_generated::Route53RecordSetUpdateParts {
            alias_target,
            resource_records,
            ttl,
        } = input.into_parts();
        let alias_target = alias_target.into_inner();
        let resource_records = resource_records.into_inner();
        let ttl = ttl.into_inner();
        let alias_target = alias_target.resolve(|| Ok(None))?;
        let resource_records = resource_records.resolve(|| Ok(None))?;
        let ttl = ttl.resolve(|| Ok(None))?;
        let record_set = build_record_set(
            &id,
            alias_target.as_ref(),
            &identity,
            resource_records.as_deref(),
            ttl,
        )?;
        change_record_set(
            &self.route53_client,
            &identity.hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &id,
        )
        .await?;

        let identifier = crate::helpers::applied_identifier(
            &id,
            "route53.ChangeResourceRecordSets",
            identity.to_identifier(),
        )?;
        Ok(self.read_route53_record_set(&id, &identifier).await)
    }

    pub(crate) async fn delete_route53_record_set(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identity = RecordSetIdentity::parse(identifier).map_err(|error| {
            ProviderError::invalid_input_caused_by("Invalid record set identifier", error)
                .for_resource(id.clone())
        })?;

        // Read current state to get the exact record for deletion.
        // Route 53 DELETE requires the record to match exactly.
        let ReadOutcome::Found { state: current } =
            self.read_route53_record_set(&id, identifier).await?
        else {
            return Ok(());
        };
        let current_attributes = current.attributes();

        let mut builder = ResourceRecordSet::builder()
            .name(identity.wire_name())
            .r#type(identity.record_type.clone());

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
            &identity.hosted_zone_id,
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
        *name = CanonicalDnsName::new(name.expose_plaintext())
            .as_str()
            .into();
    }

    if let Some(DesiredValueMut::Map(alias)) = input.attribute_mut("alias_target")
        && let Some(DesiredValueMut::Known(DesiredScalar::String(dns_name))) =
            alias.get_mut("dns_name")
    {
        *dns_name = CanonicalDnsName::new(dns_name.expose_plaintext())
            .as_str()
            .into();
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
        let identity = RecordSetIdentity::try_new(&hosted_zone_id, &name, &record_type)
            .expect("valid record identity");
        let record_set = build_record_set(
            &r.id,
            alias_target.as_ref(),
            &identity,
            resource_records.as_deref(),
            ttl,
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

    fn weighted_rrs(name: &str, ty: &str, set_identifier: &str) -> ResourceRecordSet {
        ResourceRecordSet::builder()
            .name(name)
            .r#type(RrType::from(ty))
            .set_identifier(set_identifier)
            .weight(1)
            .build()
            .expect("name and type set")
    }

    fn identity(identifier: &str) -> RecordSetIdentity {
        RecordSetIdentity::parse(&AppliedIdentifier::new(identifier).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn create_rejects_record_name_delimiter_without_requests() {
        let mut resource = record_set("a|b.example.com")
            .with_attribute(
                "hosted_zone_id",
                Value::Concrete(ConcreteValue::String("Z123".into())),
            )
            .with_attribute(
                "type",
                Value::Concrete(ConcreteValue::enum_identifier(
                    "aws.route53.RecordSet.Type.a",
                )),
            )
            .with_attribute("ttl", Value::Concrete(ConcreteValue::Int(300)))
            .with_attribute(
                "resource_records",
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::String("192.0.2.1".into()),
                )])),
            );
        carina_core::value::canonicalize_resources_with_schemas(
            std::slice::from_mut(&mut resource),
            &crate::schemas::schema_registry(),
        )
        .unwrap();
        let (provider, connector) = crate::test_http::provider([]);
        let error = match provider
            .create_route53_record_set(&resource, crate::test_resource_schema("route53.RecordSet"))
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("a raw identifier delimiter must be rejected before create"),
        };
        assert!(matches!(error, ProviderError::InvalidInput(_)));
        let error =
            carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
        assert!(
            error
                .detail()
                .cause()
                .is_some_and(|cause| cause.is::<RecordSetIdentityError>())
        );
        assert!(
            error
                .to_string()
                .contains(r"record set name must not contain '|'; write it as \174"),
            "{error}"
        );
        connector.assert_finished();
        assert!(connector.requests().is_empty());
    }

    #[test]
    fn checked_identity_constructor_rejects_delimiters_in_every_part() {
        for (zone, name, record_type, part) in [
            ("Z|123", "example.com", "A", "hosted_zone_id"),
            ("Z123", "a|b.example.com", "A", "name"),
            ("Z123", "example.com", "A|a-first", "type"),
        ] {
            let error = RecordSetIdentity::try_new(zone, name, record_type).unwrap_err();
            assert_eq!(error, RecordSetIdentityError::DelimiterInPart(part));
            assert_eq!(
                error.to_string(),
                format!(r"record set {part} must not contain '|'; write it as \174")
            );
        }
    }

    #[test]
    fn checked_identities_round_trip_for_valid_dns_spellings() {
        for zone in ["Z123", "Z456"] {
            for name in [
                "example.com",
                "Example.COM",
                "Example.COM.",
                "*.example.com",
                "*.Example.COM.",
                r"\052.example.com.",
                r"a\174b.example.com",
                r"a\056b.example.com",
                r"a\134b.example.com",
                r"\105xample.com",
            ] {
                for record_type in ["A", "AAAA", "TXT"] {
                    let original = RecordSetIdentity::try_new(zone, name, record_type).unwrap();
                    let identifier = original.to_identifier();
                    let parsed = RecordSetIdentity::parse(
                        &AppliedIdentifier::new(identifier.clone()).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(parsed.hosted_zone_id, original.hosted_zone_id);
                    assert_eq!(parsed.name, original.name);
                    assert_eq!(parsed.record_type, original.record_type);
                    assert_eq!(parsed.to_identifier(), identifier);
                }
            }
        }
    }

    #[test]
    fn three_part_identity_round_trips_and_preserves_change_request_name() {
        let id = record_set("example.com").id;
        for (identifier, wire_name) in [
            ("Z123|example.com|A", "example.com."),
            ("Z123|Example.COM.|A", "Example.COM."),
            ("Z123|*.Example.COM|A", "*.Example.COM."),
            (r"Z123|\052.Example.COM.|A", r"\052.Example.COM."),
        ] {
            let identity = identity(identifier);
            assert_eq!(identity.hosted_zone_id, "Z123");
            assert_eq!(identity.record_type.as_str(), "A");
            assert_eq!(identity.to_identifier(), identifier);
            let record = build_record_set(
                &id,
                None,
                &identity,
                Some(&["192.0.2.1".to_string()]),
                Some(300),
            )
            .unwrap();
            assert_eq!(record.name(), wire_name);
        }
    }

    #[test]
    fn identity_matches_wildcard_and_case_canonically() {
        for (identifier, returned_name, canonical) in [
            (
                "Z123|*.example.com|A",
                r"\052.example.com.",
                r"\052.example.com",
            ),
            ("Z123|Example.COM|A", "example.com.", "example.com"),
        ] {
            let identity = identity(identifier);
            assert_eq!(identity.name.as_str(), canonical);
            assert!(identity.matches(&rrs(returned_name, "A")));
            assert!(matches!(
                scan_page_for_record(&[rrs(returned_name, "A")], &identity),
                PageScan::Found(_)
            ));
        }
    }

    #[test]
    fn identity_rejects_set_identifier_segments_with_typed_error() {
        for identifier in [
            "Z123|example.com|A|a-first",
            "Z123|example.com|A|",
            "Z123|example.com|A|a-first|extra",
        ] {
            let error =
                RecordSetIdentity::parse(&AppliedIdentifier::new(identifier).unwrap()).unwrap_err();
            assert_eq!(error, RecordSetIdentityError::UnsupportedRoutingPolicy);
            assert!(
                error
                    .to_string()
                    .contains("routing-policy record sets are not supported")
            );
        }
    }

    #[test]
    fn identity_rejects_empty_parts_with_typed_error() {
        for (zone, name, record_type, part) in [
            ("", "example.com", "A", "hosted_zone_id"),
            ("Z123", "", "A", "name"),
            ("Z123", "example.com", "", "type"),
        ] {
            assert_eq!(
                RecordSetIdentity::try_new(zone, name, record_type).unwrap_err(),
                RecordSetIdentityError::EmptyPart(part)
            );
            let identifier = format!("{zone}|{name}|{record_type}");
            let error =
                RecordSetIdentity::parse(&AppliedIdentifier::new(identifier).unwrap()).unwrap_err();
            assert_eq!(error, RecordSetIdentityError::EmptyPart(part));
            assert_eq!(
                error.to_string(),
                format!("record set identifier {part} must not be empty")
            );
        }
    }

    #[test]
    fn identity_rejects_missing_parts_with_typed_error() {
        for identifier in ["Z123", "Z123|example.com"] {
            let error =
                RecordSetIdentity::parse(&AppliedIdentifier::new(identifier).unwrap()).unwrap_err();
            assert_eq!(error, RecordSetIdentityError::InvalidFormat);
            assert_eq!(
                error.to_string(),
                "expected a record set identifier with exactly three parts: hosted_zone_id|name|type"
            );
        }
    }

    #[tokio::test]
    async fn malformed_identities_error_consistently_before_read_update_or_delete_requests() {
        let id = record_set("example.com").id;
        let current =
            crate::read_model::test_existing_state(id.clone(), Default::default()).to_snapshot();
        let to = crate::helpers::UpdateResource::new(
            &current,
            &carina_core::provider::UpdatePatch { ops: vec![] },
        );
        for (raw, expected_reason) in [
            (
                "Z123",
                "expected a record set identifier with exactly three parts",
            ),
            (
                "Z123|example.com",
                "expected a record set identifier with exactly three parts",
            ),
            (
                "Z123|example.com|A|a-first",
                "routing-policy record sets are not supported",
            ),
            (
                "|example.com|A",
                "record set identifier hosted_zone_id must not be empty",
            ),
            ("Z123||A", "record set identifier name must not be empty"),
            (
                "Z123|example.com|",
                "record set identifier type must not be empty",
            ),
        ] {
            let (provider, connector) = crate::test_http::provider([]);
            let identifier = AppliedIdentifier::new(raw).unwrap();
            let read_error = provider
                .read_route53_record_set(&id, &identifier)
                .await
                .expect_err("malformed read identity must be rejected");
            let update_error = provider
                .update_route53_record_set(
                    id.clone(),
                    &identifier,
                    &to,
                    crate::test_resource_schema("route53.RecordSet"),
                )
                .await
                .expect_err("malformed update identity must be rejected");
            let delete_error = provider
                .delete_route53_record_set(id.clone(), &identifier)
                .await
                .expect_err("malformed delete identity must be rejected");
            for error in [read_error, update_error, delete_error] {
                assert!(matches!(error, ProviderError::InvalidInput(_)));
                let error =
                    carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(
                        error,
                    );
                assert!(
                    error
                        .detail()
                        .cause()
                        .is_some_and(|cause| cause.is::<RecordSetIdentityError>())
                );
                assert!(error.to_string().contains(expected_reason), "{error}");
            }
            connector.assert_finished();
        }
    }

    #[test]
    fn routing_members_only_keep_paging() {
        let page = vec![
            weighted_rrs("example.com.", "A", "a-first"),
            weighted_rrs("example.com.", "A", "b-second"),
        ];
        assert!(matches!(
            scan_page_for_record(&page, &identity("Z123|example.com|A")),
            PageScan::KeepPaging
        ));
    }

    #[test]
    fn routing_member_followed_by_different_name_stops_not_found() {
        let page = vec![
            weighted_rrs("example.com.", "A", "a-first"),
            rrs("next.example.com.", "A"),
        ];
        assert!(matches!(
            scan_page_for_record(&page, &identity("Z123|example.com|A")),
            PageScan::StopNotFound
        ));
    }

    #[test]
    fn routing_target_type_with_other_simple_type_stops_at_different_name() {
        let page = vec![
            weighted_rrs("example.com.", "AAAA", "a-first"),
            rrs("example.com.", "TXT"),
            rrs("next.example.com.", "AAAA"),
        ];
        assert!(matches!(
            scan_page_for_record(&page, &identity("Z123|example.com|AAAA")),
            PageScan::StopNotFound
        ));
    }

    #[tokio::test]
    async fn read_with_only_weighted_members_is_not_found_across_identifier_cursor() {
        let page = |set_identifier: &str, cursor: &str| {
            format!(
                r#"<ListResourceRecordSetsResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/">
                    <ResourceRecordSets><ResourceRecordSet>
                        <Name>example.com.</Name><Type>A</Type>
                        <SetIdentifier>{set_identifier}</SetIdentifier><Weight>1</Weight>
                        <TTL>300</TTL><ResourceRecords><ResourceRecord><Value>192.0.2.1</Value></ResourceRecord></ResourceRecords>
                    </ResourceRecordSet></ResourceRecordSets>
                    {cursor}<MaxItems>100</MaxItems>
                </ListResourceRecordSetsResponse>"#
            )
        };
        let (provider, connector) = crate::test_http::provider([
            (
                200,
                page(
                    "a-first",
                    "<IsTruncated>true</IsTruncated><NextRecordName>example.com.</NextRecordName><NextRecordType>A</NextRecordType><NextRecordIdentifier>b-second</NextRecordIdentifier>",
                ),
            ),
            (200, page("b-second", "<IsTruncated>false</IsTruncated>")),
        ]);
        let id = record_set("example.com").id;
        let outcome = provider
            .read_route53_record_set(&id, &AppliedIdentifier::new("Z123|example.com|A").unwrap())
            .await
            .unwrap();
        assert!(matches!(outcome, ReadOutcome::NotFound { .. }));
        connector.assert_finished();
        let requests = connector.requests();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1].contains("identifier=b-second"),
            "{}",
            requests[1]
        );
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
        match scan_page_for_record(&page, &identity("Z123|registry-dev.carina-rs.dev.|AAAA")) {
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
            scan_page_for_record(&page, &identity("Z123|registry-dev.carina-rs.dev.|AAAA")),
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
            scan_page_for_record(&page, &identity("Z123|registry-dev.carina-rs.dev.|AAAA")),
            PageScan::KeepPaging
        ));
    }

    /// An empty page (no records at or after the cursor) is not a match
    /// and nothing follows — treated as keep-paging; the caller's
    /// `is_truncated` check ends the loop.
    #[test]
    fn empty_page_keeps_paging() {
        assert!(matches!(
            scan_page_for_record(&[], &identity("Z123|registry-dev.carina-rs.dev.|AAAA")),
            PageScan::KeepPaging
        ));
    }
}
