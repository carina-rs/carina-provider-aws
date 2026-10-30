use aws_sdk_s3::types::{
    ServerSideEncryption, ServerSideEncryptionByDefault, ServerSideEncryptionConfiguration,
    ServerSideEncryptionRule,
};
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    ConcreteValue, Resource, ResourceId, State, Value, enum_attr_at_schema_path,
};
use carina_core::schema::ResourceSchema;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, enum_attr_error, retry_aws_operation, sdk_error_message};
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketServerSideEncryptionConfigurationReadModel;
use crate::send_models_generated::{
    S3BucketServerSideEncryptionConfigurationCreateInput,
    S3BucketServerSideEncryptionConfigurationUpdateInput,
};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_server_side_encryption_configuration(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_encryption()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let config = output
                    .server_side_encryption_configuration()
                    .ok_or_else(|| {
                        ProviderError::api_error(
                            "GetBucketEncryption returned no encryption configuration",
                        )
                        .for_resource(id.clone())
                    })?;
                let rules = config.rules().iter().map(rule_to_value).collect();
                let model = S3BucketServerSideEncryptionConfigurationReadModel {
                    bucket: crate::read_model::applied_identifier_value(identifier),
                    rules: Value::Concrete(ConcreteValue::List(rules)),
                };
                found_managed_read(id, applied_identifier, model)
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "ServerSideEncryptionConfigurationNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to get bucket encryption", e)
                    .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_server_side_encryption_configuration(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = S3BucketServerSideEncryptionConfigurationCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::S3BucketServerSideEncryptionConfigurationCreateParts {
            bucket,
            rules,
        } = input.into_parts();
        let bucket = bucket.into_inner();
        let rules = rules.into_inner();
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketEncryption", bucket)?;
        let outcome = self
            .put_s3_bucket_encryption(&resource.id, &identifier, &rules, schema)
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

    pub(crate) async fn update_s3_bucket_server_side_encryption_configuration(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let crate::send_models_generated::S3BucketServerSideEncryptionConfigurationUpdateParts {
            rules,
        } = S3BucketServerSideEncryptionConfigurationUpdateInput::decode(&to)?.into_parts();
        let rules = rules.into_inner();
        self.put_s3_bucket_encryption(&id, identifier, &rules, schema)
            .await
    }

    async fn put_s3_bucket_encryption(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        rules: &Value,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let rules = match rules {
            Value::Concrete(ConcreteValue::List(items)) => items,
            _ => {
                return Err(
                    ProviderError::invalid_input("rules is required and must be a list")
                        .for_resource(id.clone()),
                );
            }
        };

        let sdk_rules: Vec<ServerSideEncryptionRule> = rules
            .iter()
            .map(|v| build_rule(id, schema, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = ServerSideEncryptionConfiguration::builder()
            .set_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build server-side encryption configuration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_encryption()
            .bucket(bucket)
            .server_side_encryption_configuration(config)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket encryption", e).for_resource(id.clone())
            })?;

        Ok(self
            .read_s3_bucket_server_side_encryption_configuration(id, applied_identifier)
            .await)
    }

    pub(crate) async fn delete_s3_bucket_server_side_encryption_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_encryption().bucket(identifier)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(
                    &e,
                    "ServerSideEncryptionConfigurationNotFoundError",
                ) || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(api_error_with_meta("Failed to delete bucket encryption", e)
                .for_resource(id.clone())),
        }
    }
}

fn build_rule(
    id: &ResourceId,
    schema: &ResourceSchema,
    rule_value: &Value,
) -> ProviderResult<ServerSideEncryptionRule> {
    let Value::Concrete(ConcreteValue::Map(rule_map)) = rule_value else {
        return Err(
            ProviderError::invalid_input("each rule must be a map").for_resource(id.clone())
        );
    };

    let mut builder = ServerSideEncryptionRule::builder();

    if let Some(by_default) = rule_map.get("apply_server_side_encryption_by_default") {
        builder = builder
            .apply_server_side_encryption_by_default(build_by_default(id, schema, by_default)?);
    }
    if let Some(Value::Concrete(ConcreteValue::Bool(b))) = rule_map.get("bucket_key_enabled") {
        builder = builder.bucket_key_enabled(*b);
    }

    Ok(builder.build())
}

fn build_by_default(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<ServerSideEncryptionByDefault> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input(
            "apply_server_side_encryption_by_default must be a map",
        )
        .for_resource(id.clone()));
    };
    let algorithm = enum_attr_at_schema_path(
        schema,
        "rules",
        &["apply_server_side_encryption_by_default", "sse_algorithm"],
        map.get("sse_algorithm"),
    )
    .map_err(|error| enum_attr_error(id, error))?
    .ok_or_else(|| {
        ProviderError::invalid_input("sse_algorithm is required").for_resource(id.clone())
    })?;
    let mut builder = ServerSideEncryptionByDefault::builder()
        .sse_algorithm(ServerSideEncryption::from(algorithm.api_value()));
    if let Some(Value::Concrete(ConcreteValue::String(k))) = map.get("kms_master_key_id") {
        builder = builder.kms_master_key_id(k.expose_plaintext());
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message(
            "Failed to build apply_server_side_encryption_by_default",
            &e,
        ))
        .for_resource(id.clone())
    })
}

fn rule_to_value(rule: &ServerSideEncryptionRule) -> Value {
    let mut map = IndexMap::new();
    if let Some(by_default) = rule.apply_server_side_encryption_by_default() {
        let mut inner = IndexMap::new();
        inner.insert(
            "sse_algorithm".to_string(),
            Value::Concrete(ConcreteValue::String(
                by_default.sse_algorithm().as_str().to_string().into(),
            )),
        );
        if let Some(k) = by_default.kms_master_key_id() {
            inner.insert(
                "kms_master_key_id".to_string(),
                Value::Concrete(ConcreteValue::String(k.to_string().into())),
            );
        }
        map.insert(
            "apply_server_side_encryption_by_default".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
    }
    if let Some(b) = rule.bucket_key_enabled() {
        map.insert(
            "bucket_key_enabled".to_string(),
            Value::Concrete(ConcreteValue::Bool(b)),
        );
    }
    Value::Concrete(ConcreteValue::Map(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_sse_algorithm_is_rejected() {
        let id =
            ResourceId::with_identity("s3.bucket_server_side_encryption_configuration", "test");
        let schema = crate::schemas::generated::s3::bucket_server_side_encryption_configuration::s3_bucket_server_side_encryption_configuration_config().schema;
        let value = Value::Concrete(ConcreteValue::Map(
            [(
                "sse_algorithm".to_string(),
                Value::Concrete(ConcreteValue::String("aes256".into())),
            )]
            .into_iter()
            .collect(),
        ));

        build_by_default(&id, &schema, &value)
            .expect_err("a non-exact closed-enum leaf must be rejected");
    }
}
