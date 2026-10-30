use std::io::{self, Write};

use crate::AwsProvider;
use crate::error_helpers::{IntoSendFailure, api_error_with_meta};
use crate::helpers::{RetryPolicy, retry_aws_operation};
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketVersioningReadModel;
use crate::send_models_generated::{S3BucketVersioningCreateInput, S3BucketVersioningUpdateInput};
use crate::services::s3::bucket::is_s3_not_configured_error;
use crate::services::s3::{InvalidBucketStateReason, classify_invalid_bucket_state};
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::operation::put_bucket_versioning::PutBucketVersioningError;
use aws_sdk_s3::types::{BucketVersioningStatus, VersioningConfiguration};
use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
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
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let Some(status) = output.status() else {
                    return Ok(ReadOutcome::not_found(id.clone()));
                };
                let model = S3BucketVersioningReadModel {
                    bucket: crate::read_model::applied_identifier_value(identifier),
                    status: Value::Concrete(ConcreteValue::String(
                        status.as_str().to_string().into(),
                    )),
                };
                found_managed_read(id, applied_identifier, model)
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to get bucket versioning", e)
                    .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_versioning(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = S3BucketVersioningCreateInput::decode(resource)?;
        let sent = input.sent();
        let (bucket, status) = input.into_parts();
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketVersioning", bucket)?;
        let outcome = self
            .put_s3_bucket_versioning(&resource.id, &identifier, status)
            .await?;
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

    pub(crate) async fn update_s3_bucket_versioning(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let (status,) = S3BucketVersioningUpdateInput::decode(&to)?.into_parts();
        self.put_s3_bucket_versioning(&id, identifier, status).await
    }

    async fn put_s3_bucket_versioning(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        status: String,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let status = BucketVersioningStatus::from(status.as_str());

        let config_builder = VersioningConfiguration::builder().status(status);

        self.s3_client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(config_builder.build())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket versioning", e).for_resource(id.clone())
            })?;

        Ok(self.read_s3_bucket_versioning(id, applied_identifier).await)
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
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client
                .put_bucket_versioning()
                .bucket(identifier)
                .versioning_configuration(
                    VersioningConfiguration::builder()
                        .status(BucketVersioningStatus::Suspended)
                        .build(),
                )
        })
        .await;
        finish_s3_bucket_versioning_suspend(id, identifier, result)
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

fn finish_s3_bucket_versioning_suspend<Failure>(
    id: ResourceId,
    identifier: &str,
    result: Result<
        aws_sdk_s3::operation::put_bucket_versioning::PutBucketVersioningOutput,
        Failure,
    >,
) -> ProviderResult<()>
where
    Failure: IntoSendFailure<Error = PutBucketVersioningError>,
{
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            let error = error.into_send_failure();
            match classify_suspension_error(error.code(), error.message()) {
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
                    error,
                )
                .for_resource(id)),
            }
        }
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
    use aws_sdk_s3::error::SdkError;
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

    #[test]
    fn object_lock_invalid_bucket_state_allows_versioning_teardown_to_continue() {
        finish_s3_bucket_versioning_suspend(
            versioning_id(),
            "locked-bucket",
            Err(suspension_service_error(
                "InvalidBucketState",
                "An Object Lock configuration is present on this bucket, so the versioning state cannot be changed.",
            )),
        )
        .expect("Object Lock must not block versioning resource teardown");
    }

    #[test]
    fn unrelated_invalid_bucket_state_still_fails_versioning_teardown() {
        let error = finish_s3_bucket_versioning_suspend(
            versioning_id(),
            "unlocked-bucket",
            Err(suspension_service_error(
                "InvalidBucketState",
                "another invalid bucket state",
            )),
        )
        .expect_err("unrelated InvalidBucketState must remain an error");

        assert!(
            error.to_string().contains("another invalid bucket state"),
            "{error}"
        );
    }
}
