use std::collections::HashMap;
use std::io::{self, Write};

use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::put_object_lock_configuration::{
    PutObjectLockConfigurationError, PutObjectLockConfigurationOutput,
};
use aws_sdk_s3::types::{
    BucketVersioningStatus, DefaultRetention, ObjectLockConfiguration, ObjectLockEnabled,
    ObjectLockRetentionMode, ObjectLockRule,
};
use carina_core::provider::{AppliedIdentifier, CreateOutcome, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    ConcreteValue, Resource, ResourceId, State, Value, enum_attr_at_schema_path,
};
use carina_core::schema::ResourceSchema;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{enum_attr_error, require_enum_attr, require_string_attr};
use crate::services::s3::bucket::is_s3_not_configured_error;
use crate::services::s3::{InvalidBucketStateReason, classify_invalid_bucket_state};

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_object_lock_configuration(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_object_lock_configuration()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let Some(configuration) = output.object_lock_configuration() else {
                    return Ok(ReadOutcome::not_found(id.clone()));
                };
                let Some(enabled) = configuration.object_lock_enabled() else {
                    return Ok(ReadOutcome::not_found(id.clone()));
                };
                let mut attributes = HashMap::new();
                attributes.insert("bucket".to_string(), string_value(bucket));
                attributes.insert(
                    "object_lock_enabled".to_string(),
                    string_value(enabled.as_str()),
                );
                if let Some(rule) = configuration.rule() {
                    attributes.insert("rule".to_string(), rule_to_value(id, rule)?);
                }

                Ok(crate::helpers::found_read(
                    id,
                    applied_identifier,
                    attributes,
                ))
            }
            Err(error)
                if is_s3_not_configured_error(&error, "ObjectLockConfigurationNotFoundError")
                    || is_s3_not_configured_error(&error, "NoSuchBucket") =>
            {
                Ok(ReadOutcome::not_found(id.clone()))
            }
            Err(error) => Err(api_error_with_meta(
                "Failed to get bucket Object Lock configuration",
                "s3.GetObjectLockConfiguration",
                error,
            )
            .for_resource(id.clone())),
        }
    }

    pub(crate) async fn create_s3_bucket_object_lock_configuration(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateOutcome> {
        let bucket = require_string_attr(resource, "bucket")?;
        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "s3.PutObjectLockConfiguration",
            bucket,
        )?;
        self.ensure_create_does_not_omit_existing_rule(&resource.id, identifier.as_str(), resource)
            .await?;
        let outcome = self
            .put_s3_bucket_object_lock_configuration(&resource.id, &identifier, resource, schema)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    async fn ensure_create_does_not_omit_existing_rule(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<()> {
        if resource.get_attr("rule").is_some() {
            return Ok(());
        }

        // Create can follow a state-only destroy. Because Put without Rule
        // clears a retained default (measured at the Put path below), absence
        // in the desired resource is not evidence that AWS has no Rule.
        let result = self
            .s3_client
            .get_object_lock_configuration()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output)
                if output
                    .object_lock_configuration()
                    .and_then(|configuration| configuration.rule())
                    .is_some() =>
            {
                Err(ProviderError::invalid_input(format!(
                    "S3 bucket '{bucket}' already has an Object Lock default retention rule; creating this resource without 'rule' would remove it. Import/adopt the existing configuration or declare 'rule' explicitly"
                ))
                .for_resource(id.clone()))
            }
            Ok(_) => Ok(()),
            Err(error)
                if is_s3_not_configured_error(
                    &error,
                    "ObjectLockConfigurationNotFoundError",
                ) || is_s3_not_configured_error(&error, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(error) => Err(api_error_with_meta(
                "Failed to check existing bucket Object Lock configuration before create",
                "s3.GetObjectLockConfiguration",
                error,
            )
            .for_resource(id.clone())),
        }
    }

    pub(crate) async fn update_s3_bucket_object_lock_configuration(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.put_s3_bucket_object_lock_configuration(&id, identifier, &to, schema)
            .await
    }

    async fn put_s3_bucket_object_lock_configuration(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        self.ensure_object_lock_versioning_enabled(id, bucket)
            .await?;

        let enabled = require_enum_attr(resource, schema, "object_lock_enabled")?;
        let mut configuration = ObjectLockConfiguration::builder()
            .object_lock_enabled(ObjectLockEnabled::from(enabled.as_str()));

        // A Put with Rule followed by a Put without Rule removed the bucket
        // default, and Get reflected the change. Measured in us-east-1 on
        // 2026-08-14, with present -> absent and absent -> present both
        // checked. Omitting Rule here is therefore a real in-place update;
        // create separately rejects that omission when AWS already has a Rule.
        if let Some(rule) = resource.get_attr("rule") {
            configuration = configuration.rule(build_rule(id, schema, rule)?);
        }

        let result = self
            .s3_client
            .put_object_lock_configuration()
            .bucket(bucket)
            .object_lock_configuration(configuration.build())
            .send()
            .await;

        map_put_object_lock_configuration_result(id, bucket, result)?;

        // Reapplying an identical configuration and changing
        // GOVERNANCE/Days=30 -> COMPLIANCE/Years=1 both converged in place.
        // Measured in us-east-1 on 2026-08-14, with no-op and changed update
        // directions checked. Always read back the authoritative result.
        Ok(self
            .read_s3_bucket_object_lock_configuration(id, applied_identifier)
            .await)
    }

    async fn ensure_object_lock_versioning_enabled(
        &self,
        id: &ResourceId,
        bucket: &str,
    ) -> ProviderResult<()> {
        let output = self
            .s3_client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await
            .map_err(|error| {
                api_error_with_meta(
                    "Failed to check bucket versioning before enabling Object Lock",
                    "s3.GetBucketVersioning",
                    error,
                )
                .for_resource(id.clone())
            })?;

        // PutObjectLockConfiguration without versioning returned
        // InvalidBucketState: Versioning must be 'Enabled' on the bucket.
        // Measured in us-east-1 on 2026-08-14; disabled -> rejected and
        // enabled -> accepted directions were both checked.
        if output.status() != Some(&BucketVersioningStatus::Enabled) {
            return Err(versioning_required_error(id, bucket));
        }
        Ok(())
    }

    pub(crate) async fn delete_s3_bucket_object_lock_configuration_noop(
        &self,
        _id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        // Object Lock enablement proved irreversible: after enabling it,
        // PutBucketVersioning Status=Suspended returned InvalidBucketState.
        // Measured in us-east-1 on 2026-08-14; direction checked was Object
        // Lock enabled -> versioning suspension rejected. AWS has no disable
        // call, so destroy intentionally performs no AWS request.
        //
        // After this no-op, Get still returned the full configuration and an
        // identical update Put was idempotent. Measured in us-east-1 on
        // 2026-08-14; the paths checked were destroy -> read and destroy ->
        // read -> update. An unguarded destroy -> create without Rule can clear
        // that retained default, so create now reads first and rejects it.
        let _ = write_destroy_warning(io::stderr().lock(), identifier);
        Ok(())
    }
}

fn versioning_required_error(id: &ResourceId, bucket: &str) -> ProviderError {
    ProviderError::invalid_input(format!(
        "S3 bucket '{bucket}' cannot enable Object Lock: versioning must be enabled first"
    ))
    .for_resource(id.clone())
}

fn map_put_object_lock_configuration_result(
    id: &ResourceId,
    bucket: &str,
    result: Result<PutObjectLockConfigurationOutput, SdkError<PutObjectLockConfigurationError>>,
) -> ProviderResult<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error) => match classify_invalid_bucket_state(error.code(), error.message()) {
            // Put without enabled versioning returned this code and message in
            // us-east-1 on 2026-08-14; both disabled -> rejected and enabled
            // -> accepted directions were checked. Keep this API-side mapping
            // for a versioning change after the precheck, but preserve other
            // InvalidBucketState errors because the code is ambiguous.
            InvalidBucketStateReason::VersioningNotEnabled => {
                Err(versioning_required_error(id, bucket))
            }
            InvalidBucketStateReason::ObjectLockConfigurationPresent
            | InvalidBucketStateReason::Other => Err(api_error_with_meta(
                "Failed to put bucket Object Lock configuration",
                "s3.PutObjectLockConfiguration",
                error,
            )
            .for_resource(id.clone())),
        },
    }
}

fn build_rule(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<ObjectLockRule> {
    let Value::Concrete(ConcreteValue::Map(rule)) = value else {
        return Err(ProviderError::invalid_input("rule must be a map").for_resource(id.clone()));
    };

    let mut builder = ObjectLockRule::builder();
    if let Some(default_retention) = rule.get("default_retention") {
        builder =
            builder.default_retention(build_default_retention(id, schema, default_retention)?);
    }
    Ok(builder.build())
}

fn build_default_retention(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<DefaultRetention> {
    let Value::Concrete(ConcreteValue::Map(default_retention)) = value else {
        return Err(
            ProviderError::invalid_input("default_retention must be a map")
                .for_resource(id.clone()),
        );
    };

    let mode = enum_attr_at_schema_path(
        schema,
        "rule",
        &["default_retention", "mode"],
        default_retention.get("mode"),
    )
    .map_err(|error| enum_attr_error(id, error))?
    .ok_or_else(|| {
        ProviderError::invalid_input("default_retention.mode is required").for_resource(id.clone())
    })?;
    let Value::Concrete(ConcreteValue::Map(period)) =
        default_retention.get("retention_period").ok_or_else(|| {
            ProviderError::invalid_input("default_retention.retention_period is required")
                .for_resource(id.clone())
        })?
    else {
        return Err(ProviderError::invalid_input(
            "default_retention.retention_period must be a map",
        )
        .for_resource(id.clone()));
    };

    let days = optional_i32(id, period, "days")?;
    let years = optional_i32(id, period, "years")?;
    let mut builder =
        DefaultRetention::builder().mode(ObjectLockRetentionMode::from(mode.api_value()));
    // Both invalid concrete shapes are reachable through Provider::create:
    // two literal integers reach `(Some, Some)`, and an empty map reaches
    // `(None, None)`. Provider-level tests exercise those exact values.
    match (days, years) {
        (Some(days), None) => builder = builder.days(days),
        (None, Some(years)) => builder = builder.years(years),
        (Some(_), Some(_)) => {
            return Err(ProviderError::invalid_input(
                "default_retention.retention_period must contain exactly one of days or years",
            )
            .for_resource(id.clone()));
        }
        (None, None) => {
            return Err(ProviderError::invalid_input(
                "default_retention.retention_period must contain days or years",
            )
            .for_resource(id.clone()));
        }
    }

    Ok(builder.build())
}

fn optional_i32(
    id: &ResourceId,
    values: &IndexMap<String, Value>,
    field: &str,
) -> ProviderResult<Option<i32>> {
    let Some(value) = values.get(field) else {
        return Ok(None);
    };
    let Value::Concrete(ConcreteValue::Int(value)) = value else {
        return Err(ProviderError::invalid_input(format!(
            "default_retention.retention_period.{field} must be an integer"
        ))
        .for_resource(id.clone()));
    };
    i32::try_from(*value).map(Some).map_err(|_| {
        ProviderError::invalid_input(format!(
            "default_retention.retention_period.{field} is outside the S3 integer range"
        ))
        .for_resource(id.clone())
    })
}

fn rule_to_value(id: &ResourceId, rule: &ObjectLockRule) -> ProviderResult<Value> {
    let mut rule_value = IndexMap::new();
    if let Some(default_retention) = rule.default_retention() {
        rule_value.insert(
            "default_retention".to_string(),
            default_retention_to_value(id, default_retention)?,
        );
    }
    Ok(Value::Concrete(ConcreteValue::Map(rule_value)))
}

fn default_retention_to_value(
    id: &ResourceId,
    retention: &DefaultRetention,
) -> ProviderResult<Value> {
    let mode = retention.mode().ok_or_else(|| {
        ProviderError::internal("S3 returned Object Lock default retention without mode")
            .for_resource(id.clone())
    })?;
    let mut retention_value = IndexMap::new();
    retention_value.insert("mode".to_string(), string_value(mode.as_str()));

    let mut period = IndexMap::new();
    match (retention.days(), retention.years()) {
        (Some(days), None) => {
            period.insert(
                "days".to_string(),
                Value::Concrete(ConcreteValue::Int(i64::from(days))),
            );
        }
        (None, Some(years)) => {
            period.insert(
                "years".to_string(),
                Value::Concrete(ConcreteValue::Int(i64::from(years))),
            );
        }
        (Some(_), Some(_)) => {
            return Err(ProviderError::internal(
                "S3 returned Object Lock default retention with both days and years",
            )
            .for_resource(id.clone()));
        }
        (None, None) => {
            return Err(ProviderError::internal(
                "S3 returned Object Lock default retention without days or years",
            )
            .for_resource(id.clone()));
        }
    }
    retention_value.insert(
        "retention_period".to_string(),
        Value::Concrete(ConcreteValue::Map(period)),
    );
    Ok(Value::Concrete(ConcreteValue::Map(retention_value)))
}

fn string_value(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn write_destroy_warning(mut writer: impl Write, bucket: &str) -> io::Result<()> {
    writeln!(
        writer,
        "Warning: destroy removed only the Carina state entry for S3 bucket '{bucket}' and made no AWS request. Any remote Object Lock configuration remains; if a default retention rule exists, import/adopt the configuration or declare 'rule' explicitly before re-creating the resource."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    fn put_service_error(code: &str, message: &str) -> SdkError<PutObjectLockConfigurationError> {
        let error = PutObjectLockConfigurationError::generic(
            ErrorMetadata::builder().code(code).message(message).build(),
        );
        let response = HttpResponse::new(409.try_into().unwrap(), SdkBody::empty());
        SdkError::service_error(error, response)
    }

    fn object_lock_id() -> ResourceId {
        ResourceId::with_provider_identity("aws", "s3.BucketObjectLockConfiguration", "test", None)
    }

    #[test]
    fn invalid_default_retention_mode_is_rejected() {
        let schema = crate::schemas::generated::s3::bucket_object_lock_configuration::s3_bucket_object_lock_configuration_config().schema;
        let value = Value::Concrete(ConcreteValue::Map(
            [(
                "mode".to_string(),
                Value::Concrete(ConcreteValue::String("governance".into())),
            )]
            .into_iter()
            .collect(),
        ));

        build_default_retention(&object_lock_id(), &schema, &value)
            .expect_err("a non-exact closed-enum leaf must be rejected");
    }

    fn assert_original_put_error(error: &ProviderError, expected_code: &str, expected: &str) {
        let ProviderError::ApiError(detail) = error else {
            panic!("expected ApiError, got {error:?}");
        };

        assert_eq!(
            detail.operation.as_deref(),
            Some("s3.PutObjectLockConfiguration")
        );
        assert_eq!(detail.code.as_deref(), Some(expected_code));
        let rendered = error.to_string();
        assert!(
            rendered.contains(expected),
            "original AWS message must be rendered: {rendered}"
        );
        assert!(
            detail.cause().is_some(),
            "original SDK error must be retained"
        );
        assert!(
            !error
                .to_string()
                .contains("versioning must be enabled first"),
            "{error}"
        );
    }

    #[test]
    fn read_converter_rejects_sdk_period_with_both_fields() {
        let retention = DefaultRetention::builder()
            .mode(ObjectLockRetentionMode::Governance)
            .days(30)
            .years(1)
            .build();

        let error = default_retention_to_value(&object_lock_id(), &retention)
            .expect_err("an SDK value with both periods must be rejected");

        assert!(matches!(&error, ProviderError::Internal(_)));
        assert_eq!(
            error.detail().context(),
            "S3 returned Object Lock default retention with both days and years"
        );
    }

    #[test]
    fn read_converter_rejects_sdk_period_with_neither_field() {
        let retention = DefaultRetention::builder()
            .mode(ObjectLockRetentionMode::Governance)
            .build();

        let error = default_retention_to_value(&object_lock_id(), &retention)
            .expect_err("an SDK value without a period must be rejected");

        assert!(matches!(&error, ProviderError::Internal(_)));
        assert_eq!(
            error.detail().context(),
            "S3 returned Object Lock default retention without days or years"
        );
    }

    #[test]
    fn put_versioning_invalid_bucket_state_maps_to_actionable_error() {
        let error = map_put_object_lock_configuration_result(
            &object_lock_id(),
            "unversioned-bucket",
            Err(put_service_error(
                "InvalidBucketState",
                "Versioning must be 'Enabled' on the bucket.",
            )),
        )
        .expect_err("versioning rejection must be actionable");

        assert!(
            error
                .to_string()
                .contains("versioning must be enabled first"),
            "{error}"
        );
    }

    #[test]
    fn put_unrelated_invalid_bucket_state_preserves_aws_error() {
        let aws_message = "The bucket cannot accept this Object Lock transition.";
        let error = map_put_object_lock_configuration_result(
            &object_lock_id(),
            "versioned-bucket",
            Err(put_service_error("InvalidBucketState", aws_message)),
        )
        .expect_err("unrelated InvalidBucketState must remain an API error");

        assert_original_put_error(&error, "InvalidBucketState", aws_message);
    }

    #[test]
    fn put_versioning_message_with_other_code_preserves_aws_error() {
        let aws_message = "Versioning must be 'Enabled' on the bucket.";
        let error = map_put_object_lock_configuration_result(
            &object_lock_id(),
            "versioned-bucket",
            Err(put_service_error("AccessDenied", aws_message)),
        )
        .expect_err("the message alone must not trigger versioning mapping");

        assert_original_put_error(&error, "AccessDenied", aws_message);
    }

    #[test]
    fn destroy_warning_names_bucket_and_preserved_remote_settings() {
        let mut output = Vec::new();

        write_destroy_warning(&mut output, "locked-bucket").expect("write warning");

        let warning = String::from_utf8(output).expect("warning is UTF-8");
        assert!(warning.contains("locked-bucket"), "{warning}");
        assert!(warning.contains("made no AWS request"), "{warning}");
        assert!(
            warning.contains("Any remote Object Lock configuration remains"),
            "{warning}"
        );
        assert!(
            warning.contains("import/adopt the configuration"),
            "{warning}"
        );
        assert!(warning.contains("declare 'rule' explicitly"), "{warning}");
    }
}
