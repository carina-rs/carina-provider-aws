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
use thiserror::Error;

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

#[derive(Debug)]
struct AwsServiceMessage(Option<String>);

impl AwsServiceMessage {
    fn new<E>(message: Option<String>, source: &AwsSdkError<E>) -> Self
    where
        E: std::error::Error + 'static,
    {
        Self(message.filter(|message| !error_chain_contains(source, message)))
    }
}

impl std::fmt::Display for AwsServiceMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(message) => f.write_str(message),
            None => f.write_str("AWS SDK request failed"),
        }
    }
}

fn error_chain_contains(mut error: &(dyn std::error::Error + 'static), needle: &str) -> bool {
    loop {
        if error.to_string().contains(needle) {
            return true;
        }
        match error.source() {
            Some(source) => error = source,
            None => return false,
        }
    }
}

/// AWS diagnostic attached beneath the operation-specific provider context.
///
/// The SDK error remains a source because transport and modeled service details
/// live in its source chain. The wrapper renders the AWS-side message only when
/// that chain does not already render it; `ProviderError` owns the traversal.
#[derive(Debug, Error)]
#[error("{message}")]
struct AwsServiceErrorCause<E>
where
    E: std::error::Error + 'static,
{
    message: AwsServiceMessage,
    #[source]
    source: AwsSdkError<E>,
}

/// AWS diagnostic with dynamic typed subject data below a static context.
#[derive(Debug, Error)]
#[error("{subject}: {message}")]
struct AwsServiceErrorWithSubjectCause<E, Subject>
where
    E: std::error::Error + 'static,
    Subject: std::fmt::Debug + std::fmt::Display,
{
    subject: Subject,
    message: AwsServiceMessage,
    #[source]
    source: AwsSdkError<E>,
}

/// Build a structured `ProviderError::ApiError` from an AWS SDK error.
///
/// Extracts HTTP status, AWS error code, AWS-side message, and request
/// id via the SDK's `ProvideErrorMetadata` + `RequestId` traits. The
/// host renderer (carina-rs/carina#3247) then surfaces them as labeled
/// lines followed by the typed cause:
///
/// ```text
/// Failed to list IAM roles
///   operation: iam.ListRoles
///   status: 403 AccessDenied
///   request_id: test-request-id
///   cause: User: arn:aws:sts::... is not authorized to perform: iam:ListRoles: service error: ServiceFailureException: ServiceFailureException
/// ```
///
/// `context` is the per-operation human-readable header (the same
/// string the legacy `sdk_error_message` used, e.g. `"Failed to list
/// IAM roles"`). It remains the first-line `ProviderError` context.
/// The AWS-side message lives in the typed cause and is omitted there
/// when the SDK source chain already renders it.
///
/// `operation` is the service-qualified API operation
/// (e.g. `"iam.ListRoles"`, `"s3.HeadBucket"`) so the operator can
/// grep CI logs by operation without re-parsing the message.
///
/// The SDK error is preserved as the typed cause's source so
/// transport-level diagnostics (DNS, TLS, network timeouts — none of
/// which populate the HTTP-response fields) still reach the operator
/// via the renderer's appended `cause:` line.
pub fn api_error_with_meta<E, S>(
    context: &'static str,
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
    api_error_with_meta_cause(context, operation, err, |message, source| {
        AwsServiceErrorCause { message, source }
    })
}

/// Build a structured AWS error whose cause also carries a dynamic typed
/// subject. The `context` remains static and first-line; subject data is
/// rendered exactly once below it.
pub(crate) fn api_error_with_meta_and_subject<E, S, Subject>(
    context: &'static str,
    operation: S,
    subject: Subject,
    err: AwsSdkError<E>,
) -> ProviderError
where
    S: Into<String>,
    Subject: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
    E: std::error::Error
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + Send
        + Sync
        + 'static,
{
    api_error_with_meta_cause(context, operation, err, |message, source| {
        AwsServiceErrorWithSubjectCause {
            subject,
            message,
            source,
        }
    })
}

fn api_error_with_meta_cause<E, S, Cause>(
    context: &'static str,
    operation: S,
    err: AwsSdkError<E>,
    make_cause: impl FnOnce(AwsServiceMessage, AwsSdkError<E>) -> Cause,
) -> ProviderError
where
    S: Into<String>,
    Cause: std::error::Error + Send + Sync + 'static,
    E: std::error::Error
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + Send
        + Sync
        + 'static,
{
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_types::request_id::RequestId;

    let status = err
        .raw_response()
        .map(|response| response.status().as_u16());
    let code = err.code().map(str::to_owned);
    let aws_message = AwsServiceMessage::new(err.message().map(str::to_owned), &err);
    let request_id = err.request_id().map(str::to_owned);
    let cause = make_cause(aws_message, err);

    let mut pe = if code.as_deref().is_some_and(is_not_found_error_code) || status == Some(404) {
        ProviderError::not_found_caused_by(context, cause)
    } else {
        ProviderError::api_error_caused_by(context, cause)
    }
    .with_operation(operation);
    if let Some(s) = status {
        pe = pe.with_status(s);
    }
    if let Some(c) = code {
        pe = pe.with_code(c);
    }
    if let Some(id) = request_id {
        pe = pe.with_request_id(id);
    }
    pe
}

fn is_not_found_error_code(code: &str) -> bool {
    matches!(
        code,
        "ResourceNotFoundException"
            | "NoSuchEntity"
            | "NoSuchBucket"
            | "NoSuchEntityException"
            | "InvalidGroup.NotFound"
            | "InvalidGroupId.NotFound"
            | "InvalidRoute.NotFound"
            | "InvalidRouteTableID.NotFound"
            | "InvalidVpcID.NotFound"
            | "InvalidSubnetID.NotFound"
            | "InvalidInternetGatewayID.NotFound"
            | "InvalidNatGatewayID.NotFound"
            | "InvalidAllocationID.NotFound"
            | "InvalidVpcPeeringConnectionID.NotFound"
            | "InvalidVpnGatewayID.NotFound"
            | "InvalidTransitGatewayID.NotFound"
            | "InvalidPrefixListID.NotFound"
            | "InvalidIpamID.NotFound"
            | "InvalidIpamPoolId.NotFound"
            | "InvalidVpcEndpointId.NotFound"
            | "InvalidNetworkInterfaceID.NotFound"
            | "InvalidGatewayID.NotFound"
            | "InvalidFlowLogId.NotFound"
            | "ResourceNotFoundFault"
            | "ResourceNotFoundError"
    )
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

    fn list_roles_service_error_with_meta(
        status: u16,
        code: Option<&str>,
        message: Option<&str>,
    ) -> SdkError<ListRolesError> {
        let mut meta = ErrorMetadata::builder();
        if let Some(code) = code {
            meta = meta.code(code);
        }
        if let Some(message) = message {
            meta = meta.message(message);
        }

        let inner = ListRolesError::ServiceFailureException(
            ServiceFailureException::builder()
                .meta(meta.build())
                .build(),
        );
        let mut raw = Response::new(StatusCode::try_from(status).unwrap(), SdkBody::empty());
        raw.headers_mut()
            .insert("x-amzn-requestid", "test-request-id");
        SdkError::service_error(inner, raw)
    }

    /// carina#3241: a real `SdkError::service_error(...)` round-trip
    /// through `api_error_with_meta` populates the structured fields
    /// the renderer surfaces — proving the extraction wiring works
    /// against a real AWS SDK error type, not just a synthetic mock.
    #[test]
    fn api_error_with_meta_extracts_response_metadata() {
        const AWS_MESSAGE: &str =
            "User: arn:aws:sts::... is not authorized to perform: iam:ListRoles";
        let sdk_err =
            list_roles_service_error_with_meta(403, Some("AccessDenied"), Some(AWS_MESSAGE));

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("iam.ListRoles"));
        assert_eq!(detail.status, Some(403));
        assert_eq!(detail.code.as_deref(), Some("AccessDenied"));
        assert_eq!(detail.request_id.as_deref(), Some("test-request-id"));
        assert_eq!(detail.context(), "Failed to list IAM roles");
        assert!(detail.cause().is_some());

        let rendered = pe.to_string();
        assert_eq!(
            rendered,
            format!(
                "Failed to list IAM roles\n  operation: iam.ListRoles\n  status: 403 AccessDenied\n  request_id: test-request-id\n  cause: {AWS_MESSAGE}: service error: ServiceFailureException: ServiceFailureException"
            )
        );
        assert_eq!(
            rendered.matches(AWS_MESSAGE).count(),
            1,
            "rendered error must include the AWS message exactly once: {rendered}"
        );

        let wire = pe.to_wire();
        assert_eq!(
            wire.kind,
            carina_provider_protocol::types::ProviderErrorKind::ApiError
        );
        assert_eq!(wire.status, Some(403));
        assert_eq!(wire.code.as_deref(), Some("AccessDenied"));
        assert_eq!(wire.request_id.as_deref(), Some("test-request-id"));
        let wire_rendered = format!(
            "{}: {}",
            wire.message,
            wire.cause.as_deref().unwrap_or_default()
        );
        assert!(wire_rendered.contains("Failed to list IAM roles"));
        assert_eq!(
            wire_rendered.matches(AWS_MESSAGE).count(),
            1,
            "wire error must include the AWS message exactly once: {wire_rendered}"
        );
    }

    #[test]
    fn api_error_with_meta_does_not_repeat_message_from_sdk_source_chain() {
        const AWS_MESSAGE: &str = "message already rendered by the SDK source chain";
        let inner = ListRolesError::generic(
            ErrorMetadata::builder()
                .code("AccessDenied")
                .message(AWS_MESSAGE)
                .build(),
        );
        let raw = Response::new(StatusCode::try_from(403).unwrap(), SdkBody::empty());
        let sdk_err = SdkError::service_error(inner, raw);

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let rendered = pe.to_string();
        assert_eq!(
            rendered.matches(AWS_MESSAGE).count(),
            1,
            "rendered error must not repeat an SDK-rendered message: {rendered}"
        );

        let wire = pe.to_wire();
        let wire_rendered = format!(
            "{}: {}",
            wire.message,
            wire.cause.as_deref().unwrap_or_default()
        );
        assert_eq!(
            wire_rendered.matches(AWS_MESSAGE).count(),
            1,
            "wire error must not repeat an SDK-rendered message: {wire_rendered}"
        );
    }

    /// carina#3241: when the SDK error carries no metadata (rare —
    /// usually a transport-level failure that doesn't reach the
    /// service-error path), structured response fields are left `None`
    /// and the typed cause keeps both the per-operation context and the
    /// underlying transport diagnostic.
    #[test]
    fn api_error_with_meta_preserves_context_when_no_metadata() {
        // A `ConstructionFailure` is the closest representable shape
        // for "no HTTP response landed". Use a simple `&str` cause.
        let sdk_err: SdkError<ListRolesError> = SdkError::construction_failure("dns lookup failed");

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.context(), "Failed to list IAM roles");
        assert_eq!(detail.operation.as_deref(), Some("iam.ListRoles"));
        assert_eq!(detail.status, None);
        assert_eq!(detail.code, None);
        assert_eq!(detail.request_id, None);
        // Cause is preserved so the renderer's fallback path surfaces
        // the transport-level diagnostic.
        assert!(detail.cause().is_some());
        assert_eq!(
            pe.to_string(),
            "Failed to list IAM roles\n  operation: iam.ListRoles\n  cause: AWS SDK request failed: failed to construct request: dns lookup failed"
        );
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_acm_resource_not_found() {
        let sdk_err = list_roles_service_error_with_meta(
            400,
            Some("ResourceNotFoundException"),
            Some("resource not found"),
        );

        let pe = api_error_with_meta(
            "Failed to describe ACM certificate",
            "acm.DescribeCertificate",
            sdk_err,
        );
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("acm.DescribeCertificate"));
        assert_eq!(detail.status, Some(400));
        assert_eq!(detail.code.as_deref(), Some("ResourceNotFoundException"));
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_iam_no_such_entity() {
        let sdk_err =
            list_roles_service_error_with_meta(404, Some("NoSuchEntity"), Some("role not found"));

        let pe = api_error_with_meta("Failed to get IAM role", "iam.GetRole", sdk_err);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("iam.GetRole"));
        assert_eq!(detail.status, Some(404));
        assert_eq!(detail.code.as_deref(), Some("NoSuchEntity"));
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_ec2_invalid_group() {
        let sdk_err = list_roles_service_error_with_meta(
            400,
            Some("InvalidGroup.NotFound"),
            Some("security group not found"),
        );

        let pe = api_error_with_meta(
            "Failed to describe security groups",
            "ec2.DescribeSecurityGroups",
            sdk_err,
        );
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(
            detail.operation.as_deref(),
            Some("ec2.DescribeSecurityGroups")
        );
        assert_eq!(detail.status, Some(400));
        assert_eq!(detail.code.as_deref(), Some("InvalidGroup.NotFound"));
    }

    #[test]
    fn api_error_with_meta_returns_api_error_for_other_codes() {
        let sdk_err = list_roles_service_error_with_meta(
            400,
            Some("ValidationException"),
            Some("invalid input"),
        );

        let pe = api_error_with_meta("Failed to list IAM roles", "iam.ListRoles", sdk_err);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("iam.ListRoles"));
        assert_eq!(detail.status, Some(400));
        assert_eq!(detail.code.as_deref(), Some("ValidationException"));
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_http_404() {
        let sdk_err =
            list_roles_service_error_with_meta(404, None, Some("not found without a code"));

        let pe = api_error_with_meta("Failed to head S3 bucket", "s3.HeadBucket", sdk_err);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(detail.operation.as_deref(), Some("s3.HeadBucket"));
        assert_eq!(detail.status, Some(404));
        assert_eq!(detail.code, None);
    }
}
