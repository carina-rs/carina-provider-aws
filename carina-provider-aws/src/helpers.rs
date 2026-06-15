//! Shared helper functions for the AWS provider.
//!
//! These reduce boilerplate across EC2 (and other) service implementations.

use indexmap::IndexMap;
use std::future::Future;
use std::time::Duration;

use aws_sdk_ec2::types::{ResourceType, Tag, TagSpecification};
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_smithy_types::retry::ProvideErrorKind;
use tokio::time::sleep;

use carina_core::provider::{PatchOpKind, ProviderError, ProviderResult, UpdatePatch};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

/// Borrow a `Value` as `&str` if it is a concrete string, otherwise `None`.
pub fn value_as_str(v: &Value) -> Option<&str> {
    if let Value::Concrete(ConcreteValue::String(s)) = v {
        Some(s.as_str())
    } else {
        None
    }
}

/// Extract a required `String` attribute from a resource.
///
/// Returns the string value or a `ProviderError` with `"{attr_name} is required"`.
pub fn require_string_attr(resource: &Resource, attr_name: &str) -> ProviderResult<String> {
    match resource.get_attr(attr_name) {
        Some(Value::Concrete(ConcreteValue::String(s))) => Ok(s.clone()),
        _ => Err(
            ProviderError::invalid_input(format!("{} is required", attr_name))
                .for_resource(resource.id.clone()),
        ),
    }
}

/// Return the AWS API canonical spelling for a schema-typed enum
/// attribute, accepting every `Value` shape the carina-core canonicalize
/// pipeline can produce at the provider boundary:
///
/// - `ConcreteValue::String(s)` — quoted-string DSL form
///   (`validation_method = 'DNS'`) or a state-read echo
/// - `ConcreteValue::EnumIdentifier(raw)` — bare DSL identifier form
///   (`validation_method = dns`, post-carina#3463)
/// - `ConcreteValue::CanonicalEnum(c)` — typed witness produced by
///   `canonicalize_resources_with_schemas` for schema-known values
///   (carina#3438)
///
/// Returns `None` for any other shape or a missing attribute. Use this
/// (or its wrappers below) for any schema attribute declared as
/// `AttributeType::enum_(..)`. Plain `value_as_str` /
/// `require_string_attr` match only `ConcreteValue::String` and silently
/// drop the other two variants, which is the carina-rs/carina-provider-aws#440
/// failure mode.
pub(crate) fn enum_attr_str<'a>(resource: &'a Resource, attr_name: &str) -> Option<&'a str> {
    match resource.get_attr(attr_name)? {
        Value::Concrete(ConcreteValue::String(s)) => Some(s.as_str()),
        Value::Concrete(ConcreteValue::EnumIdentifier(raw)) => Some(raw.as_str()),
        Value::Concrete(ConcreteValue::CanonicalEnum(c)) => Some(c.api_value()),
        _ => None,
    }
}

/// Required-attribute wrapper around [`enum_attr_str`]. Returns the
/// AWS API canonical spelling, or `ProviderError::invalid_input`
/// when the attribute is absent or carries a non-enum-shaped value.
pub fn require_enum_attr(resource: &Resource, attr_name: &str) -> ProviderResult<String> {
    enum_attr_str(resource, attr_name)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ProviderError::invalid_input(format!("{} is required", attr_name))
                .for_resource(resource.id.clone())
        })
}

/// Optional-attribute wrapper around [`enum_attr_str`] for readability
/// at call sites where the enum is optional (e.g. ACM
/// `validation_method`, `key_algorithm`).
pub fn optional_enum_attr<'a>(resource: &'a Resource, attr_name: &str) -> Option<&'a str> {
    enum_attr_str(resource, attr_name)
}

/// Return the AWS API canonical spelling for a schema-typed enum
/// attribute that lives inside a nested struct attribute (the
/// `Map`-shaped value at `struct_attr`). Accepts every `Value` shape the
/// carina-core canonicalize pipeline can produce, matching
/// [`enum_attr_str`].
///
/// Returns `None` when the outer struct attribute is absent or not a
/// Map, when the field is absent inside the Map, or when the field's
/// value is not an enum-shaped scalar. The carina-rs/carina-provider-aws#441
/// / #442 failure mode was the provider reading a struct field at top
/// level (`resource.get_attr("dns_support")`) rather than from inside
/// its parent struct (`resource.get_attr("options")` -> Map ->
/// `m.get("dns_support")`).
pub(crate) fn enum_struct_field_str<'a>(
    resource: &'a Resource,
    struct_attr: &str,
    field_name: &str,
) -> Option<&'a str> {
    let Some(Value::Concrete(ConcreteValue::Map(m))) = resource.get_attr(struct_attr) else {
        return None;
    };
    match m.get(field_name)? {
        Value::Concrete(ConcreteValue::String(s)) => Some(s.as_str()),
        Value::Concrete(ConcreteValue::EnumIdentifier(raw)) => Some(raw.as_str()),
        Value::Concrete(ConcreteValue::CanonicalEnum(c)) => Some(c.api_value()),
        _ => None,
    }
}

pub(crate) fn optional_enum_struct_field<'a>(
    resource: &'a Resource,
    struct_attr: &str,
    field_name: &str,
) -> Option<&'a str> {
    enum_struct_field_str(resource, struct_attr, field_name)
}

/// Return the `i64` value of an `Int`-typed field inside the nested
/// struct attribute `struct_attr`. Same shape contract as
/// [`enum_struct_field_str`] (outer attribute must be a `Map`; missing
/// outer/field returns `None`), narrowed to a single `Value` shape:
/// `ConcreteValue::Int`. Anything else returns `None`.
///
/// Sibling of [`optional_enum_struct_field`]. The wider-shape variants
/// the carina-core canonicalize pipeline could in principle produce
/// (e.g. a Deferred unknown) are intentionally rejected — schema-typed
/// `Int` attributes do not have a canonical-enum form to absorb, so
/// there is nothing analogous to the three-variant enum coverage.
/// carina-rs/carina-provider-aws#441 / #442.
pub(crate) fn optional_int_struct_field(
    resource: &Resource,
    struct_attr: &str,
    field_name: &str,
) -> Option<i64> {
    let Some(Value::Concrete(ConcreteValue::Map(m))) = resource.get_attr(struct_attr) else {
        return None;
    };
    match m.get(field_name)? {
        Value::Concrete(ConcreteValue::Int(i)) => Some(*i),
        _ => None,
    }
}

/// Return the `Vec<String>` value of a `List<String>`-typed field
/// inside the nested struct attribute `struct_attr`. Accepts both
/// `ConcreteValue::StringList` (the canonical shape after carina-core
/// `canonicalize_resources_with_schemas` resolves a
/// `string_or_list_of_strings` attribute) and `ConcreteValue::List` of
/// `ConcreteValue::String` (the parser-emitted shape before
/// canonicalization). Anything else, including a list with any
/// non-`String` element, returns `None`.
///
/// Same shape contract as [`enum_struct_field_str`] for the outer
/// `Map`. Sibling of [`optional_enum_struct_field`] /
/// [`optional_int_struct_field`]; the `List<NonString>` case is
/// intentionally rejected — schema-typed `List<String>` is the only
/// inner-list shape this PR's seam covers, and a non-string element
/// indicates schema/value mismatch upstream.
/// carina-rs/carina-provider-aws#441 / #442.
pub(crate) fn optional_string_list_struct_field(
    resource: &Resource,
    struct_attr: &str,
    field_name: &str,
) -> Option<Vec<String>> {
    let Some(Value::Concrete(ConcreteValue::Map(m))) = resource.get_attr(struct_attr) else {
        return None;
    };
    match m.get(field_name)? {
        Value::Concrete(ConcreteValue::StringList(items)) => Some(items.clone()),
        Value::Concrete(ConcreteValue::List(items)) => items
            .iter()
            .map(|item| match item {
                Value::Concrete(ConcreteValue::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Build an EC2 `TagSpecification` from DSL tags for a given resource type.
///
/// Returns `None` if the resource has no `tags` attribute.
pub fn build_tag_specification(
    resource: &Resource,
    resource_type: ResourceType,
) -> Option<TagSpecification> {
    if let Some(Value::Concrete(ConcreteValue::Map(tags))) = resource.get_attr("tags") {
        Some(build_tag_specification_from_map(tags, resource_type))
    } else {
        None
    }
}

/// Build an EC2 `TagSpecification` from a tag map.
fn build_tag_specification_from_map(
    tags: &IndexMap<String, Value>,
    resource_type: ResourceType,
) -> TagSpecification {
    let mut tag_spec = TagSpecification::builder().resource_type(resource_type);
    for (key, val) in tags {
        if let Value::Concrete(ConcreteValue::String(v)) = val {
            tag_spec = tag_spec.tags(Tag::builder().key(key).value(v).build());
        }
    }
    tag_spec.build()
}

/// Represents the state returned by a poll function for `wait_for_ec2_state`.
pub enum PollState {
    /// The resource reached the desired state.
    Ready,
    /// The resource reached a terminal failure state.
    Failed,
    /// The resource no longer exists (useful for delete waits).
    Gone,
    /// The resource is still transitioning.
    Pending,
}

/// Format an AWS SDK error with the full error chain.
///
/// Uses `DisplayErrorContext` to walk the source chain, producing messages like:
/// `ChangeResourceRecordSets failed: service error: InvalidChangeBatch: ...`
/// instead of the unhelpful `ChangeResourceRecordSets failed: service error`.
pub fn sdk_error_message(context: &str, err: &(impl std::error::Error + 'static)) -> String {
    format!("{}: {}", context, DisplayErrorContext(err))
}

/// Exponential-backoff retry budget for [`retry_aws_operation`].
///
/// All AWS calls in this provider share a single policy
/// ([`RetryPolicy::default`]) rather than per-call-site magic numbers,
/// so the retry budget is tuned in one place. The fields are named so a
/// call site reads as intent, not as two unlabeled integers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryPolicy {
    /// Maximum number of attempts, including the first.
    max_attempts: u32,
    /// Delay before the first retry; doubles each subsequent attempt.
    initial_delay_secs: u64,
    /// Upper bound on any single backoff delay.
    max_delay_secs: u64,
}

impl RetryPolicy {
    /// Backoff delay before the given retry (`attempt` is 1-based: the
    /// delay after the 1st failed attempt is `attempt == 1`).
    fn delay_secs(&self, attempt: u32) -> u64 {
        let raw = self
            .initial_delay_secs
            .saturating_mul(2u64.saturating_pow(attempt - 1));
        std::cmp::min(raw, self.max_delay_secs)
    }
}

impl Default for RetryPolicy {
    /// 8 attempts, 5s initial backoff, 120s cap.
    ///
    /// Backoff schedule (7 retries): 5, 10, 20, 40, 80, 120, 120 — 395s
    /// of cumulative sleep. S3 `OperationAborted` (CreateBucket /
    /// DeleteBucket name races, clearing in ~60–90s) and `RequestTimeout`
    /// recurring under load both need a budget well past the 75s of
    /// sleep that the previous 5-attempt default gave (4 retries:
    /// 5+10+20+40); 8 attempts cover them without pinning a single
    /// operation for the ~10min an even larger budget would.
    fn default() -> Self {
        Self {
            max_attempts: 8,
            initial_delay_secs: 5,
            max_delay_secs: 120,
        }
    }
}

/// Retry an AWS SDK operation with exponential backoff on transient errors.
///
/// Whether an error is retried is decided by [`is_retryable_sdk_error`],
/// which consults the SDK's own classification (`ProvideErrorKind`) plus
/// an explicit carve-out for the [`S3_RETRYABLE_ERROR_CODES`] that the
/// AWS docs call transient but the S3 SDK model does not flag.
///
/// - `operation_name`: Human-readable name for log messages.
/// - `policy`: The [`RetryPolicy`] budget — production callers pass
///   [`RetryPolicy::default()`].
/// - `f`: A closure that returns a `Future` producing the SDK result.
pub async fn retry_aws_operation<F, Fut, T, E, R>(
    operation_name: &str,
    policy: RetryPolicy,
    f: F,
) -> Result<T, SdkError<E, R>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, SdkError<E, R>>>,
    E: ProvideErrorKind + ProvideErrorMetadata + std::error::Error + 'static,
    R: std::fmt::Debug,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < policy.max_attempts && is_retryable_sdk_error(&e) => {
                let delay = policy.delay_secs(attempt);
                eprintln!(
                    "  Retrying {} (attempt {}/{}): {}",
                    operation_name,
                    attempt,
                    policy.max_attempts,
                    DisplayErrorContext(&e)
                );
                sleep(Duration::from_secs(delay)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// S3 service-error codes that the AWS docs classify as transient but
/// the generated SDK does not flag as retryable.
///
/// The S3 Smithy model attaches no `@retryable` trait to any operation
/// error, so every S3 operation's generated `retryable_error_kind()`
/// returns `None` unconditionally (e.g. `DeleteBucketWebsiteError` only
/// has an `Unhandled` variant). Without this carve-out, transient S3
/// failures abort on the first occurrence instead of backing off.
///
/// - `RequestTimeout` (HTTP 400): "Your socket connection to the server
///   was not read from or written to within the timeout period." The
///   raw response is marked `retryable: true`; a retry of an idempotent
///   operation is safe.
/// - `SlowDown` (HTTP 503): S3 request-rate throttling — back off.
/// - `OperationAborted` (HTTP 409): CreateBucket / DeleteBucket race;
///   the bucket name's control-plane state clears in ~60–90s.
const S3_RETRYABLE_ERROR_CODES: &[&str] = &["RequestTimeout", "SlowDown", "OperationAborted"];

/// Classify an [`SdkError`] as transient (worth retrying) or terminal.
///
/// Service errors are classified in two steps:
///
/// 1. The SDK's own `retryable_error_kind()` — `Some(ErrorKind)` when the
///    service model marks the error retryable (throttling, server
///    errors). Note this is **always `None` for S3**: no S3 operation
///    error carries an `@retryable` trait.
/// 2. An explicit carve-out for [`S3_RETRYABLE_ERROR_CODES`] — S3 codes
///    the AWS docs call transient but the SDK model does not flag.
///
/// Transport-layer failures (`TimeoutError`, `DispatchFailure`,
/// `ResponseError`, `ConstructionFailure`) are always transient — the
/// HTTP exchange did not complete, so a retry is safe for idempotent
/// operations (which is what all carina provider operations are).
pub fn is_retryable_sdk_error<E, R>(err: &SdkError<E, R>) -> bool
where
    E: ProvideErrorKind + ProvideErrorMetadata,
{
    match err {
        SdkError::ConstructionFailure(_)
        | SdkError::TimeoutError(_)
        | SdkError::DispatchFailure(_)
        | SdkError::ResponseError(_) => true,
        SdkError::ServiceError(ctx) => {
            let inner = ctx.err();
            if inner.retryable_error_kind().is_some() {
                return true;
            }
            // S3 carve-out: the SDK model flags no S3 error as
            // retryable, so consult the documented transient codes.
            // Disambiguate `code()` — both `ProvideErrorKind` and
            // `ProvideErrorMetadata` define a same-named accessor.
            matches!(
                ProvideErrorMetadata::code(inner),
                Some(code) if S3_RETRYABLE_ERROR_CODES.contains(&code)
            )
        }
        // `SdkError` is `#[non_exhaustive]`; future-added variants get
        // the conservative default (don't retry).
        _ => false,
    }
}

/// Reconstruct a `Resource` from `from` state plus an `UpdatePatch`.
///
/// The aws provider's existing per-resource `update_*` methods take
/// `to: Resource` (a full desired state). The Level 3 `Provider::update`
/// signature replaces `to` with `(from: State, patch: UpdatePatch)`, so
/// this adapter rebuilds an equivalent desired `Resource` by applying
/// each [`PatchOpKind`] on top of `from`'s attributes:
///
/// - `Add` / `Replace` set the attribute to the patch value.
/// - `Remove` deletes the attribute from the resulting resource.
///
/// This is a faithful translation: the result mirrors what the user's
/// desired state would look like if it had been computed full-replace
/// style. Per-resource update methods continue to write the same fields
/// they always did.
///
/// The returned `Resource` carries `from`'s `ResourceId` and an empty
/// `Directives` (directives are delete-only and are not consulted on
/// update paths in this provider).
pub fn apply_patch_to_state(from: &State, patch: &UpdatePatch) -> Resource {
    let mut resource = Resource::new(from.id.resource_type.clone(), from.id.name.to_string());
    resource.id = from.id.clone();
    resource.attributes = from
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for op in &patch.ops {
        match op.kind {
            PatchOpKind::Add | PatchOpKind::Replace => {
                if let Some(ref v) = op.value {
                    resource.attributes.insert(op.key.clone(), v.clone());
                }
            }
            PatchOpKind::Remove => {
                resource.attributes.shift_remove(&op.key);
            }
        }
    }
    resource
}

/// Generic wait/poll loop for EC2 resources.
///
/// Polls at 5-second intervals for up to `max_iterations` iterations.
///
/// - `poll_fn`: An async function that describes the resource and returns its `PollState`.
/// - `max_iterations`: Maximum number of poll iterations (each 5 seconds apart).
/// - `timeout_msg`: Error message if the loop times out.
/// - `failure_msg`: Error message if the resource reaches a failed state.
pub async fn wait_for_ec2_state<F, Fut>(
    id: &ResourceId,
    poll_fn: F,
    max_iterations: u32,
    timeout_msg: &str,
    failure_msg: &str,
) -> ProviderResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = ProviderResult<PollState>>,
{
    for _ in 0..max_iterations {
        match poll_fn().await? {
            PollState::Ready => return Ok(()),
            PollState::Gone => return Ok(()),
            PollState::Failed => {
                return Err(ProviderError::api_error(failure_msg).for_resource(id.clone()));
            }
            PollState::Pending => {}
        }
        sleep(Duration::from_secs(5)).await;
    }

    Err(ProviderError::timeout(timeout_msg).for_resource(id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_resource(attrs: Vec<(&str, &str)>) -> Resource {
        let mut resource = Resource::new("acm.Certificate", "test");
        for (k, v) in attrs {
            resource.attributes.insert(
                k.to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        resource
    }

    fn make_resource_with_value(attr_name: &str, value: Value) -> Resource {
        let mut resource = Resource::new("acm.Certificate", "test");
        resource.attributes.insert(attr_name.to_string(), value);
        resource
    }

    fn make_resource_with_struct_field(field_name: &str, value: Value) -> Resource {
        make_resource_with_value(
            "options",
            Value::Concrete(ConcreteValue::Map(
                [(field_name.to_string(), value)].into_iter().collect(),
            )),
        )
    }

    fn canonical_validation_method_value() -> Value {
        use carina_core::schema::{AttributeType, Schema, enum_identity};

        let attr_type = AttributeType::enum_(
            enum_identity("ValidationMethod", Some("aws.acm.Certificate")),
            Some(vec!["DNS".to_string(), "EMAIL".to_string()]),
            vec![
                ("DNS".to_string(), "dns".to_string()),
                ("EMAIL".to_string(), "email".to_string()),
            ],
            None,
            None,
        );
        let schema = Schema::flat(attr_type);
        schema.canonicalize(Value::Concrete(ConcreteValue::enum_identifier("dns")))
    }

    #[test]
    fn test_apply_patch_to_state_add_replace_remove() {
        use carina_core::provider::{PatchOp, PatchOpKind, UpdatePatch};
        use std::collections::HashMap;

        // from-state has two attrs; patch adds one, replaces one, removes one.
        let id = ResourceId::with_provider("aws", "ec2.Vpc", "test", None);
        let mut from_attrs: HashMap<String, Value> = HashMap::new();
        from_attrs.insert(
            "cidr_block".into(),
            Value::Concrete(ConcreteValue::String("10.0.0.0/16".into())),
        );
        from_attrs.insert(
            "tags".into(),
            Value::Concrete(ConcreteValue::Map(
                [(
                    "Name".to_string(),
                    Value::Concrete(ConcreteValue::String("old".into())),
                )]
                .into_iter()
                .collect(),
            )),
        );
        let from = State::existing(id, from_attrs);

        let patch = UpdatePatch {
            ops: vec![
                // Add a brand-new attribute
                PatchOp {
                    kind: PatchOpKind::Add,
                    key: "instance_tenancy".into(),
                    value: Some(Value::Concrete(ConcreteValue::String("default".into()))),
                },
                // Replace the existing tags
                PatchOp {
                    kind: PatchOpKind::Replace,
                    key: "tags".into(),
                    value: Some(Value::Concrete(ConcreteValue::Map(
                        [(
                            "Name".to_string(),
                            Value::Concrete(ConcreteValue::String("new".into())),
                        )]
                        .into_iter()
                        .collect(),
                    ))),
                },
                // Remove cidr_block
                PatchOp {
                    kind: PatchOpKind::Remove,
                    key: "cidr_block".into(),
                    value: None,
                },
            ],
        };

        let to = apply_patch_to_state(&from, &patch);

        assert!(
            !to.attributes.contains_key("cidr_block"),
            "Remove op should drop the key"
        );
        assert_eq!(
            to.attributes.get("instance_tenancy"),
            Some(&Value::Concrete(ConcreteValue::String("default".into()))),
            "Add op should insert the new value"
        );
        match to.attributes.get("tags") {
            Some(Value::Concrete(ConcreteValue::Map(m))) => {
                assert_eq!(
                    m.get("Name"),
                    Some(&Value::Concrete(ConcreteValue::String("new".into())))
                );
            }
            other => panic!("expected tags map, got {:?}", other),
        }
        // ResourceId is preserved from `from`
        assert_eq!(to.id.resource_type, "ec2.Vpc");
    }

    #[test]
    fn test_require_enum_attr_string() {
        let resource = make_test_resource(vec![("type", "DNS")]);
        assert_eq!(require_enum_attr(&resource, "type").unwrap(), "DNS");
    }

    #[test]
    fn test_require_enum_attr_enum_identifier() {
        let resource = make_resource_with_value(
            "type",
            Value::Concrete(ConcreteValue::enum_identifier("dns")),
        );
        assert_eq!(require_enum_attr(&resource, "type").unwrap(), "dns");
    }

    #[test]
    fn test_require_enum_attr_canonical_enum() {
        let resource = make_resource_with_value("type", canonical_validation_method_value());
        assert_eq!(require_enum_attr(&resource, "type").unwrap(), "DNS");
    }

    #[test]
    fn test_require_enum_attr_missing_is_err() {
        let resource = make_test_resource(vec![]);
        assert!(require_enum_attr(&resource, "type").is_err());
    }

    #[test]
    fn test_require_enum_attr_non_enum_shape_is_err() {
        let resource = make_resource_with_value("type", Value::Concrete(ConcreteValue::Int(1)));
        assert!(require_enum_attr(&resource, "type").is_err());
    }

    #[test]
    fn test_optional_enum_attr_string() {
        let resource = make_test_resource(vec![("type", "DNS")]);
        assert_eq!(optional_enum_attr(&resource, "type"), Some("DNS"));
    }

    #[test]
    fn test_optional_enum_attr_enum_identifier() {
        let resource = make_resource_with_value(
            "type",
            Value::Concrete(ConcreteValue::enum_identifier("dns")),
        );
        assert_eq!(optional_enum_attr(&resource, "type"), Some("dns"));
    }

    #[test]
    fn test_optional_enum_attr_canonical_enum() {
        let resource = make_resource_with_value("type", canonical_validation_method_value());
        assert_eq!(optional_enum_attr(&resource, "type"), Some("DNS"));
    }

    #[test]
    fn test_optional_enum_attr_missing_is_none() {
        let resource = make_test_resource(vec![]);
        assert_eq!(optional_enum_attr(&resource, "type"), None);
    }

    #[test]
    fn test_optional_enum_attr_non_enum_shape_is_none() {
        let resource = make_resource_with_value("type", Value::Concrete(ConcreteValue::Bool(true)));
        assert_eq!(optional_enum_attr(&resource, "type"), None);
    }

    #[test]
    fn test_optional_enum_struct_field_string() {
        let resource = make_resource_with_struct_field(
            "certificate_transparency_logging_preference",
            Value::Concrete(ConcreteValue::String("ENABLED".to_string())),
        );
        assert_eq!(
            optional_enum_struct_field(
                &resource,
                "options",
                "certificate_transparency_logging_preference"
            ),
            Some("ENABLED")
        );
    }

    #[test]
    fn test_optional_enum_struct_field_enum_identifier() {
        let resource = make_resource_with_struct_field(
            "certificate_transparency_logging_preference",
            Value::Concrete(ConcreteValue::enum_identifier("enabled")),
        );
        assert_eq!(
            optional_enum_struct_field(
                &resource,
                "options",
                "certificate_transparency_logging_preference"
            ),
            Some("enabled")
        );
    }

    #[test]
    fn test_optional_enum_struct_field_canonical_enum() {
        let resource = make_resource_with_struct_field(
            "validation_method",
            canonical_validation_method_value(),
        );
        assert_eq!(
            optional_enum_struct_field(&resource, "options", "validation_method"),
            Some("DNS")
        );
    }

    #[test]
    fn test_optional_int_struct_field_int() {
        let resource = make_resource_with_struct_field(
            "amazon_side_asn",
            Value::Concrete(ConcreteValue::Int(64512)),
        );
        assert_eq!(
            optional_int_struct_field(&resource, "options", "amazon_side_asn"),
            Some(64512)
        );
    }

    #[test]
    fn test_optional_string_list_struct_field_string_list() {
        let resource = make_resource_with_struct_field(
            "transit_gateway_cidr_blocks",
            Value::Concrete(ConcreteValue::StringList(vec!["10.0.0.0/24".to_string()])),
        );
        assert_eq!(
            optional_string_list_struct_field(&resource, "options", "transit_gateway_cidr_blocks"),
            Some(vec!["10.0.0.0/24".to_string()])
        );
    }

    #[test]
    fn test_optional_string_list_struct_field_list_of_string() {
        let resource = make_resource_with_struct_field(
            "transit_gateway_cidr_blocks",
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::String("10.0.0.0/24".to_string()),
            )])),
        );
        assert_eq!(
            optional_string_list_struct_field(&resource, "options", "transit_gateway_cidr_blocks"),
            Some(vec!["10.0.0.0/24".to_string()])
        );
    }

    #[test]
    fn test_struct_field_helpers_missing_outer_struct_are_none() {
        let resource = make_test_resource(vec![]);
        assert_eq!(
            optional_enum_struct_field(&resource, "options", "dns_support"),
            None
        );
        assert_eq!(
            optional_int_struct_field(&resource, "options", "amazon_side_asn"),
            None
        );
        assert_eq!(
            optional_string_list_struct_field(&resource, "options", "transit_gateway_cidr_blocks"),
            None
        );
    }

    #[test]
    fn test_struct_field_helpers_non_map_outer_struct_are_none() {
        let resource = make_resource_with_value(
            "options",
            Value::Concrete(ConcreteValue::String("not-a-map".to_string())),
        );
        assert_eq!(
            optional_enum_struct_field(&resource, "options", "dns_support"),
            None
        );
        assert_eq!(
            optional_int_struct_field(&resource, "options", "amazon_side_asn"),
            None
        );
        assert_eq!(
            optional_string_list_struct_field(&resource, "options", "transit_gateway_cidr_blocks"),
            None
        );
    }

    #[test]
    fn test_struct_field_helpers_missing_field_are_none() {
        let resource = make_resource_with_value(
            "options",
            Value::Concrete(ConcreteValue::Map(IndexMap::new())),
        );
        assert_eq!(
            optional_enum_struct_field(&resource, "options", "dns_support"),
            None
        );
        assert_eq!(
            optional_int_struct_field(&resource, "options", "amazon_side_asn"),
            None
        );
        assert_eq!(
            optional_string_list_struct_field(&resource, "options", "transit_gateway_cidr_blocks"),
            None
        );
    }

    #[test]
    fn test_struct_field_helpers_wrong_shape_are_none() {
        let enum_resource = make_resource_with_struct_field(
            "dns_support",
            Value::Concrete(ConcreteValue::Bool(true)),
        );
        let int_resource = make_resource_with_struct_field(
            "amazon_side_asn",
            Value::Concrete(ConcreteValue::String("64512".to_string())),
        );
        let list_resource = make_resource_with_struct_field(
            "transit_gateway_cidr_blocks",
            Value::Concrete(ConcreteValue::Int(1)),
        );
        let mixed_list_resource = make_resource_with_struct_field(
            "transit_gateway_cidr_blocks",
            Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                ConcreteValue::Int(1),
            )])),
        );
        assert_eq!(
            optional_enum_struct_field(&enum_resource, "options", "dns_support"),
            None
        );
        assert_eq!(
            optional_int_struct_field(&int_resource, "options", "amazon_side_asn"),
            None
        );
        assert_eq!(
            optional_string_list_struct_field(
                &list_resource,
                "options",
                "transit_gateway_cidr_blocks"
            ),
            None
        );
        assert_eq!(
            optional_string_list_struct_field(
                &mixed_list_resource,
                "options",
                "transit_gateway_cidr_blocks"
            ),
            None
        );
    }

    #[test]
    fn test_sdk_error_message_includes_full_chain() {
        // Simulate a chained error: outer wraps inner
        #[derive(Debug)]
        struct InnerError;
        impl std::fmt::Display for InnerError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "InvalidChangeBatch: record already exists")
            }
        }
        impl std::error::Error for InnerError {}

        #[derive(Debug)]
        struct OuterError(InnerError);
        impl std::fmt::Display for OuterError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "service error")
            }
        }
        impl std::error::Error for OuterError {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let err = OuterError(InnerError);

        // Without DisplayErrorContext, we'd get "ChangeResourceRecordSets failed: service error"
        let bad = format!("ChangeResourceRecordSets failed: {}", err);
        assert_eq!(bad, "ChangeResourceRecordSets failed: service error");

        // With our helper, the full chain is included
        let good = sdk_error_message("ChangeResourceRecordSets failed", &err);
        assert!(
            good.contains("InvalidChangeBatch"),
            "expected full chain, got: {}",
            good
        );
    }

    #[test]
    fn test_sdk_error_message_single_error() {
        #[derive(Debug)]
        struct SimpleError;
        impl std::fmt::Display for SimpleError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "something went wrong")
            }
        }
        impl std::error::Error for SimpleError {}

        let msg = sdk_error_message("CreateBucket failed", &SimpleError);
        assert!(
            msg.starts_with("CreateBucket failed: something went wrong"),
            "expected context + message, got: {}",
            msg
        );
    }

    // Mock service-error type for testing the retry classifier. Implements
    // the minimum traits the helper requires: `ProvideErrorKind` (for
    // SDK-modeled retryability) and `ProvideErrorMetadata` (for the S3
    // `OperationAborted` carve-out).
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_runtime_api::client::result::SdkError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind};

    #[derive(Debug)]
    struct MockServiceError {
        meta: ErrorMetadata,
        retryable: Option<ErrorKind>,
    }

    impl MockServiceError {
        fn new(code: impl Into<String>, retryable: Option<ErrorKind>) -> Self {
            let meta = ErrorMetadata::builder().code(code).build();
            Self { meta, retryable }
        }
    }

    impl std::fmt::Display for MockServiceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.meta)
        }
    }

    impl std::error::Error for MockServiceError {}

    impl ProvideErrorMetadata for MockServiceError {
        fn meta(&self) -> &ErrorMetadata {
            &self.meta
        }
    }

    impl ProvideErrorKind for MockServiceError {
        fn retryable_error_kind(&self) -> Option<ErrorKind> {
            self.retryable
        }
        fn code(&self) -> Option<&str> {
            self.meta.code()
        }
    }

    fn empty_http_response() -> HttpResponse {
        HttpResponse::new(500.try_into().unwrap(), SdkBody::empty())
    }

    fn service_err(
        code: &str,
        retryable: Option<ErrorKind>,
    ) -> SdkError<MockServiceError, HttpResponse> {
        SdkError::service_error(
            MockServiceError::new(code, retryable),
            empty_http_response(),
        )
    }

    #[test]
    fn sdk_marked_transient_error_retries() {
        // Service returned a response the SDK flagged as transient
        // (e.g. RequestTimeout — the bug behind #260). Should retry.
        let err = service_err("RequestTimeout", Some(ErrorKind::TransientError));
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn sdk_marked_throttling_retries() {
        let err = service_err("Throttling", Some(ErrorKind::ThrottlingError));
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn sdk_marked_server_error_retries() {
        let err = service_err("InternalError", Some(ErrorKind::ServerError));
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn unmodeled_client_error_does_not_retry() {
        // Validation / permission / not-found errors must terminate
        // immediately — they will never succeed on a second attempt.
        let err = service_err("ValidationException", None);
        assert!(!is_retryable_sdk_error(&err));
    }

    #[test]
    fn s3_operation_aborted_retries_despite_no_sdk_flag() {
        // S3 CreateBucket / DeleteBucket race: the SDK does not classify
        // OperationAborted (HTTP 409) as retryable, but the AWS docs say
        // to back off because the control plane clears in ~60–90s.
        // See carina-rs/carina-provider-aws#156.
        let err = service_err("OperationAborted", None);
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn s3_request_timeout_retries_despite_no_sdk_flag() {
        // Reproduces carina-rs/carina-provider-aws#272: the real S3 SDK
        // attaches NO `@retryable` trait to any operation error, so the
        // generated `retryable_error_kind()` returns `None` even for
        // RequestTimeout. The fixture passes `None` to match that real
        // behavior (earlier tests passed `Some(..)`, which the actual
        // SDK never produces). The documented-codes carve-out must still
        // classify RequestTimeout as retryable.
        let err = service_err("RequestTimeout", None);
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn s3_slow_down_retries_despite_no_sdk_flag() {
        // S3 request-rate throttling (HTTP 503). Same SDK gap as
        // RequestTimeout — no `@retryable` trait, so the carve-out must
        // cover it.
        let err = service_err("SlowDown", None);
        assert!(is_retryable_sdk_error(&err));
    }

    #[test]
    fn transport_failures_retry() {
        // No HTTP response received → safe to retry idempotent operations.
        let timeout: SdkError<MockServiceError, HttpResponse> =
            SdkError::timeout_error("hit deadline");
        assert!(is_retryable_sdk_error(&timeout));
    }

    /// Zero-delay policy so retry tests do not actually sleep.
    fn test_policy(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            initial_delay_secs: 0,
            max_delay_secs: 0,
        }
    }

    #[tokio::test]
    async fn retry_aws_operation_succeeds_first_try() {
        let result: Result<&str, SdkError<MockServiceError, HttpResponse>> =
            retry_aws_operation("test op", test_policy(3), || async { Ok("success") }).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn retry_aws_operation_retries_transient_then_succeeds() {
        // Models the bug from #260: a transient RequestTimeout is followed
        // by a successful retry once the SDK's retryable flag is honored.
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempt_count.clone();
        let result: Result<&str, SdkError<MockServiceError, HttpResponse>> =
            retry_aws_operation("delete bucket sub-resource", test_policy(3), || {
                let counter = counter.clone();
                async move {
                    let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Err(service_err(
                            "RequestTimeout",
                            Some(ErrorKind::TransientError),
                        ))
                    } else {
                        Ok("deleted")
                    }
                }
            })
            .await;
        assert_eq!(result.unwrap(), "deleted");
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "should retry once after a transient RequestTimeout"
        );
    }

    #[tokio::test]
    async fn retry_aws_operation_non_retryable_fails_immediately() {
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempt_count.clone();
        let result: Result<&str, SdkError<MockServiceError, HttpResponse>> =
            retry_aws_operation("test op", test_policy(3), || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(service_err("ValidationException", None))
                }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "non-retryable errors must terminate after one attempt"
        );
    }

    #[tokio::test]
    async fn retry_aws_operation_exhausts_policy_budget() {
        // A transient error that never clears must retry exactly
        // max_attempts times, then surface the error.
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempt_count.clone();
        let result: Result<&str, SdkError<MockServiceError, HttpResponse>> =
            retry_aws_operation("create S3 bucket", test_policy(8), || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(service_err("OperationAborted", None))
                }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            8,
            "a never-clearing transient error must use the full budget"
        );
    }

    #[test]
    fn default_retry_policy_budget() {
        // Reproduces carina-rs/carina-provider-aws#351: the previous
        // per-site budgets (3 attempts for most S3 deletes, 5 for
        // creates) were exhausted by S3 OperationAborted under load.
        // The single default must be larger.
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 8);
        assert_eq!(p.initial_delay_secs, 5);
        assert_eq!(p.max_delay_secs, 120);
    }

    #[test]
    fn retry_policy_delay_doubles_then_caps() {
        let p = RetryPolicy::default();
        // 5, 10, 20, 40, 80, then capped at 120.
        assert_eq!(p.delay_secs(1), 5);
        assert_eq!(p.delay_secs(2), 10);
        assert_eq!(p.delay_secs(3), 20);
        assert_eq!(p.delay_secs(4), 40);
        assert_eq!(p.delay_secs(5), 80);
        assert_eq!(p.delay_secs(6), 120, "delay must cap at max_delay_secs");
        assert_eq!(p.delay_secs(7), 120);
    }

    #[test]
    fn retry_policy_delay_does_not_overflow() {
        // A large attempt number must not panic via shift/mul overflow.
        let p = RetryPolicy::default();
        assert_eq!(p.delay_secs(100), 120);
    }
}
