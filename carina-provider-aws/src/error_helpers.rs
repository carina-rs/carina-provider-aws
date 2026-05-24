//! Structured AWS-error construction helpers for `ProviderError`.
//!
//! Turns an AWS SDK error into a `carina_core::ProviderError` carrying
//! the response metadata as structured fields (`operation`, `status`,
//! `code`, `request_id`) so the host's renderer can surface them as
//! labeled lines instead of the single-line debug dump that motivated
//! carina-rs/carina#3241.
//!
//! See carina-rs/carina#3242 for the unified design — D3 (one-shot
//! constructor), D4 (WIT fields), D5 (Display fallback). The new
//! `api_error_with_meta` helper here is the provider-side one-shot
//! constructor.
//!
//! `helpers::sdk_error_message` is intentionally left in place; it
//! still serves call sites that hand off non-SDK errors (builder
//! errors from AWS SDK type-state builders, JSON encode errors,
//! validator failures). Those don't implement `ProvideErrorMetadata`
//! and have no HTTP-response shape — for them the host renderer's
//! fallback path (PR carina-rs/carina#3247, design D5) handles the
//! display correctly.
//!
//! Scope: `pub(crate)` on purpose. carina-provider-awscc faces the
//! same shape and would copy this helper verbatim — but that goes
//! against the `carina-aws-types` shared-vs-triplicated convention
//! (see the carina#3242 design discussion). If cross-provider reuse
//! becomes real, lift the helper to `carina-aws-types` rather than
//! promoting it to `pub` here.

use carina_core::provider::ProviderError;

/// Type alias for the `aws-sdk-*` family's `SdkError` shape with the
/// HTTP-response generic pinned. Every `aws-sdk-*` crate's
/// `error::SdkError<E>` re-exports the same smithy
/// `SdkError<E, HttpResponse>`, so this single alias matches every
/// service's `.send().await.err()` shape. Lets the helper signature
/// stay short and the future per-service migrations not re-import
/// the long smithy path.
pub(crate) type AwsSdkError<E> = aws_smithy_runtime_api::client::result::SdkError<
    E,
    aws_smithy_runtime_api::client::orchestrator::HttpResponse,
>;

/// Build a structured `ProviderError::ApiError` from an AWS SDK error.
///
/// Extracts HTTP status, AWS error code, AWS-side message, and request
/// id via the SDK's `ProvideErrorMetadata` + `RequestId` traits. The
/// host renderer (carina-rs/carina#3247) then surfaces them as labeled
/// lines:
///
/// ```text
/// [iam.Roles.admin_access_roles] Failed to list IAM roles: User is not authorized...
///   operation: iam.ListRoles
///   status: 403 AccessDenied
///   request_id: 997aa923-...
/// ```
///
/// `context` is the per-operation human-readable header (the same
/// string the legacy `sdk_error_message` used, e.g. `"Failed to list
/// IAM roles"`). The AWS-side message is appended to it as
/// `"<context>: <aws-message>"` so the header line carries both —
/// PR-1's design (carina#3242 D6) keeps the message in the header,
/// not as a separate labeled line.
///
/// `operation` is the service-qualified API operation
/// (e.g. `"iam.ListRoles"`, `"s3.HeadBucket"`) so the operator can
/// grep CI logs by operation without re-parsing the message.
///
/// The SDK error is preserved as `ProviderError.cause` so
/// transport-level diagnostics (DNS, TLS, network timeouts — none of
/// which populate the HTTP-response fields) still reach the operator
/// via the renderer's appended `cause:` line.
pub fn api_error_with_meta<E, S>(
    context: impl Into<String>,
    operation: S,
    err: AwsSdkError<E>,
) -> ProviderError
where
    S: Into<String>,
    E: std::error::Error
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + Send
        + Sync
        + 'static,
{
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_types::request_id::RequestId;

    let status = err.raw_response().map(|r| r.status().as_u16());
    let code = err.code().map(str::to_owned);
    let aws_message = err.message().map(str::to_owned);
    let request_id = err.request_id().map(str::to_owned);

    let context = context.into();
    let header = match aws_message {
        Some(m) => format!("{context}: {m}"),
        None => context,
    };

    let mut pe = ProviderError::api_error(header).with_operation(operation);
    if let Some(s) = status {
        pe = pe.with_status(s);
    }
    if let Some(c) = code {
        pe = pe.with_code(c);
    }
    if let Some(id) = request_id {
        pe = pe.with_request_id(id);
    }
    pe.with_cause(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_iam::error::SdkError;
    use aws_sdk_iam::operation::list_roles::ListRolesError;
    use aws_sdk_iam::types::error::ServiceFailureException;
    use aws_smithy_runtime_api::http::{Response, StatusCode};
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::metadata::ErrorMetadata;
    use carina_core::provider::ProviderError as CoreProviderError;

    /// carina#3241: a real `SdkError::service_error(...)` round-trip
    /// through `api_error_with_meta` populates the structured fields
    /// the renderer surfaces — proving the extraction wiring works
    /// against a real AWS SDK error type, not just a synthetic mock.
    #[test]
    fn api_error_with_meta_extracts_response_metadata() {
        let meta = ErrorMetadata::builder()
            .code("AccessDenied")
            .message("User: arn:aws:sts::... is not authorized to perform: iam:ListRoles")
            .build();
        let inner = ListRolesError::ServiceFailureException(
            ServiceFailureException::builder().meta(meta).build(),
        );
        let raw = Response::new(StatusCode::try_from(403u16).unwrap(), SdkBody::empty());
        let sdk_err = SdkError::service_error(inner, raw);

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("iam.ListRoles"));
        assert_eq!(detail.status, Some(403));
        assert_eq!(detail.code.as_deref(), Some("AccessDenied"));
        assert!(
            detail.message.contains("Failed to list IAM roles"),
            "header must keep per-operation context, got: {}",
            detail.message
        );
        assert!(
            detail.message.contains("User: arn:aws:sts::"),
            "header must fold in AWS-side message, got: {}",
            detail.message
        );
    }

    /// carina#3241: when the SDK error carries no metadata (rare —
    /// usually a transport-level failure that doesn't reach the
    /// service-error path), the header stays as the per-operation
    /// context only and structured fields are left `None` — letting
    /// the renderer's fallback chain walker surface the underlying
    /// error via the `cause:` line (design D5).
    #[test]
    fn api_error_with_meta_preserves_context_when_no_metadata() {
        // A `ConstructionFailure` is the closest representable shape
        // for "no HTTP response landed". Use a simple `&str` cause.
        let sdk_err: SdkError<ListRolesError> = SdkError::construction_failure("dns lookup failed");

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.message, "Failed to list IAM roles");
        assert_eq!(detail.operation.as_deref(), Some("iam.ListRoles"));
        assert_eq!(detail.status, None);
        assert_eq!(detail.code, None);
        assert_eq!(detail.request_id, None);
        // Cause is preserved so the renderer's fallback path surfaces
        // the transport-level diagnostic.
        assert!(detail.cause.is_some());
    }
}
