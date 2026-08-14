use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, optional_enum_attr, require_enum_attr, require_string_attr, retry_aws_operation,
};
use crate::services::s3::bucket::is_s3_not_configured_error;
use crate::services::s3::{InvalidBucketStateReason, classify_invalid_bucket_state};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::put_bucket_versioning::{
    PutBucketVersioningError, PutBucketVersioningOutput,
};
use aws_sdk_s3::types::{BucketVersioningStatus, MfaDelete, VersioningConfiguration};
use carina_core::provider::ProviderResult;
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

impl AwsProvider {
    /// Read an S3 BucketVersioning.
    ///
    /// `identifier` is the bucket name. AWS returns `Status = None` when
    /// versioning has never been enabled; we treat that as `not_found` so a
    /// destroyed-then-redeclared resource gets a clean re-create.
    pub(crate) async fn read_s3_bucket_versioning(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let Some(status) = output.status() else {
                    return Ok(State::not_found(id.clone()));
                };
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                attributes.insert(
                    "status".to_string(),
                    Value::Concrete(ConcreteValue::String(status.as_str().to_string())),
                );
                if let Some(mfa) = output.mfa_delete() {
                    attributes.insert(
                        "mfa_delete".to_string(),
                        Value::Concrete(ConcreteValue::String(mfa.as_str().to_string())),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to get bucket versioning",
                    "s3.GetBucketVersioning",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_versioning(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(resource, "bucket")?;
        self.put_s3_bucket_versioning(&resource.id, &bucket, resource, schema)
            .await
    }

    pub(crate) async fn update_s3_bucket_versioning(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_versioning(&id, identifier, &to, schema)
            .await
    }

    async fn put_s3_bucket_versioning(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<State> {
        let status_str = require_enum_attr(resource, schema, "status")?;
        let status = BucketVersioningStatus::from(status_str.as_str());

        let mut config_builder = VersioningConfiguration::builder().status(status);
        if let Some(s) = optional_enum_attr(resource, schema, "mfa_delete") {
            config_builder = config_builder.mfa_delete(MfaDelete::from(s));
        }

        self.s3_client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(config_builder.build())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to put bucket versioning",
                    "s3.PutBucketVersioning",
                    e,
                )
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_versioning(id, Some(bucket)).await
    }

    /// "Delete" an S3 BucketVersioning by suspending it.
    ///
    /// There is no DeleteBucketVersioning API: once enabled, versioning can
    /// only be Suspended. This matches Terraform's `aws_s3_bucket_versioning`
    /// destroy behaviour. NoSuchBucket is treated as success so that a
    /// destroy retry after the parent bucket is gone still satisfies its goal.
    pub(crate) async fn delete_s3_bucket_versioning_suspend(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        delete_s3_bucket_versioning_suspend_with(id, identifier, || {
            let client = &self.s3_client;
            async move {
                client
                    .put_bucket_versioning()
                    .bucket(identifier)
                    .versioning_configuration(
                        VersioningConfiguration::builder()
                            .status(BucketVersioningStatus::Suspended)
                            .build(),
                    )
                    .send()
                    .await
            }
        })
        .await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuspensionErrorKind {
    BucketMissing,
    ObjectLockPresent,
    Other,
}

fn classify_suspension_error(code: Option<&str>, message: Option<&str>) -> SuspensionErrorKind {
    if code == Some("NoSuchBucket") {
        return SuspensionErrorKind::BucketMissing;
    }

    match classify_invalid_bucket_state(code, message) {
        InvalidBucketStateReason::ObjectLockConfigurationPresent => {
            SuspensionErrorKind::ObjectLockPresent
        }
        InvalidBucketStateReason::VersioningNotEnabled | InvalidBucketStateReason::Other => {
            SuspensionErrorKind::Other
        }
    }
}

async fn delete_s3_bucket_versioning_suspend_with<F, Fut>(
    id: ResourceId,
    identifier: &str,
    suspend: F,
) -> ProviderResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<PutBucketVersioningOutput, SdkError<PutBucketVersioningError>>>,
{
    let result =
        retry_aws_operation("suspend bucket versioning", RetryPolicy::default(), suspend).await;

    match result {
        Ok(_) => Ok(()),
        Err(error) => match classify_suspension_error(error.code(), error.message()) {
            SuspensionErrorKind::BucketMissing => Ok(()),
            SuspensionErrorKind::ObjectLockPresent => {
                // Once Object Lock was enabled, PutBucketVersioning with
                // Status=Suspended returned InvalidBucketState. Measured in
                // us-east-1 on 2026-08-14; direction checked was Object Lock
                // enabled -> versioning suspension rejected. Ignore this
                // suspension failure so destroy can continue to the parent;
                // parent deletion can still fail when retained versions exist.
                let _ = write_suspension_warning(io::stderr().lock(), identifier);
                Ok(())
            }
            SuspensionErrorKind::Other => Err(api_error_with_meta(
                "Failed to suspend bucket versioning",
                "s3.PutBucketVersioning",
                error,
            )
            .for_resource(id)),
        },
    }
}

fn write_suspension_warning(mut writer: impl Write, bucket: &str) -> io::Result<()> {
    writeln!(
        writer,
        "Warning: versioning could not be suspended on S3 bucket '{bucket}' because Object Lock is present; deleting the bucket will fail if any object versions remain under retention."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    fn suspension_service_error(code: &str, message: &str) -> SdkError<PutBucketVersioningError> {
        let error = PutBucketVersioningError::generic(
            ErrorMetadata::builder().code(code).message(message).build(),
        );
        let response = HttpResponse::new(409.try_into().unwrap(), SdkBody::empty());
        SdkError::service_error(error, response)
    }

    fn versioning_id() -> ResourceId {
        ResourceId::with_provider_identity("aws", "s3.BucketVersioning", "test", None)
    }

    #[test]
    fn suspension_warning_states_the_remaining_bucket_deletion_risk() {
        let mut output = Vec::new();

        write_suspension_warning(&mut output, "locked-bucket").expect("write warning");

        let warning = String::from_utf8(output).expect("warning is UTF-8");
        assert!(warning.contains("locked-bucket"), "{warning}");
        assert!(
            warning.contains("versioning could not be suspended"),
            "{warning}"
        );
        assert!(
            warning.contains(
                "deleting the bucket will fail if any object versions remain under retention"
            ),
            "{warning}"
        );
    }

    #[tokio::test]
    async fn object_lock_invalid_bucket_state_allows_versioning_teardown_to_continue() {
        delete_s3_bucket_versioning_suspend_with(
            versioning_id(),
            "locked-bucket",
            || async {
                Err(suspension_service_error(
                    "InvalidBucketState",
                    "An Object Lock configuration is present on this bucket, so the versioning state cannot be changed.",
                ))
            },
        )
        .await
        .expect("Object Lock must not block versioning resource teardown");
    }

    #[tokio::test]
    async fn unrelated_invalid_bucket_state_still_fails_versioning_teardown() {
        let error = delete_s3_bucket_versioning_suspend_with(
            versioning_id(),
            "unlocked-bucket",
            || async {
                Err(suspension_service_error(
                    "InvalidBucketState",
                    "another invalid bucket state",
                ))
            },
        )
        .await
        .expect_err("unrelated InvalidBucketState must remain an error");

        assert!(
            error.to_string().contains("another invalid bucket state"),
            "{error}"
        );
    }
}
