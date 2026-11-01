//! Shared, comparison-only KMS reference resolution for AWS provider guests.

use carina_core::provider::BoxFuture;
use carina_provider_protocol::types::{
    ProviderError, ProviderErrorKind, ReferenceError, ReferenceOutcome,
};

/// The initialized guest supplies a client using its effective credentials.
/// `region` is the instance region for bare forms, or the ARN's own region.
pub trait DescribeKeyClient: Sync {
    fn describe_key<'a>(
        &'a self,
        region: &'a str,
        raw: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>, DescribeKeyFailure>>;
}

/// Structured AWS metadata only; classification never inspects message text.
/// Missing response metadata represents a transport, timeout or credentials failure.
#[derive(Debug, Clone)]
pub struct DescribeKeyFailure {
    pub code: Option<String>,
    pub status: Option<u16>,
    pub request_id: Option<String>,
}

fn error(message: impl Into<String>, failure: Option<DescribeKeyFailure>) -> Box<ProviderError> {
    let failure = failure.unwrap_or(DescribeKeyFailure {
        code: None,
        status: None,
        request_id: None,
    });
    Box::new(ProviderError {
        kind: ProviderErrorKind::InvalidInput,
        message: message.into(),
        resource_id: None,
        cause: None,
        provider_name: None,
        operation: Some("kms.DescribeKey".into()),
        code: failure.code,
        status: failure.status,
        request_id: failure.request_id,
    })
}

/// Reject partitions without a supported, reachable endpoint in this instance.
fn partition(region: &str) -> Option<&'static str> {
    if region.starts_with("cn-") {
        Some("aws-cn")
    } else if region.starts_with("us-gov-") {
        Some("aws-us-gov")
    } else if region.starts_with("us-iso") || region.starts_with("eu-iso") {
        None
    } else {
        Some("aws")
    }
}

/// Pure resolution over an injected DescribeKey client. No payload or state is rewritten.
pub async fn resolve_kms_key_reference(
    client: &impl DescribeKeyClient,
    instance_region: &str,
    raw: &str,
) -> Result<ReferenceOutcome, ReferenceError> {
    crate::validate_kms_key_reference(raw)
        .map_err(|reason| ReferenceError::InvalidReference(error(reason, None)))?;
    let mut region = instance_region;
    if raw.starts_with("arn:") {
        let parts: Vec<_> = raw.splitn(6, ':').collect();
        if partition(instance_region) != Some(parts[1]) || partition(parts[3]) != Some(parts[1]) {
            return Err(ReferenceError::InvalidReference(error(
                "KMS ARN partition is unreachable from this provider instance",
                None,
            )));
        }
        region = parts[3];
        if parts[5].starts_with("key/") {
            crate::validate_kms_key_arn(raw)
                .map_err(|reason| ReferenceError::InvalidReference(error(reason, None)))?;
            return Ok(ReferenceOutcome::Resolved(raw.to_string()));
        }
    } else if partition(instance_region).is_none() {
        return Err(ReferenceError::InvalidReference(error(
            "KMS partition is unreachable from this provider instance",
            None,
        )));
    }
    match client.describe_key(region, raw).await {
        Ok(Some(arn)) if crate::validate_kms_key_arn(&arn).is_ok() => {
            Ok(ReferenceOutcome::Resolved(arn))
        }
        Ok(_) => Err(ReferenceError::InvalidReference(error(
            "DescribeKey response must contain a valid KmsKeyArn",
            None,
        ))),
        Err(failure) => classify_failure(failure),
    }
}

fn classify_failure(failure: DescribeKeyFailure) -> Result<ReferenceOutcome, ReferenceError> {
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
    let mut error = error("KMS reference resolution failed", Some(failure));
    error.kind = ProviderErrorKind::ApiError;
    if not_found {
        Ok(ReferenceOutcome::NotFound(error))
    } else if denied {
        Err(ReferenceError::AccessDenied(error))
    } else if invalid {
        error.kind = ProviderErrorKind::InvalidInput;
        Err(ReferenceError::InvalidReference(error))
    } else {
        Err(ReferenceError::Transient(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_provider_protocol::types::{ReferenceError, ReferenceOutcome};
    use std::sync::Mutex;

    const KEY_ID: &str = "1234abcd-12ab-34cd-56ef-1234567890ab";
    const KEY_ARN: &str =
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab";

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
            matches!(resolve_kms_key_reference(&fake, "us-west-2", KEY_ARN).await,
            Ok(ReferenceOutcome::Resolved(arn)) if arn == KEY_ARN)
        );
        assert!(fake.calls.lock().unwrap().is_empty());
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
                matches!(resolve_kms_key_reference(&fake, "us-west-2", raw).await,
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
            matches!(resolve_kms_key_reference(&fake, "us-east-1", raw).await,
            Ok(ReferenceOutcome::Resolved(arn)) if arn == canonical)
        );
        assert_eq!(
            *fake.calls.lock().unwrap(),
            [("eu-west-1".into(), raw.into())]
        );
    }

    #[tokio::test]
    async fn unreachable_partitions_and_malformed_inputs_never_call_kms() {
        for raw in [
            "arn:aws-cn:kms:cn-north-1:123456789012:alias/a",
            "arn:aws-us-gov:kms:us-gov-west-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
            "arn:aws-iso:kms:us-iso-east-1:123456789012:alias/a",
            "arn:aws:kms:cn-north-1:123456789012:alias/a",
            "alias/",
            "not-a-key",
            "arn:aws:kms::123:key/",
            "arn:aws:s3:::bucket",
        ] {
            let fake = client(Ok(Some(KEY_ARN.into())));
            assert!(
                matches!(
                    resolve_kms_key_reference(&fake, "us-east-1", raw).await,
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
                resolve_kms_key_reference(&fake, "us-east-1", "alias/a").await,
                Err(ReferenceError::InvalidReference(_))
            ));
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
                code: code.map(str::to_owned),
                status,
                request_id: Some("request-123".into()),
            }));
            let result = resolve_kms_key_reference(&fake, "us-east-1", "alias/aws/sqs").await;
            let (actual, error) = match result {
                Ok(ReferenceOutcome::NotFound(e)) => ("not-found", e),
                Err(ReferenceError::AccessDenied(e)) => ("denied", e),
                Err(ReferenceError::Transient(e)) => ("transient", e),
                Err(ReferenceError::InvalidReference(e)) => ("invalid", e),
                other => panic!("unexpected {other:?}"),
            };
            assert_eq!(actual, expected, "{code:?}");
            assert_eq!(error.code.as_deref(), code);
            assert_eq!(error.status, status);
            assert_eq!(error.request_id.as_deref(), Some("request-123"));
            assert_eq!(fake.calls.lock().unwrap().len(), 1);
        }
    }
}
