//! Shared helper functions for the AWS provider.
//!
//! These reduce boilerplate across EC2 (and other) service implementations.

use indexmap::IndexMap;
use std::future::Future;
use std::time::Duration;

use aws_sdk_ec2::types::{ResourceType, Tag, TagSpecification};
use aws_smithy_types::error::display::DisplayErrorContext;
use tokio::time::sleep;

use carina_core::provider::{PatchOpKind, ProviderError, ProviderResult, UpdatePatch};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

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

/// Extract a required enum attribute from a resource.
///
/// `AwsNormalizer::normalize_desired` rewrites every enum value to its
/// AWS API-canonical bare spelling (`Enabled`, `VPC`, `STANDARD_IA`) at
/// plan time, so the caller can feed the result straight to an SDK
/// builder like `BucketVersioningStatus::from(...)`.
///
/// Equivalent to `require_string_attr` today — the alias historically
/// stripped a DSL namespace prefix, but normalization now happens
/// upstream and namespaces never reach the provider. The helper is
/// kept so caller intent stays readable.
pub fn require_enum_attr(resource: &Resource, attr_name: &str) -> ProviderResult<String> {
    require_string_attr(resource, attr_name)
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

/// Retry an AWS SDK operation with exponential backoff on transient errors.
///
/// Only retries on known transient error patterns (throttling, service errors).
/// Does NOT retry on validation, permission, or resource-not-found errors.
///
/// - `operation_name`: Human-readable name for log messages.
/// - `max_attempts`: Maximum number of attempts (including the first).
/// - `initial_delay_secs`: Delay before the first retry (doubles each attempt, capped at 120s).
/// - `f`: A closure that returns a `Future` producing the SDK result.
pub async fn retry_aws_operation<F, Fut, T, E>(
    operation_name: &str,
    max_attempts: u32,
    initial_delay_secs: u64,
    f: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < max_attempts && is_retryable_error(&e.to_string()) => {
                let delay = std::cmp::min(initial_delay_secs * 2u64.pow(attempt - 1), 120);
                eprintln!(
                    "  Retrying {} (attempt {}/{}): {}",
                    operation_name, attempt, max_attempts, e
                );
                sleep(Duration::from_secs(delay)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Check whether an error message indicates a transient AWS error worth retrying.
fn is_retryable_error(error_msg: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "ThrottlingException",
        "Throttling",
        "Rate exceeded",
        "RequestLimitExceeded",
        "ServiceUnavailable",
        "InternalError",
        "InternalServerError",
        "ServiceException",
        // S3 returns this with HTTP 409 when CreateBucket races against a
        // recent DeleteBucket for the same name; the control plane clears
        // after ~60–90s, so backoff is the right response.
        "OperationAborted",
    ];
    PATTERNS.iter().any(|p| error_msg.contains(p))
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
        let mut resource = Resource::new("route53.RecordSet", "test");
        for (k, v) in attrs {
            resource.attributes.insert(
                k.to_string(),
                Value::Concrete(ConcreteValue::String(v.to_string())),
            );
        }
        resource
    }

    #[test]
    fn test_apply_patch_to_state_add_replace_remove() {
        use carina_core::provider::{PatchOp, PatchOpKind, UpdatePatch};
        use std::collections::HashMap;

        // from-state has two attrs; patch adds one, replaces one, removes one.
        let id = ResourceId::with_provider("aws", "ec2.Vpc", "test");
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
    fn test_require_enum_attr_returns_canonical_value() {
        // After AwsNormalizer::normalize_desired runs, enum attributes
        // carry the bare AWS API-canonical spelling. require_enum_attr
        // just passes it through.
        let resource = make_test_resource(vec![("type", "A")]);
        assert_eq!(require_enum_attr(&resource, "type").unwrap(), "A");
    }

    #[test]
    fn test_require_enum_attr_plain_value() {
        let resource = make_test_resource(vec![("type", "AAAA")]);
        assert_eq!(require_enum_attr(&resource, "type").unwrap(), "AAAA");
    }

    #[test]
    fn test_require_enum_attr_missing() {
        let resource = make_test_resource(vec![]);
        assert!(require_enum_attr(&resource, "type").is_err());
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

    #[test]
    fn test_is_retryable_error_throttling() {
        assert!(is_retryable_error("ThrottlingException: Rate exceeded"));
        assert!(is_retryable_error("Throttling: request limit"));
        assert!(is_retryable_error("Rate exceeded for API call"));
        assert!(is_retryable_error("RequestLimitExceeded"));
    }

    #[test]
    fn test_is_retryable_error_service_errors() {
        assert!(is_retryable_error("ServiceUnavailable: try again"));
        assert!(is_retryable_error("InternalError occurred"));
        assert!(is_retryable_error("InternalServerError"));
        assert!(is_retryable_error("ServiceException: transient"));
    }

    #[test]
    fn test_is_retryable_error_non_retryable() {
        assert!(!is_retryable_error("ValidationError: invalid parameter"));
        assert!(!is_retryable_error("AccessDeniedException: not authorized"));
        assert!(!is_retryable_error("ResourceNotFoundException: not found"));
        assert!(!is_retryable_error("InvalidParameterValue"));
    }

    #[test]
    fn test_is_retryable_error_s3_operation_aborted() {
        // S3 CreateBucket can return OperationAborted shortly after the same
        // name was deleted; the right behavior is to back off and retry, not
        // propagate. See carina-rs/carina-provider-aws#156.
        assert!(is_retryable_error(
            "OperationAborted: A conflicting conditional operation is currently in progress against this resource. Please try again."
        ));
        assert!(is_retryable_error(
            "service error: unhandled error (OperationAborted): ..."
        ));
    }

    #[tokio::test]
    async fn test_retry_aws_operation_succeeds_first_try() {
        let result: Result<&str, String> =
            retry_aws_operation("test op", 3, 1, || async { Ok("success") }).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_aws_operation_retries_operation_aborted_then_succeeds() {
        // Models the S3 CreateBucket / DeleteBucket race from #156: the first
        // attempt sees OperationAborted, the second one goes through once the
        // control plane has cleared the prior delete.
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempt_count.clone();
        let result: Result<&str, String> = retry_aws_operation("create S3 bucket", 3, 1, || {
            let counter = counter.clone();
            async move {
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Err("OperationAborted: A conflicting conditional operation is currently in progress against this resource. Please try again.".to_string())
                } else {
                    Ok("created")
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), "created");
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "should retry once after OperationAborted"
        );
    }

    #[tokio::test]
    async fn test_retry_aws_operation_non_retryable_fails_immediately() {
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempt_count.clone();
        let result: Result<&str, String> = retry_aws_operation("test op", 3, 1, || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("ValidationError: bad input".to_string())
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "should not retry non-retryable errors"
        );
    }
}
