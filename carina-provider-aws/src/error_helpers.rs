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
//! Errors without an HTTP response or `ProvideErrorMetadata` (such as SDK
//! builder errors) use `ProviderError::*_caused_by` directly. Their context
//! contains only provider-authored text, and the typed cause preserves the
//! source chain for the host renderer.
//!
//! Scope: `pub(crate)` on purpose. carina-provider-awscc faces the
//! same shape and would copy this helper verbatim — but that goes
//! against the `carina-aws-types` shared-vs-triplicated convention
//! (see the carina#3242 design discussion). If cross-provider reuse
//! becomes real, lift the helper to `carina-aws-types` rather than
//! promoting it to `pub` here.

use carina_core::provider::ProviderError;
use carina_core::provider_text::{ProviderContext, Retryability};
use thiserror::Error;

use carina_aws_types::{Idempotency, SendClassification, Transience};

use crate::aws_operation::DecidedOperationError;

const OUTCOME_UNKNOWN_NOTE: &str =
    "outcome unknown: the request may have been applied; check before re-applying";

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

/// An AWS per-item rejection. Empty codes are absent; messages stay as returned.
#[derive(Debug)]
pub(crate) struct AwsItemFailure {
    resource_id: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

impl AwsItemFailure {
    pub(crate) fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

impl From<&carina_aws_types::sdk::ec2::types::UnsuccessfulItem> for AwsItemFailure {
    fn from(item: &carina_aws_types::sdk::ec2::types::UnsuccessfulItem) -> Self {
        Self {
            resource_id: item.resource_id().map(str::to_owned),
            code: item
                .error()
                .and_then(|error| error.code())
                .filter(|code| !code.is_empty())
                .map(str::to_owned),
            message: item
                .error()
                .and_then(|error| error.message())
                .map(str::to_owned),
        }
    }
}

impl std::fmt::Display for AwsItemFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(resource_id) = &self.resource_id {
            write!(f, "{resource_id}: ")?;
        }
        match (
            self.code(),
            self.message().filter(|message| !message.is_empty()),
        ) {
            (Some(code), Some(message)) => write!(f, "{code}: {message}"),
            (Some(code), None) => f.write_str(code),
            (None, Some(message)) => f.write_str(message),
            (None, None) => f.write_str("no message"),
        }
    }
}

impl std::error::Error for AwsItemFailure {}

/// Every unsuccessful item from an AWS response, guaranteed to be non-empty.
#[derive(Debug)]
pub(crate) struct AwsItemFailures {
    first: AwsItemFailure,
    rest: Vec<AwsItemFailure>,
}

impl AwsItemFailures {
    pub(crate) fn from_items(
        items: &[carina_aws_types::sdk::ec2::types::UnsuccessfulItem],
    ) -> Option<Self> {
        let (first, rest) = items.split_first()?;
        Some(Self {
            first: first.into(),
            rest: rest.iter().map(AwsItemFailure::from).collect(),
        })
    }

    #[cfg(test)]
    pub(crate) fn first(&self) -> &AwsItemFailure {
        &self.first
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &AwsItemFailure> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    fn classification(&self) -> AwsErrorClass {
        // An uncoded rejection prevents retrying the whole batch, even if the
        // remaining codes would allow it. Keep the selected code diagnostic.
        if self.iter().any(|failure| failure.code().is_none()) {
            return AwsErrorClass::Permanent;
        }
        let code = self.code();
        AwsErrorClass::classify(code, Transience::of_code(code), None, None)
    }

    fn code(&self) -> Option<&str> {
        self.iter()
            .filter_map(AwsItemFailure::code)
            .min_by_key(|&code| {
                let tier = match AwsErrorClass::classify(
                    Some(code),
                    Transience::of_code(Some(code)),
                    None,
                    None,
                ) {
                    AwsErrorClass::Permanent => 0,
                    AwsErrorClass::DependencyBlocked => 1,
                    AwsErrorClass::Transient => 2,
                    AwsErrorClass::NotFound => 3,
                };
                (tier, code)
            })
    }
}

impl std::fmt::Display for AwsItemFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.first)?;
        for failure in &self.rest {
            write!(f, "; {failure}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AwsItemFailures {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AwsErrorClass {
    NotFound,
    Transient,
    DependencyBlocked,
    Permanent,
}

impl AwsErrorClass {
    /// Classify AWS metadata once, before diagnostics are made opaque.
    /// Item failures have no operation or message fallback.
    fn classify(
        code: Option<&str>,
        transience: Transience,
        operation: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        if code.is_some_and(is_not_found_error_code) {
            Self::NotFound
        } else if transience == Transience::Transient {
            Self::Transient
        } else if code == Some("DependencyViolation")
            || (operation
                .and_then(|operation| operation.rsplit_once('.'))
                .is_some_and(|(_, name)| {
                    // Detaching an attachment and releasing an address are
                    // delete operations too; the context text is irrelevant.
                    name.starts_with("Delete")
                        || name.starts_with("Detach")
                        || name.starts_with("Release")
                })
                && message.is_some_and(|message| {
                    [
                        "has dependent object",
                        "has a dependent object",
                        "resource has dependencies",
                        "mapped public address",
                    ]
                    .iter()
                    .any(|marker| message.contains(marker))
                }))
        {
            Self::DependencyBlocked
        } else {
            Self::Permanent
        }
    }

    fn retryability(self) -> Retryability {
        match self {
            Self::Transient => Retryability::Transient,
            Self::DependencyBlocked => Retryability::DependencyBlocked,
            Self::NotFound | Self::Permanent => Retryability::Unclassified,
        }
    }
}

/// Attach all per-item AWS diagnostics beneath a provider-authored context.
pub(crate) fn api_error_with_item_failure<E: DecidedOperationError>(
    context: impl Into<ProviderContext>,
    failures: AwsItemFailures,
    request_id: Option<&str>,
) -> ProviderError {
    let class = failures.classification();
    item_failure_error::<E>(context, failures, request_id, class)
}

/// Some items were created, so the issued operation must never be retried or
/// treated as missing. Keep the item codes as diagnostics only.
pub(crate) fn api_error_with_partial_item_failure<E: DecidedOperationError>(
    context: impl Into<ProviderContext>,
    failures: AwsItemFailures,
    request_id: Option<&str>,
) -> ProviderError {
    item_failure_error::<E>(context, failures, request_id, AwsErrorClass::Permanent)
}

fn item_failure_error<E: DecidedOperationError>(
    context: impl Into<ProviderContext>,
    failures: AwsItemFailures,
    request_id: Option<&str>,
    class: AwsErrorClass,
) -> ProviderError {
    let code = failures.code().map(str::to_owned);
    let retryability = class.retryability();
    let mut error = api_error_with_code(context, code, None, class, retryability, failures)
        .with_operation(E::NAME);
    if let Some(request_id) = request_id {
        error = error.with_request_id(request_id);
    }
    error
}

fn api_error_with_code(
    context: impl Into<ProviderContext>,
    code: Option<String>,
    status: Option<u16>,
    class: AwsErrorClass,
    retryability: Retryability,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> ProviderError {
    let mut error = if class == AwsErrorClass::NotFound {
        ProviderError::not_found_caused_by(context, cause)
    } else {
        ProviderError::api_error_caused_by(context, cause)
    };
    if let Some(status) = status {
        error = error.with_status(status);
    }
    if let Some(code) = code {
        error = error.with_code(code);
    }
    error.with_retryability(retryability)
}

/// A failed send together with its complete send classification.
///
/// The SDK error is boxed so retry wrapper results keep a small `Err` variant
/// even when generated service errors make `SdkError` large.
#[derive(Debug)]
pub(crate) struct SendFailure<E> {
    classification: SendClassification,
    error: Box<AwsSdkError<E>>,
}

impl<E> SendFailure<E> {
    pub(crate) fn new(classification: SendClassification, error: AwsSdkError<E>) -> Self {
        Self {
            classification,
            error: Box::new(error),
        }
    }

    pub(crate) fn classification(&self) -> SendClassification {
        self.classification
    }

    pub(crate) fn into_parts(self) -> (SendClassification, AwsSdkError<E>) {
        (self.classification, *self.error)
    }
}

impl<E> std::ops::Deref for SendFailure<E> {
    type Target = AwsSdkError<E>;

    fn deref(&self) -> &Self::Target {
        self.error.as_ref()
    }
}

pub(crate) trait IntoSendFailure {
    type Error;

    fn into_send_failure(self) -> SendFailure<Self::Error>;
}

impl<E> IntoSendFailure for SendFailure<E> {
    type Error = E;

    fn into_send_failure(self) -> SendFailure<E> {
        self
    }
}

impl<E> IntoSendFailure for AwsSdkError<E>
where
    E: aws_smithy_types::error::metadata::ProvideErrorMetadata
        + aws_smithy_types::retry::ProvideErrorKind
        + std::error::Error
        + 'static,
{
    type Error = E;

    fn into_send_failure(self) -> SendFailure<E> {
        let classification = SendClassification::of(&self);
        SendFailure::new(classification, self)
    }
}

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

#[derive(Debug, Error)]
#[error("{OUTCOME_UNKNOWN_NOTE}")]
struct OutcomeUnknownCause<C>
where
    C: std::error::Error + 'static,
{
    #[source]
    source: C,
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
/// string, e.g. `"Failed to list IAM roles"`). It remains the first-line
/// `ProviderError` context.
/// The AWS-side message lives in the typed cause and is omitted there
/// when the SDK source chain already renders it.
///
/// The SDK error is preserved as the typed cause's source so
/// transport-level diagnostics (DNS, TLS, network timeouts — none of
/// which populate the HTTP-response fields) still reach the operator
/// via the renderer's appended `cause:` line.
pub(crate) fn api_error_with_meta<F>(
    context: impl Into<ProviderContext>,
    failure: F,
) -> ProviderError
where
    F: IntoSendFailure,
    F::Error: std::error::Error
        + DecidedOperationError
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + aws_smithy_types::retry::ProvideErrorKind
        + Send
        + Sync
        + 'static,
{
    let (classification, error) = failure.into_send_failure().into_parts();
    api_error_with_meta_cause(context, classification, error, |message, source| {
        AwsServiceErrorCause { message, source }
    })
}

/// Build a structured AWS error whose cause also carries a dynamic typed
/// subject. The `context` remains static and first-line; subject data is
/// rendered exactly once below it.
pub(crate) fn api_error_with_meta_and_subject<E, Subject>(
    context: &'static str,
    subject: Subject,
    err: AwsSdkError<E>,
) -> ProviderError
where
    Subject: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
    E: std::error::Error
        + DecidedOperationError
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + aws_smithy_types::retry::ProvideErrorKind
        + Send
        + Sync
        + 'static,
{
    let classification = SendClassification::of(&err);
    api_error_with_meta_cause(context, classification, err, |message, source| {
        AwsServiceErrorWithSubjectCause {
            subject,
            message,
            source,
        }
    })
}

fn api_error_with_meta_cause<E, Cause>(
    context: impl Into<ProviderContext>,
    classification: SendClassification,
    err: AwsSdkError<E>,
    make_cause: impl FnOnce(AwsServiceMessage, AwsSdkError<E>) -> Cause,
) -> ProviderError
where
    Cause: std::error::Error + Send + Sync + 'static,
    E: std::error::Error
        + DecidedOperationError
        + aws_smithy_types::error::metadata::ProvideErrorMetadata
        + aws_smithy_types::retry::ProvideErrorKind
        + Send
        + Sync
        + 'static,
{
    use aws_smithy_types::error::metadata::ProvideErrorMetadata;
    use aws_types::request_id::RequestId;

    let operation = E::NAME.to_string();
    let append_outcome_unknown = match classification {
        SendClassification::MaybeSent { .. } => E::IDEMPOTENCY == Idempotency::NotIdempotent,
        SendClassification::NotSent(_) | SendClassification::Rejected => false,
    };
    let status = err
        .raw_response()
        .map(|response| response.status().as_u16());
    let code = err.code().map(str::to_owned);
    let aws_message = AwsServiceMessage::new(err.message().map(str::to_owned), &err);
    let request_id = err.request_id().map(str::to_owned);
    let transience = Transience::of(&err);
    let class = if status == Some(404) {
        AwsErrorClass::NotFound
    } else {
        AwsErrorClass::classify(code.as_deref(), transience, Some(E::NAME), err.message())
    };
    // SDK retryability is independent of kind: a NotFound response can still
    // carry a transient modeled error, code, or HTTP status.
    let retryability = match transience {
        Transience::Transient => Retryability::Transient,
        Transience::Permanent => class.retryability(),
    };
    let cause = make_cause(aws_message, err);

    let mut pe = if append_outcome_unknown {
        api_error_with_code(
            context,
            code,
            status,
            class,
            retryability,
            OutcomeUnknownCause { source: cause },
        )
    } else {
        api_error_with_code(context, code, status, class, retryability, cause)
    }
    .with_operation(operation);
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
pub(crate) fn unsuccessful_item(
    resource_id: Option<&str>,
    code: Option<&str>,
    message: Option<&str>,
) -> carina_aws_types::sdk::ec2::types::UnsuccessfulItem {
    use carina_aws_types::sdk::ec2::types::{UnsuccessfulItem, UnsuccessfulItemError};

    UnsuccessfulItem::builder()
        .set_resource_id(resource_id.map(str::to_owned))
        .set_error((code.is_some() || message.is_some()).then(|| {
            UnsuccessfulItemError::builder()
                .set_code(code.map(str::to_owned))
                .set_message(message.map(str::to_owned))
                .build()
        }))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::http::{Response, StatusCode};
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::metadata::ErrorMetadata;
    use carina_aws_types::sdk::ec2::operation::delete_flow_logs::DeleteFlowLogsError;
    use carina_aws_types::sdk::iam::error::SdkError;
    use carina_aws_types::sdk::iam::operation::list_roles::ListRolesError;
    use carina_aws_types::sdk::iam::types::error::ServiceFailureException;
    use carina_core::provider::HostError as CoreProviderError;
    use carina_core::test_utils::provider_text::BindProviderText as _;

    #[test]
    fn item_failure_preserves_optional_aws_data() {
        for (code, message) in [
            (None, None),
            (Some("403"), None),
            (None, Some("AWS diagnostic")),
            (Some(""), Some("")),
            (Some("InvalidParameterValue"), Some("AWS diagnostic")),
        ] {
            let item = unsuccessful_item(None, code, message);
            let failure = AwsItemFailure::from(&item);
            assert_eq!(failure.code(), code.filter(|code| !code.is_empty()));
            assert_eq!(failure.message(), message);
            assert_eq!(failure.resource_id, None);
        }
        let failure = AwsItemFailure::from(&unsuccessful_item(None, None, None));
        assert_eq!(failure.code(), None);
        assert_eq!(failure.message(), None);
    }

    #[test]
    fn item_failures_require_at_least_one_item() {
        assert!(AwsItemFailures::from_items(&[]).is_none());
        let failures = AwsItemFailures::from_items(&[unsuccessful_item(None, None, None)]).unwrap();
        assert_eq!(failures.iter().count(), 1);
        assert!(failures.first().code().is_none());
        assert!(failures.first().message().is_none());
    }

    #[test]
    fn item_failure_preserves_aws_resource_id() {
        let item = unsuccessful_item(
            Some("vpc-b"),
            Some("DependencyViolation"),
            Some("still in use"),
        );
        let failure = AwsItemFailure::from(&item);
        assert_eq!(failure.resource_id.as_deref(), Some("vpc-b"));
        assert_eq!(failure.code(), Some("DependencyViolation"));
        assert_eq!(failure.message(), Some("still in use"));
        assert_eq!(
            failure.to_string(),
            "vpc-b: DependencyViolation: still in use"
        );
    }

    #[test]
    fn item_failures_display_every_code_and_optional_message() {
        let items = [
            (
                Some("vpc-a"),
                Some("InvalidParameterValue"),
                Some("bad input"),
            ),
            (Some("vpc-b"), Some("DependencyViolation"), None),
            (None, Some("RequestLimitExceeded"), Some("try later")),
            (None, Some("403"), None),
            (None, Some("403"), Some("")),
            (None, None, Some("AWS diagnostic")),
            (None, None, None),
            (Some("vpc-c"), None, Some("")),
        ]
        .map(|(resource_id, code, message)| unsuccessful_item(resource_id, code, message));
        let failures = AwsItemFailures::from_items(&items).unwrap();
        assert_eq!(
            failures.to_string(),
            "vpc-a: InvalidParameterValue: bad input; vpc-b: DependencyViolation; RequestLimitExceeded: try later; 403; 403; AWS diagnostic; no message; vpc-c: no message"
        );
    }

    #[test]
    fn item_failures_code_skips_uncoded_and_prefers_non_not_found() {
        const NOT_FOUND: &str = "InvalidFlowLogId.NotFound";
        for (codes, expected, class) in [
            (
                [Some(NOT_FOUND), None, Some("DependencyViolation")],
                Some("DependencyViolation"),
                AwsErrorClass::Permanent,
            ),
            (
                [None, Some(NOT_FOUND), None],
                Some(NOT_FOUND),
                AwsErrorClass::Permanent,
            ),
            (
                [Some(NOT_FOUND), None, Some(NOT_FOUND)],
                Some(NOT_FOUND),
                AwsErrorClass::Permanent,
            ),
            ([None, Some(""), None], None, AwsErrorClass::Permanent),
            (
                [
                    Some("InvalidParameterValue"),
                    None,
                    Some("DependencyViolation"),
                ],
                Some("InvalidParameterValue"),
                AwsErrorClass::Permanent,
            ),
            (
                [Some("RequestLimitExceeded"), None, Some("RequestTimeout")],
                Some("RequestLimitExceeded"),
                AwsErrorClass::Permanent,
            ),
            (
                [
                    Some("RequestTimeout"),
                    Some("RequestLimitExceeded"),
                    Some("RequestTimeout"),
                ],
                Some("RequestLimitExceeded"),
                AwsErrorClass::Transient,
            ),
            (
                [
                    Some("InvalidVpcID.NotFound"),
                    Some(NOT_FOUND),
                    Some(NOT_FOUND),
                ],
                Some(NOT_FOUND),
                AwsErrorClass::NotFound,
            ),
            (
                [
                    Some(NOT_FOUND),
                    Some("RequestLimitExceeded"),
                    Some(NOT_FOUND),
                ],
                Some("RequestLimitExceeded"),
                AwsErrorClass::Transient,
            ),
            (
                [
                    Some(NOT_FOUND),
                    Some("ServiceUnavailable"),
                    Some("RequestLimitExceeded"),
                ],
                Some("RequestLimitExceeded"),
                AwsErrorClass::Transient,
            ),
            (
                [Some(NOT_FOUND), None, Some("RequestLimitExceeded")],
                Some("RequestLimitExceeded"),
                AwsErrorClass::Permanent,
            ),
            (
                [
                    Some("DependencyViolation"),
                    None,
                    Some("RequestLimitExceeded"),
                ],
                Some("DependencyViolation"),
                AwsErrorClass::Permanent,
            ),
            (
                [
                    Some("RequestLimitExceeded"),
                    None,
                    Some("DependencyViolation"),
                ],
                Some("DependencyViolation"),
                AwsErrorClass::Permanent,
            ),
        ] {
            let items = codes.map(|code| unsuccessful_item(None, code, None));
            let failures = AwsItemFailures::from_items(&items).unwrap();
            assert_eq!(
                (failures.code(), failures.classification()),
                (expected, class),
                "{codes:?}"
            );
        }
    }

    #[test]
    fn item_failures_classification_is_order_independent() {
        use carina_core::provider::ProviderErrorKind;

        for (codes, expected_code, transient) in [
            (
                ["RequestLimitExceeded", "InvalidParameterValue"],
                "InvalidParameterValue",
                false,
            ),
            (
                ["DependencyViolation", "InvalidParameterValue"],
                "InvalidParameterValue",
                false,
            ),
            (
                ["InvalidFlowLogId.NotFound", "RequestLimitExceeded"],
                "RequestLimitExceeded",
                true,
            ),
            (
                ["InvalidParameterValue", "AccessDenied"],
                "AccessDenied",
                false,
            ),
        ] {
            let [first, reversed] = [codes, [codes[1], codes[0]]].map(|codes| {
                let items = codes.map(|code| unsuccessful_item(None, Some(code), None));
                let error = api_error_with_item_failure::<DeleteFlowLogsError>(
                    "Failed batch",
                    AwsItemFailures::from_items(&items).unwrap(),
                    None,
                );
                assert_eq!(error.kind(), ProviderErrorKind::ApiError, "{codes:?}");
                assert_eq!(error.is_transient(), transient, "{codes:?}");
                assert!(!error.is_delete_dependency(), "{codes:?}");
                let error =
                    carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(
                        error,
                    );
                assert_eq!(
                    error.detail().code().as_ref().map(|code| code.as_str()),
                    Some(expected_code),
                    "{codes:?}"
                );
                (
                    error.is_delete_dependency(),
                    error.detail().code().map(|code| code.as_str().to_owned()),
                    error.kind(),
                    error.retryability(),
                )
            });
            // Every item's code remains diagnostic; only the provider's class decides.
            assert_eq!(first, reversed, "{codes:?}");
        }
    }

    #[test]
    fn item_failures_classify_all_items_and_keep_each_diagnostic() {
        use carina_core::provider::ProviderErrorKind;

        const NOT_FOUND: &str = "InvalidFlowLogId.NotFound";
        for (first, second, kind, code, transient) in [
            (
                Some(NOT_FOUND),
                Some(NOT_FOUND),
                ProviderErrorKind::NotFound,
                Some(NOT_FOUND),
                false,
            ),
            (
                Some(NOT_FOUND),
                Some("DependencyViolation"),
                ProviderErrorKind::ApiError,
                Some("DependencyViolation"),
                false,
            ),
            (
                Some(NOT_FOUND),
                None,
                ProviderErrorKind::ApiError,
                Some(NOT_FOUND),
                false,
            ),
            (
                Some(NOT_FOUND),
                Some(""),
                ProviderErrorKind::ApiError,
                Some(NOT_FOUND),
                false,
            ),
            (
                Some("RequestLimitExceeded"),
                None,
                ProviderErrorKind::ApiError,
                Some("RequestLimitExceeded"),
                false,
            ),
            (
                None,
                Some(NOT_FOUND),
                ProviderErrorKind::ApiError,
                Some(NOT_FOUND),
                false,
            ),
            (
                None,
                Some("RequestLimitExceeded"),
                ProviderErrorKind::ApiError,
                Some("RequestLimitExceeded"),
                false,
            ),
            (
                Some("InvalidParameterValue"),
                Some("RequestLimitExceeded"),
                ProviderErrorKind::ApiError,
                Some("InvalidParameterValue"),
                false,
            ),
            (
                Some("RequestLimitExceeded"),
                Some(NOT_FOUND),
                ProviderErrorKind::ApiError,
                Some("RequestLimitExceeded"),
                true,
            ),
        ] {
            let items = [
                (first, "first item diagnostic"),
                (second, "second item diagnostic"),
            ]
            .map(|(code, message)| unsuccessful_item(None, code, Some(message)));
            let failures = AwsItemFailures::from_items(&items).unwrap();
            assert_eq!(
                failures
                    .iter()
                    .map(AwsItemFailure::code)
                    .collect::<Vec<_>>(),
                [first, second].map(|code| code.filter(|code| !code.is_empty()))
            );
            let error =
                api_error_with_item_failure::<DeleteFlowLogsError>("Failed batch", failures, None);
            assert_eq!(error.kind(), kind, "{first:?} {second:?}");
            assert_eq!(error.is_transient(), transient, "{first:?} {second:?}");
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            assert_eq!(
                error.detail().code().as_ref().map(|code| code.as_str()),
                code
            );
            assert!(
                error
                    .detail()
                    .cause()
                    .unwrap()
                    .as_cause()
                    .matches_for_test::<AwsItemFailures>(|failures| failures.iter().count() == 2)
            );
            assert_eq!(error.detail().context(), "Failed batch");
            let rendered = error.to_string();
            assert!(rendered.contains("first item diagnostic"), "{rendered}");
            assert!(rendered.contains("second item diagnostic"), "{rendered}");
        }
    }

    #[test]
    fn partial_item_failure_is_unclassified_even_for_retryable_codes_or_messages() {
        use carina_aws_types::sdk::ec2::operation::create_flow_logs::CreateFlowLogsError;
        use carina_core::provider::ProviderErrorKind;

        for code in [
            "InvalidVpcID.NotFound",
            "RequestLimitExceeded",
            "InvalidParameterValue",
            "DependencyViolation",
        ] {
            let item = unsuccessful_item(None, Some(code), Some("Rate exceeded"));
            let error = api_error_with_partial_item_failure::<CreateFlowLogsError>(
                "CreateFlowLogs created 1 flow log(s), but another item failed",
                AwsItemFailures::from_items(&[item]).unwrap(),
                Some("partial-request"),
            );
            assert_eq!(error.kind(), ProviderErrorKind::ApiError, "{code}");
            assert_eq!(error.retryability(), Retryability::Unclassified, "{code}");
            assert!(!error.is_delete_dependency(), "{code}");
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            let wire = error.to_wire();
            assert_eq!(error.retryability(), Retryability::Unclassified, "{code}");
            assert!(!error.is_delete_dependency(), "{code}");
            assert_eq!(wire.code.as_deref(), Some(code));
            assert_eq!(wire.operation.as_deref(), Some("ec2.CreateFlowLogs"));
            assert_eq!(wire.request_id.as_deref(), Some("partial-request"));
            assert!(error.detail().cause().is_some_and(|cause| {
                cause.is::<AwsItemFailures>()
                    && cause.to_string() == format!("{code}: Rate exceeded")
            }));
            assert!(!wire.message.contains("Rate exceeded"));
        }
    }

    #[test]
    fn item_failure_classification_matches_delete_sdk_errors() {
        use carina_aws_types::sdk::ec2::operation::delete_flow_logs::DeleteFlowLogsError;
        use carina_aws_types::sdk::ec2::operation::delete_vpc_endpoints::DeleteVpcEndpointsError;

        for (status, code) in [
            (400, "InvalidFlowLogId.NotFound"),
            (400, "InvalidVpcEndpointId.NotFound"),
            (400, "RequestLimitExceeded"),
            (400, "RequestTimeout"),
            (400, "SlowDown"),
            (400, "OperationAborted"),
            (500, "InternalError"),
            (500, "InternalFailure"),
            (503, "ServiceUnavailable"),
            (503, "Unavailable"),
            (400, "DependencyViolation"),
            (400, "InvalidParameterValue"),
        ] {
            let sdk_errors = [
                api_error_with_meta(
                    "delete flow logs",
                    operation_service_error_with_meta(
                        status,
                        Some(code),
                        Some("AWS diagnostic"),
                        DeleteFlowLogsError::generic,
                    ),
                ),
                api_error_with_meta(
                    "delete VPC endpoints",
                    operation_service_error_with_meta(
                        status,
                        Some(code),
                        Some("AWS diagnostic"),
                        DeleteVpcEndpointsError::generic,
                    ),
                ),
            ];
            for sdk_error in sdk_errors {
                let item_error = api_error_with_item_failure::<DeleteFlowLogsError>(
                    "delete items",
                    AwsItemFailures::from_items(&[unsuccessful_item(
                        None,
                        Some(code),
                        Some("AWS diagnostic"),
                    )])
                    .unwrap(),
                    None,
                );
                assert_eq!(item_error.kind(), sdk_error.kind(), "{code}");
                assert_eq!(
                    item_error.is_transient(),
                    sdk_error.is_transient(),
                    "{code}"
                );
                assert_eq!(
                    item_error.retryability(),
                    sdk_error.retryability(),
                    "{code}"
                );
                assert_eq!(
                    item_error.is_delete_dependency(),
                    sdk_error.is_delete_dependency(),
                    "{code}"
                );
            }
        }
    }

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

    #[test]
    fn api_error_with_meta_keeps_retryability_and_original_sdk_cause() {
        use carina_core::provider_text::ProviderDiagnostic;

        for (status, code, expected) in [
            (403, "AccessDenied", Retryability::Unclassified),
            (400, "ValidationException", Retryability::Unclassified),
            (400, "ThrottlingException", Retryability::Transient),
            (404, "RequestLimitExceeded", Retryability::Transient),
            (429, "Unmodeled", Retryability::Transient),
            (503, "NoSuchEntity", Retryability::Transient),
            (503, "Unmodeled", Retryability::Transient),
        ] {
            let error = api_error_with_meta(
                "Failed to list IAM roles",
                operation_service_error_with_meta(
                    status,
                    Some(code),
                    Some("AWS failure"),
                    ListRolesError::generic,
                ),
            );
            assert_eq!(error.retryability(), expected, "{status} {code}");
            let diagnostic = ProviderDiagnostic::new(error);
            assert!(diagnostic.source_is::<SdkError<ListRolesError>>());
            assert!(
                diagnostic
                    .as_cause()
                    .matches_for_test(|sdk: &SdkError<ListRolesError>| sdk
                        .as_service_error()
                        .is_some())
            );
        }
    }

    #[test]
    fn api_error_with_meta_classifies_dependency_violation() {
        use carina_aws_types::sdk::ec2::operation::delete_vpc::DeleteVpcError;

        let error = api_error_with_meta(
            "Failed to delete VPC",
            operation_service_error_with_meta(
                400,
                Some("DependencyViolation"),
                Some("still in use"),
                DeleteVpcError::generic,
            ),
        );
        assert!(error.is_delete_dependency());
        assert_eq!(error.retryability(), Retryability::DependencyBlocked);
        assert!(!error.is_transient());
        let error = error.into_host_for_test();
        assert!(error.is_delete_dependency());
        assert_eq!(error.retryability(), Retryability::DependencyBlocked);
    }

    #[test]
    fn item_dependency_class_comes_from_the_selected_code_only() {
        for (codes, expected_code, dependency) in [
            (
                ["DependencyViolation", "DependencyViolation"],
                "DependencyViolation",
                true,
            ),
            (
                ["InvalidFlowLogId.NotFound", "DependencyViolation"],
                "DependencyViolation",
                true,
            ),
            (
                ["RequestLimitExceeded", "DependencyViolation"],
                "DependencyViolation",
                true,
            ),
            (
                ["InvalidParameterValue", "DependencyViolation"],
                "InvalidParameterValue",
                false,
            ),
            (
                ["DependencyViolation", "InvalidParameterValue"],
                "InvalidParameterValue",
                false,
            ),
            (
                ["AccessDenied", "DependencyViolation"],
                "AccessDenied",
                false,
            ),
        ] {
            let items =
                codes.map(|code| unsuccessful_item(None, Some(code), Some("Rate exceeded")));
            let error = api_error_with_item_failure::<DeleteFlowLogsError>(
                "Failed batch",
                AwsItemFailures::from_items(&items).unwrap(),
                None,
            );
            let expected = if dependency {
                Retryability::DependencyBlocked
            } else {
                Retryability::Unclassified
            };
            assert_eq!(error.retryability(), expected, "{codes:?}");
            assert_eq!(error.is_delete_dependency(), dependency, "{codes:?}");
            let error = error.into_host_for_test();
            assert_eq!(error.retryability(), expected, "{codes:?}");
            assert_eq!(error.is_delete_dependency(), dependency, "{codes:?}");
            assert_eq!(
                error.to_wire().code.as_deref(),
                Some(expected_code),
                "{codes:?}"
            );
        }
    }

    #[test]
    fn delete_dependency_messages_are_scoped_to_delete_operations() {
        use carina_aws_types::sdk::ec2::operation::{
            delete_vpc::DeleteVpcError, describe_vpcs::DescribeVpcsError,
            detach_internet_gateway::DetachInternetGatewayError,
            release_address::ReleaseAddressError,
        };

        for message in [
            "VPC has dependent object",
            "VPC has a dependent object",
            "resource has dependencies",
            "VPC has some mapped public address(es)",
        ] {
            let errors = [
                api_error_with_meta(
                    "Failed to delete VPC",
                    operation_service_error_with_meta(
                        400,
                        Some("InvalidParameterValue"),
                        Some(message),
                        DeleteVpcError::generic,
                    ),
                ),
                api_error_with_meta(
                    "Failed to detach internet gateway",
                    operation_service_error_with_meta(
                        400,
                        Some("InvalidParameterValue"),
                        Some(message),
                        DetachInternetGatewayError::generic,
                    ),
                ),
                api_error_with_meta(
                    "Failed to release address",
                    operation_service_error_with_meta(
                        400,
                        Some("InvalidParameterValue"),
                        Some(message),
                        ReleaseAddressError::generic,
                    ),
                ),
            ];
            for error in errors {
                assert_eq!(
                    error.retryability(),
                    Retryability::DependencyBlocked,
                    "{message}"
                );
                assert!(
                    error.into_host_for_test().is_delete_dependency(),
                    "{message}"
                );
            }
            let read_error = api_error_with_meta(
                "Failed to delete VPC",
                operation_service_error_with_meta(
                    400,
                    Some("InvalidParameterValue"),
                    Some(message),
                    DescribeVpcsError::generic,
                ),
            );
            assert_eq!(
                read_error.retryability(),
                Retryability::Unclassified,
                "{message}"
            );
            assert!(
                !read_error.into_host_for_test().is_delete_dependency(),
                "{message}"
            );
            let item_error = api_error_with_item_failure::<DeleteFlowLogsError>(
                "Failed batch",
                AwsItemFailures::from_items(&[unsuccessful_item(
                    None,
                    Some("InvalidParameterValue"),
                    Some(message),
                )])
                .unwrap(),
                None,
            );
            assert_eq!(
                item_error.retryability(),
                Retryability::Unclassified,
                "{message}"
            );
        }
    }

    #[test]
    fn detach_context_inherits_only_the_classified_cause() {
        use carina_aws_types::sdk::ec2::operation::{
            detach_internet_gateway::DetachInternetGatewayError,
            detach_vpn_gateway::DetachVpnGatewayError,
        };

        for (code, expected) in [
            ("AccessDenied", Retryability::Unclassified),
            ("DependencyViolation", Retryability::DependencyBlocked),
            ("RequestLimitExceeded", Retryability::Transient),
        ] {
            let errors = [
                api_error_with_meta(
                    "Failed to detach internet gateway",
                    operation_service_error_with_meta(
                        400,
                        Some(code),
                        Some("AWS failure"),
                        DetachInternetGatewayError::generic,
                    ),
                ),
                api_error_with_meta(
                    "Failed to detach VPN gateway",
                    operation_service_error_with_meta(
                        400,
                        Some(code),
                        Some("AWS failure"),
                        DetachVpnGatewayError::generic,
                    ),
                ),
            ];
            for cause in errors {
                let error = ProviderError::api_error_caused_by("Failed to detach gateway", cause);
                assert_eq!(error.retryability(), expected, "{code}");
                assert_eq!(
                    error.is_delete_dependency(),
                    expected == Retryability::DependencyBlocked,
                    "{code}"
                );
                assert_eq!(
                    error
                        .clone()
                        .with_retryability(Retryability::Unclassified)
                        .retryability(),
                    Retryability::Unclassified
                );
                let error = error.into_host_for_test();
                assert_eq!(error.retryability(), expected, "{code}");
                assert_eq!(
                    error.is_delete_dependency(),
                    expected == Retryability::DependencyBlocked,
                    "{code}"
                );
            }
        }
    }

    fn operation_service_error_with_meta<E>(
        status: u16,
        code: Option<&str>,
        message: Option<&str>,
        make_error: impl FnOnce(ErrorMetadata) -> E,
    ) -> AwsSdkError<E> {
        let mut meta = ErrorMetadata::builder();
        if let Some(code) = code {
            meta = meta.code(code);
        }
        if let Some(message) = message {
            meta = meta.message(message);
        }
        let raw = Response::new(StatusCode::try_from(status).unwrap(), SdkBody::empty());
        SdkError::service_error(make_error(meta.build()), raw)
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

        let pe = api_error_with_meta("Failed to list IAM roles", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("iam.ListRoles")
        );
        assert_eq!(detail.status, Some(403));
        assert_eq!(
            detail.code().as_ref().map(|text| text.as_str()),
            Some("AccessDenied")
        );
        assert_eq!(
            detail
                .request_id
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("test-request-id")
        );
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

        let pe = api_error_with_meta("Failed to list IAM roles", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
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

        let pe = api_error_with_meta("Failed to list IAM roles", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(detail.context(), "Failed to list IAM roles");
        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("iam.ListRoles")
        );
        assert_eq!(detail.status, None);
        assert_eq!(detail.code().as_ref().map(|text| text.as_str()), None);
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
    fn api_error_with_meta_keeps_readonly_context_without_outcome_unknown_note() {
        let sdk_err: SdkError<ListRolesError> = SdkError::timeout_error("response timed out");

        let pe = api_error_with_meta("Failed to list IAM roles", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let rendered = pe.to_string();

        assert!(
            rendered.starts_with("Failed to list IAM roles"),
            "{rendered}"
        );
        assert!(!rendered.contains("may have been applied"), "{rendered}");
        assert!(rendered.contains("iam.ListRoles"), "{rendered}");
    }

    #[test]
    fn api_error_with_meta_uses_operation_identity_from_the_error_type() {
        use carina_aws_types::sdk::cloudwatchlogs::operation::describe_log_groups::DescribeLogGroupsError;

        let sdk_err: carina_aws_types::sdk::cloudwatchlogs::error::SdkError<
            DescribeLogGroupsError,
        > = carina_aws_types::sdk::cloudwatchlogs::error::SdkError::timeout_error(
            "response timed out",
        );

        let pe = api_error_with_meta("Failed to describe log groups", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);

        assert_eq!(
            pe.detail()
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("cloudwatchlogs.DescribeLogGroups")
        );
        assert!(!pe.to_string().contains(OUTCOME_UNKNOWN_NOTE));
    }

    #[test]
    fn api_error_with_meta_appends_outcome_unknown_only_for_non_idempotent_operation() {
        use carina_aws_types::sdk::ec2::operation::create_vpc::CreateVpcError;

        let sdk_err: carina_aws_types::sdk::ec2::error::SdkError<CreateVpcError> =
            carina_aws_types::sdk::ec2::error::SdkError::timeout_error("response timed out");

        let pe = api_error_with_meta("Failed to create VPC", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let rendered = pe.to_string();

        assert_eq!(pe.detail().context(), "Failed to create VPC");
        assert!(rendered.starts_with("Failed to create VPC"), "{rendered}");
        assert!(rendered.contains(OUTCOME_UNKNOWN_NOTE), "{rendered}");
        assert!(rendered.contains("response timed out"), "{rendered}");
    }

    #[test]
    fn api_error_with_meta_uses_sticky_outcome_from_provider_retries() {
        use carina_aws_types::sdk::ec2::operation::create_vpc::CreateVpcError;

        let service_error = CreateVpcError::generic(
            ErrorMetadata::builder()
                .code("RequestLimitExceeded")
                .message("final attempt throttled")
                .build(),
        );
        let raw = Response::new(
            StatusCode::try_from(429).expect("valid status"),
            SdkBody::empty(),
        );
        let failure = SendFailure::new(
            SendClassification::MaybeSent {
                final_attempt: carina_aws_types::FinalAttempt::Sent,
            },
            SdkError::service_error(service_error, raw),
        );

        let error = api_error_with_meta("Failed to create VPC", failure);
        let error =
            carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
        let rendered = error.to_string();

        assert!(rendered.contains(OUTCOME_UNKNOWN_NOTE), "{rendered}");
        assert_eq!(error.detail().status, Some(429));
        assert_eq!(
            error.detail().code().as_ref().map(|text| text.as_str()),
            Some("RequestLimitExceeded")
        );
    }

    #[test]
    fn api_error_with_meta_does_not_mark_token_bearing_create_as_outcome_unknown() {
        use carina_aws_types::sdk::ec2::operation::create_nat_gateway::CreateNatGatewayError;

        let sdk_err: carina_aws_types::sdk::ec2::error::SdkError<CreateNatGatewayError> =
            carina_aws_types::sdk::ec2::error::SdkError::timeout_error("response timed out");

        let pe = api_error_with_meta("Failed to create NAT gateway", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);

        assert_eq!(pe.detail().context(), "Failed to create NAT gateway");
        assert!(!pe.to_string().contains("may have been applied"));
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_acm_resource_not_found() {
        use carina_aws_types::sdk::acm::operation::describe_certificate::DescribeCertificateError;

        let sdk_err = operation_service_error_with_meta(
            400,
            Some("ResourceNotFoundException"),
            Some("resource not found"),
            DescribeCertificateError::generic,
        );

        let pe = api_error_with_meta("Failed to describe ACM certificate", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("acm.DescribeCertificate")
        );
        assert_eq!(detail.status, Some(400));
        assert_eq!(
            detail.code().as_ref().map(|text| text.as_str()),
            Some("ResourceNotFoundException")
        );
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_iam_no_such_entity() {
        use carina_aws_types::sdk::iam::operation::get_role::GetRoleError;

        let sdk_err = operation_service_error_with_meta(
            404,
            Some("NoSuchEntity"),
            Some("role not found"),
            GetRoleError::generic,
        );

        let pe = api_error_with_meta("Failed to get IAM role", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("iam.GetRole")
        );
        assert_eq!(detail.status, Some(404));
        assert_eq!(
            detail.code().as_ref().map(|text| text.as_str()),
            Some("NoSuchEntity")
        );
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_ec2_invalid_group() {
        use carina_aws_types::sdk::ec2::operation::describe_security_groups::DescribeSecurityGroupsError;

        let sdk_err = operation_service_error_with_meta(
            400,
            Some("InvalidGroup.NotFound"),
            Some("security group not found"),
            DescribeSecurityGroupsError::generic,
        );

        let pe = api_error_with_meta("Failed to describe security groups", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("ec2.DescribeSecurityGroups")
        );
        assert_eq!(detail.status, Some(400));
        assert_eq!(
            detail.code().as_ref().map(|text| text.as_str()),
            Some("InvalidGroup.NotFound")
        );
    }

    #[test]
    fn api_error_with_meta_returns_api_error_for_other_codes() {
        let sdk_err = list_roles_service_error_with_meta(
            400,
            Some("ValidationException"),
            Some("invalid input"),
        );

        let pe = api_error_with_meta("Failed to list IAM roles", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::ApiError(detail) = &pe else {
            panic!("expected ApiError, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("iam.ListRoles")
        );
        assert_eq!(detail.status, Some(400));
        assert_eq!(
            detail.code().as_ref().map(|text| text.as_str()),
            Some("ValidationException")
        );
    }

    #[test]
    fn api_error_with_meta_returns_not_found_for_http_404() {
        use carina_aws_types::sdk::s3::operation::head_bucket::HeadBucketError;

        let sdk_err = operation_service_error_with_meta(
            404,
            None,
            Some("not found without a code"),
            HeadBucketError::generic,
        );

        let pe = api_error_with_meta("Failed to head S3 bucket", sdk_err);
        let pe = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(pe);
        let CoreProviderError::NotFound(detail) = &pe else {
            panic!("expected NotFound, got {:?}", pe);
        };

        assert_eq!(
            detail
                .operation
                .as_ref()
                .map(|text| text.as_diagnostic())
                .as_ref()
                .map(|text| text.as_str()),
            Some("s3.HeadBucket")
        );
        assert_eq!(detail.status, Some(404));
        assert_eq!(detail.code().as_ref().map(|text| text.as_str()), None);
    }
}
