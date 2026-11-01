//! Shared, comparison-only KMS reference resolution for AWS provider guests.

use std::error::Error;

use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use carina_core::provider::BoxFuture;
use carina_provider_protocol::types::{
    ProviderError, ProviderErrorKind, ReferenceError, ReferenceOutcome, ReferenceRequest,
};
use futures::{StreamExt, stream};

const MAX_CONCURRENT_DESCRIBE_KEY: usize = 8;

/// The initialized guest supplies a client using its effective credentials.
/// `region` is the instance region for bare forms, or the ARN's own region.
pub trait DescribeKeyClient: Sync {
    fn describe_key<'a>(
        &'a self,
        region: &'a str,
        raw: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>;
}

/// Safe SDK display text and AWS metadata; classification never inspects message text.
/// Missing response metadata represents a transport, timeout or credentials failure.
#[derive(Debug, Clone)]
pub struct DescribeKeyFailure {
    message: String,
    code: Option<String>,
    status: Option<u16>,
    request_id: Option<String>,
}

#[cfg(feature = "test-utils")]
impl DescribeKeyFailure {
    /// Construct a fake failure for tests. Production failures use [`describe_key_failure`].
    pub fn for_test(
        message: impl Into<String>,
        code: Option<String>,
        status: Option<u16>,
        request_id: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            code,
            status,
            request_id,
        }
    }
}

/// Preserve service metadata and the immediate transport cause's Display only.
/// `ServiceError` keeps the AWS message, which can include the caller principal ARN
/// or session name; these are identifiers, not secrets.
/// Never format Debug or walk sources, which can expose credential-provider details.
pub fn describe_key_failure<E>(failure: SdkError<E, HttpResponse>) -> DescribeKeyFailure
where
    E: ProvideErrorMetadata + Error + 'static,
{
    let detail = failure.message().map(str::to_owned).or_else(|| {
        if matches!(
            failure,
            SdkError::ConstructionFailure(_)
                | SdkError::TimeoutError(_)
                | SdkError::DispatchFailure(_)
        ) {
            failure.source().map(ToString::to_string)
        } else {
            None
        }
    });
    let message = match detail {
        Some(detail) => format!("{failure}: {detail}"),
        None => failure.to_string(),
    };
    DescribeKeyFailure {
        message,
        code: failure.code().map(str::to_owned),
        status: failure
            .raw_response()
            .map(|response| response.status().as_u16()),
        request_id: failure
            .raw_response()
            .and_then(|response| {
                response
                    .headers()
                    .get("x-amzn-requestid")
                    .or_else(|| response.headers().get("x-amz-request-id"))
            })
            .map(str::to_owned),
    }
}

fn error(
    provider_name: &str,
    operation: &str,
    kind: ProviderErrorKind,
    message: impl Into<String>,
    failure: Option<DescribeKeyFailure>,
) -> Box<ProviderError> {
    let mut error = ProviderError {
        kind,
        message: message.into(),
        resource_id: None,
        cause: None,
        provider_name: Some(provider_name.into()),
        operation: Some(operation.into()),
        code: None,
        status: None,
        request_id: None,
    };
    if let Some(failure) = failure {
        error.cause = Some(failure.message);
        error.code = failure.code;
        error.status = failure.status;
        error.request_id = failure.request_id;
    }
    Box::new(error)
}

/// Map the region to its partition for ARN reachability checks.
fn partition(region: &str) -> &'static str {
    if region.starts_with("cn-") {
        "aws-cn"
    } else if region.starts_with("us-gov-") {
        "aws-us-gov"
    } else if region.starts_with("us-iso-") {
        "aws-iso"
    } else if region.starts_with("us-isob-") {
        "aws-iso-b"
    } else if region.starts_with("eu-isoe-") {
        "aws-iso-e"
    } else if region.starts_with("us-isof-") {
        "aws-iso-f"
    } else if region.starts_with("eusc-") {
        "aws-eusc"
    } else {
        "aws"
    }
}

/// Pure resolution over an injected DescribeKey client. No payload or state is rewritten.
pub async fn resolve_kms_key_reference(
    client: &impl DescribeKeyClient,
    provider_name: &str,
    instance_region: &str,
    raw: &str,
) -> Result<ReferenceOutcome, ReferenceError> {
    crate::validate_kms_key_reference(raw).map_err(|reason| {
        ReferenceError::InvalidReference(error(
            provider_name,
            "kms.DescribeKey",
            ProviderErrorKind::InvalidInput,
            reason,
            None,
        ))
    })?;
    let mut region = instance_region;
    if raw.starts_with("arn:") {
        let parts: Vec<_> = raw.splitn(6, ':').collect();
        if partition(instance_region) != parts[1] || partition(parts[3]) != parts[1] {
            return Err(ReferenceError::InvalidReference(error(
                provider_name,
                "kms.DescribeKey",
                ProviderErrorKind::InvalidInput,
                "KMS ARN partition is unreachable from this provider instance",
                None,
            )));
        }
        region = parts[3];
        if parts[5].starts_with("key/") {
            crate::validate_kms_key_arn(raw).map_err(|reason| {
                ReferenceError::InvalidReference(error(
                    provider_name,
                    "kms.DescribeKey",
                    ProviderErrorKind::InvalidInput,
                    reason,
                    None,
                ))
            })?;
            return Ok(ReferenceOutcome::Resolved(raw.to_owned()));
        }
    }
    match client.describe_key(region, raw).await {
        Ok(Some(arn)) if crate::validate_kms_key_arn(&arn).is_ok() => {
            Ok(ReferenceOutcome::Resolved(arn))
        }
        Ok(_) => Err(ReferenceError::InvalidReference(error(
            provider_name,
            "kms.DescribeKey",
            ProviderErrorKind::InvalidInput,
            "DescribeKey response must contain a valid KmsKeyArn",
            None,
        ))),
        Err(failure) => classify_failure(provider_name, failure),
    }
}

/// Resolve a positional batch using the shared identity check and at most eight calls.
pub async fn resolve_reference_batch(
    client: &impl DescribeKeyClient,
    provider_name: &str,
    region: &str,
    requests: Vec<ReferenceRequest>,
) -> Vec<Result<ReferenceOutcome, ReferenceError>> {
    let identity = crate::AwsType::KmsKeyReference.identity();
    stream::iter(requests.into_iter().map(|request| {
        let identity = &identity;
        async move {
            if identity.provider.as_deref() != Some(request.ty.provider.as_str())
                || identity.segments != request.ty.segments
                || identity.kind != request.ty.kind
            {
                let ty = std::iter::once(request.ty.provider)
                    .chain(request.ty.segments)
                    .chain(std::iter::once(request.ty.kind))
                    .collect::<Vec<_>>()
                    .join(".");
                return Err(ReferenceError::InvalidReference(error(
                    provider_name,
                    "resolve_references",
                    ProviderErrorKind::InvalidInput,
                    format!("unsupported reference identity {ty}"),
                    None,
                )));
            }
            resolve_kms_key_reference(client, provider_name, region, &request.value).await
        }
    }))
    .buffered(MAX_CONCURRENT_DESCRIBE_KEY)
    .collect()
    .await
}

fn classify_failure(
    provider_name: &str,
    failure: DescribeKeyFailure,
) -> Result<ReferenceOutcome, ReferenceError> {
    let code = failure.code.as_deref();
    let not_found = code == Some("NotFoundException");
    let denied = code.is_some_and(|code| code.starts_with("AccessDenied"));
    let invalid = matches!(
        code,
        Some(
            "InvalidArnException"
                | "InvalidKeyIdException"
                | "ValidationException"
                | "ValidationError"
                | "InvalidParameterValueException"
                | "InvalidParameterException"
        )
    );
    let kind = if invalid {
        ProviderErrorKind::InvalidInput
    } else {
        ProviderErrorKind::ApiError
    };
    let error = error(
        provider_name,
        "kms.DescribeKey",
        kind,
        "KMS reference resolution failed",
        Some(failure),
    );
    if not_found {
        Ok(ReferenceOutcome::NotFound(error))
    } else if denied {
        Err(ReferenceError::AccessDenied(error))
    } else if invalid {
        Err(ReferenceError::InvalidReference(error))
    } else {
        Err(ReferenceError::Transient(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::provider::error::CredentialsError;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::{body::SdkBody, error::ErrorMetadata};
    use carina_provider_protocol::types::{ReferenceError, ReferenceOutcome};
    use std::sync::Mutex;

    const KEY_ID: &str = "1234abcd-12ab-34cd-56ef-1234567890ab";
    const KEY_ARN: &str =
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab";
    const PARTITION_REGIONS: [(&str, &str, &str); 8] = [
        ("aws", "us-west-2", "us-east-1"),
        ("aws-cn", "cn-northwest-1", "cn-north-1"),
        ("aws-us-gov", "us-gov-east-1", "us-gov-west-1"),
        ("aws-iso", "us-iso-west-1", "us-iso-east-1"),
        ("aws-iso-b", "us-isob-west-1", "us-isob-east-1"),
        ("aws-iso-e", "eu-isoe-west-1", "eu-isoe-west-1"),
        ("aws-iso-f", "us-isof-east-1", "us-isof-south-1"),
        ("aws-eusc", "eusc-de-east-1", "eusc-de-east-1"),
    ];

    #[test]
    fn sdk_failure_preserves_service_message_and_metadata_without_debug() {
        for code in [
            "NotFoundException",
            "AccessDeniedException",
            "InvalidArnException",
        ] {
            for request_id_header in ["x-amzn-requestid", "x-amz-request-id"] {
                let metadata = ErrorMetadata::builder()
                    .code(code)
                    .message("KMS key is unavailable to this caller")
                    .custom("sensitive-extra", "must-not-format-extra")
                    .build();
                let mut response = HttpResponse::new(
                    400.try_into().unwrap(),
                    SdkBody::from("must-not-format-body"),
                );
                response
                    .headers_mut()
                    .insert(request_id_header, "request-123");
                response
                    .headers_mut()
                    .insert("x-amz-security-token", "must-not-format-token");
                let failure = describe_key_failure(SdkError::service_error(metadata, response));
                assert_eq!(
                    failure.message,
                    "service error: KMS key is unavailable to this caller"
                );
                assert_eq!(failure.code.as_deref(), Some(code));
                assert_eq!(failure.status, Some(400));
                assert_eq!(failure.request_id.as_deref(), Some("request-123"));
            }
        }
    }

    #[test]
    fn sdk_failure_does_not_format_credential_sources() {
        // A provider can put secrets in its source. Only its safe outer Display is allowed.
        for credentials in [
            CredentialsError::not_loaded("must-not-format-credentials"),
            CredentialsError::invalid_configuration("must-not-format-credentials"),
            CredentialsError::provider_error("must-not-format-credentials"),
            CredentialsError::unhandled("must-not-format-credentials"),
        ] {
            let expected = format!("failed to construct request: {credentials}");
            let failure =
                describe_key_failure::<ErrorMetadata>(SdkError::construction_failure(credentials));
            assert_eq!(failure.message, expected);
            assert!(!failure.message.contains("must-not-format-credentials"));
            assert!(failure.code.is_none());
            assert!(failure.status.is_none());
            assert!(failure.request_id.is_none());
        }
    }

    #[test]
    fn sdk_transport_failures_preserve_immediate_display_in_cause_without_walking_sources() {
        struct DisplayOnly(&'static str);
        impl std::fmt::Display for DisplayOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        impl std::fmt::Debug for DisplayOnly {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("must not format source Debug")
            }
        }
        impl Error for DisplayOnly {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                panic!("must not walk arbitrary error sources")
            }
        }

        for (failure, expected) in [
            (
                SdkError::construction_failure(DisplayOnly("invalid connector configuration")),
                "failed to construct request: invalid connector configuration",
            ),
            (
                SdkError::timeout_error(DisplayOnly("operation timed out after 10 seconds")),
                "request has timed out: operation timed out after 10 seconds",
            ),
            (
                SdkError::dispatch_failure(ConnectorError::io(Box::new(DisplayOnly(
                    "must-not-walk-connector-source",
                )))),
                "dispatch failure: io error",
            ),
            (
                SdkError::timeout_error(CredentialsError::provider_error(
                    "must-not-format-credentials",
                )),
                "request has timed out: an error occurred while loading credentials",
            ),
        ] {
            let failure = describe_key_failure::<ErrorMetadata>(failure);
            assert_eq!(failure.message, expected);
            assert!(failure.code.is_none());
            assert!(failure.status.is_none());
            assert!(failure.request_id.is_none());
            let Err(ReferenceError::Transient(error)) = classify_failure("awscc", failure) else {
                panic!("transport failure must be transient");
            };
            assert_eq!(error.cause.as_deref(), Some(expected));
        }
    }

    struct Fake {
        calls: Mutex<Vec<(String, String)>>,
        response: Result<Option<String>, DescribeKeyFailure>,
    }

    impl DescribeKeyClient for Fake {
        fn describe_key<'a>(
            &'a self,
            region: &'a str,
            raw: &'a str,
        ) -> carina_core::provider::BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>
        {
            self.calls.lock().unwrap().push((region.into(), raw.into()));
            Box::pin(async { self.response.clone() })
        }
    }

    fn client(response: Result<Option<String>, DescribeKeyFailure>) -> Fake {
        Fake {
            calls: Mutex::new(vec![]),
            response,
        }
    }

    #[tokio::test]
    async fn key_arn_is_validated_without_a_call() {
        let fake = client(Ok(None));
        assert!(
            matches!(resolve_kms_key_reference(&fake, "aws", "us-west-2", KEY_ARN).await,
            Ok(ReferenceOutcome::Resolved(arn)) if arn == KEY_ARN)
        );
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn key_arn_preserves_authored_casing_without_a_call() {
        for key_id in [
            "1234ABCD-12AB-34CD-56EF-1234567890AB",
            "mrk-0123456789aBcDeF0123456789AbCdEf",
        ] {
            let raw = format!("arn:aws:kms:us-east-1:123456789012:key/{key_id}");
            let fake = client(Ok(None));
            let result = resolve_kms_key_reference(&fake, "aws", "us-west-2", &raw).await;
            assert!(matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == raw));
            assert!(fake.calls.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn describe_key_preserves_returned_casing_and_raw_request() {
        for key_id in [
            "1234ABCD-12AB-34CD-56EF-1234567890AB",
            "mrk-0123456789aBcDeF0123456789AbCdEf",
        ] {
            let returned = format!("arn:aws:kms:eu-west-1:999999999999:key/{key_id}");
            for raw in [key_id, "alias/MixedCase"] {
                let fake = client(Ok(Some(returned.clone())));
                let result = resolve_kms_key_reference(&fake, "awscc", "eu-west-1", raw).await;
                assert!(matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == returned));
                assert_eq!(
                    *fake.calls.lock().unwrap(),
                    [("eu-west-1".into(), raw.into())]
                );
                assert_eq!(
                    fake.response.as_ref().unwrap().as_deref(),
                    Some(returned.as_str())
                );
            }
        }
    }

    #[tokio::test]
    async fn ids_and_aliases_describe_the_raw_value_in_the_instance_region() {
        for raw in [
            KEY_ID,
            "mrk-0123456789abcdef0123456789abcdef",
            "alias/customer",
            "alias/aws/sqs",
        ] {
            let fake = client(Ok(Some(KEY_ARN.into())));
            assert!(
                matches!(resolve_kms_key_reference(&fake, "aws", "us-west-2", raw).await,
                Ok(ReferenceOutcome::Resolved(arn)) if arn == KEY_ARN)
            );
            assert_eq!(
                *fake.calls.lock().unwrap(),
                [("us-west-2".into(), raw.into())]
            );
        }
    }

    #[tokio::test]
    async fn alias_arn_keeps_its_own_region_and_account() {
        let raw = "arn:aws:kms:eu-west-1:999999999999:alias/customer";
        let canonical =
            "arn:aws:kms:eu-west-1:999999999999:key/mrk-0123456789abcdef0123456789abcdef";
        let fake = client(Ok(Some(canonical.into())));
        assert!(
            matches!(resolve_kms_key_reference(&fake, "aws", "us-east-1", raw).await,
            Ok(ReferenceOutcome::Resolved(arn)) if arn == canonical)
        );
        assert_eq!(
            *fake.calls.lock().unwrap(),
            [("eu-west-1".into(), raw.into())]
        );
    }

    #[tokio::test]
    async fn arn_reference_validates_offline_and_resolves_alias_arn() {
        use carina_core::resource::{ConcreteValue, Value};

        let raw = "arn:aws:kms:eu-west-1:999999999999:alias/Team_1:customer-key";
        let canonical =
            "arn:aws:kms:eu-west-1:999999999999:key/mrk-0123456789abcdef0123456789abcdef";
        let schema = crate::tests::rootless(crate::kms_key_or_alias_arn_reference());
        for value in [raw, canonical] {
            assert!(
                schema
                    .validate_collect(&Value::Concrete(ConcreteValue::String(value.into())))
                    .is_empty(),
                "{value}"
            );
        }

        let fake = client(Ok(Some(canonical.into())));
        let result = resolve_kms_key_reference(&fake, "awscc", "us-east-1", raw).await;
        assert!(matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == canonical));
        assert_eq!(
            *fake.calls.lock().unwrap(),
            [("eu-west-1".into(), raw.into())]
        );
    }

    #[tokio::test]
    async fn same_partition_references_resolve_in_every_partition() {
        for (partition, instance_region, arn_region) in PARTITION_REGIONS {
            let key_arn = format!("arn:{partition}:kms:{arn_region}:123456789012:key/{KEY_ID}");
            let fake = client(Ok(None));
            let result = resolve_kms_key_reference(&fake, "aws", instance_region, &key_arn).await;
            assert!(
                matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == key_arn),
                "{partition}: {key_arn}"
            );
            assert!(fake.calls.lock().unwrap().is_empty());

            let alias_arn = format!("arn:{partition}:kms:{arn_region}:123456789012:alias/customer");
            let fake = client(Ok(Some(key_arn.clone())));
            let result = resolve_kms_key_reference(&fake, "aws", instance_region, &alias_arn).await;
            assert!(
                matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == key_arn),
                "{partition}: {alias_arn}"
            );
            assert_eq!(
                *fake.calls.lock().unwrap(),
                [(arn_region.into(), alias_arn)]
            );

            let instance_key_arn =
                format!("arn:{partition}:kms:{instance_region}:123456789012:key/{KEY_ID}");
            for raw in [
                KEY_ID,
                "mrk-0123456789abcdef0123456789abcdef",
                "alias/customer",
                "alias/aws/sqs",
            ] {
                let fake = client(Ok(Some(instance_key_arn.clone())));
                let result = resolve_kms_key_reference(&fake, "aws", instance_region, raw).await;
                assert!(
                    matches!(result, Ok(ReferenceOutcome::Resolved(arn)) if arn == instance_key_arn),
                    "{partition}: {raw}"
                );
                assert_eq!(
                    *fake.calls.lock().unwrap(),
                    [(instance_region.into(), raw.into())]
                );
            }
        }
    }

    #[tokio::test]
    async fn cross_partition_references_are_invalid_without_a_call() {
        for (instance_partition, instance_region, _) in PARTITION_REGIONS {
            for (arn_partition, _, arn_region) in PARTITION_REGIONS {
                if instance_partition == arn_partition {
                    continue;
                }
                for resource in [format!("key/{KEY_ID}"), "alias/customer".into()] {
                    let raw =
                        format!("arn:{arn_partition}:kms:{arn_region}:123456789012:{resource}");
                    assert!(crate::validate_kms_key_reference(&raw).is_ok(), "{raw}");
                    let fake = client(Ok(Some(KEY_ARN.into())));
                    assert!(
                        matches!(
                            resolve_kms_key_reference(&fake, "aws", instance_region, &raw).await,
                            Err(ReferenceError::InvalidReference(_))
                        ),
                        "{instance_region}: {raw}"
                    );
                    assert!(fake.calls.lock().unwrap().is_empty());
                }
            }
        }
    }

    #[tokio::test]
    async fn malformed_inputs_never_call_kms() {
        for raw in [
            "arn:aws:kms:cn-north-1:123456789012:alias/a",
            "alias/",
            "not-a-key",
            "arn:aws:kms::123:key/",
            "arn:aws:s3:::bucket",
        ] {
            let fake = client(Ok(Some(KEY_ARN.into())));
            assert!(
                matches!(
                    resolve_kms_key_reference(&fake, "aws", "us-east-1", raw).await,
                    Err(ReferenceError::InvalidReference(_))
                ),
                "{raw}"
            );
            assert!(fake.calls.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn successes_require_a_valid_key_arn() {
        for response in [
            None,
            Some(""),
            Some(KEY_ID),
            Some("arn:aws:kms:us-east-1:123456789012:alias/a"),
            Some("arn:aws:kms:us-east-1:123456789012:key/garbage"),
        ] {
            let fake = client(Ok(response.map(str::to_owned)));
            assert!(matches!(
                resolve_kms_key_reference(&fake, "aws", "us-east-1", "alias/a").await,
                Err(ReferenceError::InvalidReference(_))
            ));
        }
    }

    #[tokio::test]
    async fn resolver_errors_use_the_callers_provider_name() {
        for provider_name in ["aws", "awscc"] {
            for (region, raw, response) in [
                ("us-east-1", "alias/", Ok(None)),
                ("cn-north-1", KEY_ARN, Ok(None)),
                ("us-iso-east-1", "alias/a", Ok(None)),
                ("us-east-1", "alias/a", Ok(None)),
                (
                    "us-east-1",
                    "alias/a",
                    Err(DescribeKeyFailure {
                        message: "service error: KMS is unavailable".into(),
                        code: None,
                        status: None,
                        request_id: None,
                    }),
                ),
            ] {
                let fake = client(response);
                let error = match resolve_kms_key_reference(&fake, provider_name, region, raw).await
                {
                    Err(
                        ReferenceError::InvalidReference(error) | ReferenceError::Transient(error),
                    ) => error,
                    other => panic!("unexpected {other:?}"),
                };
                assert_eq!(error.provider_name.as_deref(), Some(provider_name));
            }
        }
    }

    #[tokio::test]
    async fn aws_codes_determine_typed_outcomes_and_preserve_metadata() {
        for (code, status, expected) in [
            (Some("NotFoundException"), Some(400), "not-found"),
            (Some("AccessDenied"), Some(403), "denied"),
            (Some("AccessDeniedException"), Some(400), "denied"),
            (Some("AccessDeniedFoo"), Some(400), "denied"),
            (Some("ThrottlingException"), Some(400), "transient"),
            (Some("Throttling"), Some(400), "transient"),
            (Some("LimitExceededException"), Some(400), "transient"),
            (Some("DependencyTimeoutException"), Some(500), "transient"),
            (Some("KMSInternalException"), Some(500), "transient"),
            (Some("ServiceUnavailableException"), Some(503), "transient"),
            (Some("UnrecognizedClientException"), Some(400), "transient"),
            (Some("ExpiredTokenException"), Some(400), "transient"),
            (None, None, "transient"),
            (None, Some(502), "transient"),
            (Some("InvalidArnException"), Some(400), "invalid"),
            (Some("InvalidKeyIdException"), Some(400), "invalid"),
            (Some("ValidationException"), Some(400), "invalid"),
        ] {
            let fake = client(Err(DescribeKeyFailure {
                message: "service error: AWS failure details".into(),
                code: code.map(str::to_owned),
                status,
                request_id: Some("request-123".into()),
            }));
            let result =
                resolve_kms_key_reference(&fake, "aws", "us-east-1", "alias/aws/sqs").await;
            let (actual, error) = match result {
                Ok(ReferenceOutcome::NotFound(e)) => ("not-found", e),
                Err(ReferenceError::AccessDenied(e)) => ("denied", e),
                Err(ReferenceError::Transient(e)) => ("transient", e),
                Err(ReferenceError::InvalidReference(e)) => ("invalid", e),
                other => panic!("unexpected {other:?}"),
            };
            assert_eq!(actual, expected, "{code:?}");
            assert_eq!(
                error.cause.as_deref(),
                Some("service error: AWS failure details")
            );
            assert_eq!(error.provider_name.as_deref(), Some("aws"));
            assert_eq!(error.code.as_deref(), code);
            assert_eq!(error.status, status);
            assert_eq!(error.request_id.as_deref(), Some("request-123"));
            assert_eq!(fake.calls.lock().unwrap().len(), 1);
        }
    }
}

#[cfg(test)]
mod batch_tests {
    use super::{
        DescribeKeyClient, DescribeKeyFailure, MAX_CONCURRENT_DESCRIBE_KEY, resolve_reference_batch,
    };
    use carina_provider_protocol::types::{
        ReferenceError, ReferenceOutcome, ReferenceRequest, TypeIdentity,
    };
    use std::sync::Mutex;

    struct Fake(Mutex<Vec<String>>);
    impl DescribeKeyClient for Fake {
        fn describe_key<'a>(
            &'a self,
            region: &'a str,
            raw: &'a str,
        ) -> carina_core::provider::BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>
        {
            Box::pin(async move {
                assert_eq!(region, "us-east-1");
                self.0.lock().unwrap().push(raw.into());
                tokio::task::yield_now().await;
                Ok(Some(format!(
                    "arn:aws:kms:us-east-1:123456789012:key/{}",
                    if raw == "alias/first" {
                        "11111111-1111-1111-1111-111111111111"
                    } else {
                        "22222222-2222-2222-2222-222222222222"
                    }
                )))
            })
        }
    }

    fn request(value: &str) -> ReferenceRequest {
        ReferenceRequest {
            ty: TypeIdentity {
                provider: "aws".into(),
                segments: vec!["kms".into(), "Key".into()],
                kind: "Reference".into(),
            },
            value: value.into(),
        }
    }

    #[tokio::test]
    async fn batch_checks_each_identity_component_with_the_callers_provider_name() {
        for provider_name in ["aws", "awscc"] {
            let fake = Fake(Mutex::new(vec![]));
            let mut wrong_provider = request("alias/ignored");
            wrong_provider.ty.provider = "awscc".into();
            let mut wrong_segments = request("alias/ignored");
            wrong_segments.ty.segments = vec!["kms".into(), "Other".into()];
            let mut wrong_kind = request("alias/ignored");
            wrong_kind.ty.kind = "Arn".into();
            let key_arn =
                "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab";
            let results = resolve_reference_batch(
                &fake,
                provider_name,
                "us-east-1",
                vec![wrong_provider, request(key_arn), wrong_segments, wrong_kind],
            )
            .await;
            assert_eq!(results.len(), 4);
            assert!(matches!(&results[1], Ok(ReferenceOutcome::Resolved(arn)) if arn == key_arn));
            for (index, identity) in [
                (0, "awscc.kms.Key.Reference"),
                (2, "aws.kms.Other.Reference"),
                (3, "aws.kms.Key.Arn"),
            ] {
                let Err(ReferenceError::InvalidReference(error)) = &results[index] else {
                    panic!("unsupported identity must fail at its input position");
                };
                assert_eq!(error.provider_name.as_deref(), Some(provider_name));
                assert_eq!(
                    error.kind,
                    carina_provider_protocol::types::ProviderErrorKind::InvalidInput
                );
                assert_eq!(error.operation.as_deref(), Some("resolve_references"));
                assert_eq!(
                    error.message,
                    format!("unsupported reference identity {identity}")
                );
                assert!(error.cause.is_none());
            }
            assert!(fake.0.lock().unwrap().is_empty());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn batch_bounds_concurrency_and_keeps_positions_when_calls_finish_out_of_order() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        #[derive(Default)]
        struct ConcurrentClient {
            active: AtomicUsize,
            peak: AtomicUsize,
            completed: Mutex<Vec<usize>>,
        }

        impl DescribeKeyClient for ConcurrentClient {
            fn describe_key<'a>(
                &'a self,
                _region: &'a str,
                raw: &'a str,
            ) -> carina_core::provider::BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>
            {
                Box::pin(async move {
                    let index: usize = raw.strip_prefix("alias/").unwrap().parse().unwrap();
                    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                    self.peak.fetch_max(active, Ordering::SeqCst);
                    // Each window completes in reverse order under the paused clock.
                    tokio::time::sleep(Duration::from_secs(
                        (MAX_CONCURRENT_DESCRIBE_KEY - index % MAX_CONCURRENT_DESCRIBE_KEY) as u64,
                    ))
                    .await;
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    self.completed.lock().unwrap().push(index);
                    Ok(Some(format!(
                        "arn:aws:kms:us-east-1:123456789012:key/{index:08x}-0000-0000-0000-000000000000"
                    )))
                })
            }
        }

        let client = ConcurrentClient::default();
        let count = MAX_CONCURRENT_DESCRIBE_KEY * 3 + 1;
        let results = resolve_reference_batch(
            &client,
            "aws",
            "us-east-1",
            (0..count)
                .map(|index| request(&format!("alias/{index}")))
                .collect(),
        )
        .await;

        assert_eq!(
            client.peak.load(Ordering::SeqCst),
            MAX_CONCURRENT_DESCRIBE_KEY
        );
        assert_eq!(client.active.load(Ordering::SeqCst), 0);
        assert_eq!(
            client.completed.lock().unwrap()[0],
            MAX_CONCURRENT_DESCRIBE_KEY - 1
        );
        assert_eq!(results.len(), count);
        for (index, result) in results.into_iter().enumerate() {
            assert!(matches!(result, Ok(ReferenceOutcome::Resolved(arn))
                if arn.ends_with(&format!("{index:08x}-0000-0000-0000-000000000000"))));
        }
    }

    #[tokio::test]
    async fn empty_batch_never_calls_kms() {
        let fake = Fake(Mutex::new(vec![]));
        assert!(
            resolve_reference_batch(&fake, "aws", "us-east-1", vec![])
                .await
                .is_empty()
        );
        assert!(fake.0.lock().unwrap().is_empty());
    }
}
