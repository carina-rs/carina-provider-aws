use aws_sdk_s3::types::{
    AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, ExpirationStatus,
    LifecycleExpiration, LifecycleRule, LifecycleRuleAndOperator, LifecycleRuleFilter,
    NoncurrentVersionExpiration, NoncurrentVersionTransition, Tag, Transition,
    TransitionStorageClass,
};
use aws_smithy_types::{DateTime, date_time::Format};
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
use crate::read_models_generated::S3BucketLifecycleConfigurationReadModel;
use crate::send_models_generated::{
    S3BucketLifecycleConfigurationCreateInput, S3BucketLifecycleConfigurationUpdateInput,
};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_lifecycle_configuration(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_lifecycle_configuration()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let rules = output
                    .rules()
                    .iter()
                    .map(|rule| rule_to_value(id, rule))
                    .collect::<ProviderResult<Vec<_>>>()?;
                found_managed_read(
                    id,
                    identifier,
                    S3BucketLifecycleConfigurationReadModel {
                        bucket: crate::read_model::applied_identifier_value(identifier),
                        rules: Value::Concrete(ConcreteValue::List(rules)),
                    },
                )
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchLifecycleConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(
                    api_error_with_meta("Failed to get bucket lifecycle configuration", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_lifecycle_configuration(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = S3BucketLifecycleConfigurationCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::S3BucketLifecycleConfigurationCreateParts {
            bucket,
            rules,
        } = input.into_parts();
        let bucket = bucket.into_inner();
        let rules = rules.into_inner();
        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "s3.PutBucketLifecycleConfiguration",
            bucket,
        )?;
        let outcome = self
            .put_s3_bucket_lifecycle(&resource.id, &identifier, &rules, schema)
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

    pub(crate) async fn update_s3_bucket_lifecycle_configuration(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = S3BucketLifecycleConfigurationUpdateInput::decode(&to)?;
        let crate::send_models_generated::S3BucketLifecycleConfigurationUpdateParts { rules } =
            input.into_parts();
        let rules = rules.into_inner();
        self.put_s3_bucket_lifecycle(&id, identifier, &rules, schema)
            .await
    }

    async fn put_s3_bucket_lifecycle(
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

        let sdk_rules: Vec<LifecycleRule> = rules
            .iter()
            .map(|v| build_rule(id, schema, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = BucketLifecycleConfiguration::builder()
            .set_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build BucketLifecycleConfiguration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket)
            .lifecycle_configuration(config)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket lifecycle configuration", e)
                    .for_resource(id.clone())
            })?;

        Ok(self
            .read_s3_bucket_lifecycle_configuration(id, applied_identifier)
            .await)
    }

    pub(crate) async fn delete_s3_bucket_lifecycle_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_lifecycle().bucket(identifier)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchLifecycleConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(api_error_with_meta(
                "Failed to delete bucket lifecycle configuration",
                e,
            )
            .for_resource(id.clone())),
        }
    }
}

fn build_rule(
    id: &ResourceId,
    schema: &ResourceSchema,
    rule_value: &Value,
) -> ProviderResult<LifecycleRule> {
    let Value::Concrete(ConcreteValue::Map(map)) = rule_value else {
        return Err(
            ProviderError::invalid_input("each rule must be a map").for_resource(id.clone())
        );
    };

    let status = enum_attr_at_schema_path(schema, "rules", &["status"], map.get("status"))
        .map_err(|error| enum_attr_error(id, error))?
        .ok_or_else(|| {
            ProviderError::invalid_input("rule.status is required").for_resource(id.clone())
        })?;

    let mut builder = LifecycleRule::builder().status(ExpirationStatus::from(status.api_value()));

    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("id") {
        builder = builder.id(s.expose_plaintext());
    }
    // S3 requires every V2 lifecycle rule to carry a `Filter` element.
    // Build one from the `filter` attribute, or fall back to an empty
    // filter (matches all objects) so the request is not rejected with
    // `MalformedXML`.
    builder = builder.filter(match map.get("filter") {
        Some(v) => build_filter(id, v)?,
        None => LifecycleRuleFilter::builder().build(),
    });
    if let Some(v) = map.get("expiration") {
        builder = builder.expiration(build_expiration(id, v)?);
    }
    match map.get("transitions") {
        Some(Value::Concrete(ConcreteValue::List(items))) => {
            let transitions: Vec<Transition> = items
                .iter()
                .map(|v| build_transition(id, schema, v))
                .collect::<ProviderResult<Vec<_>>>()?;
            builder = builder.set_transitions(Some(transitions));
        }
        Some(_) => {
            return Err(
                ProviderError::invalid_input("transitions must be a list").for_resource(id.clone())
            );
        }
        None => {}
    }
    if let Some(v) = map.get("noncurrent_version_expiration") {
        builder = builder.noncurrent_version_expiration(build_ncv_expiration(id, v)?);
    }
    match map.get("noncurrent_version_transitions") {
        Some(Value::Concrete(ConcreteValue::List(items))) => {
            let transitions: Vec<NoncurrentVersionTransition> = items
                .iter()
                .map(|v| build_ncv_transition(id, schema, v))
                .collect::<ProviderResult<Vec<_>>>()?;
            builder = builder.set_noncurrent_version_transitions(Some(transitions));
        }
        Some(_) => {
            return Err(ProviderError::invalid_input(
                "noncurrent_version_transitions must be a list",
            )
            .for_resource(id.clone()));
        }
        None => {}
    }
    if let Some(v) = map.get("abort_incomplete_multipart_upload") {
        builder = builder.abort_incomplete_multipart_upload(build_abort_multipart(id, v)?);
    }

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build LifecycleRule", &e))
            .for_resource(id.clone())
    })
}

/// Build a single S3 object tag from a `{ key, value }` map.
fn build_filter_tag(id: &ResourceId, value: &Value) -> ProviderResult<Tag> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("filter tag must be a map").for_resource(id.clone())
        );
    };
    let key = match map.get("key") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext(),
        _ => {
            return Err(
                ProviderError::invalid_input("filter tag.key is required").for_resource(id.clone())
            );
        }
    };
    let val = match map.get("value") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext(),
        _ => {
            return Err(ProviderError::invalid_input("filter tag.value is required")
                .for_resource(id.clone()));
        }
    };
    Tag::builder().key(key).value(val).build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build filter tag", &e))
            .for_resource(id.clone())
    })
}

/// Build a `LifecycleRuleAndOperator` (prefix + tags + size bounds).
fn build_filter_and(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleRuleAndOperator> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("filter.and must be a map").for_resource(id.clone())
        );
    };
    let mut builder = LifecycleRuleAndOperator::builder();
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s.expose_plaintext());
    }
    match map.get("tags") {
        Some(Value::Concrete(ConcreteValue::List(items))) => {
            let tags: Vec<Tag> = items
                .iter()
                .map(|v| build_filter_tag(id, v))
                .collect::<ProviderResult<Vec<_>>>()?;
            builder = builder.set_tags(Some(tags));
        }
        Some(_) => {
            return Err(
                ProviderError::invalid_input("filter.and.tags must be a list")
                    .for_resource(id.clone()),
            );
        }
        None => {}
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_greater_than") {
        builder = builder.object_size_greater_than(*n);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_less_than") {
        builder = builder.object_size_less_than(*n);
    }
    Ok(builder.build())
}

/// Build a `LifecycleRuleFilter` from the DSL `filter` map. S3 allows at
/// most one of `prefix` / `tag` / size bound / `and` to be set; the
/// schema validates the shape, so this just forwards whatever is present.
fn build_filter(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleRuleFilter> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input("filter must be a map").for_resource(id.clone()));
    };
    let mut builder = LifecycleRuleFilter::builder();
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s.expose_plaintext());
    }
    if let Some(v) = map.get("tag") {
        builder = builder.tag(build_filter_tag(id, v)?);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_greater_than") {
        builder = builder.object_size_greater_than(*n);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_less_than") {
        builder = builder.object_size_less_than(*n);
    }
    if let Some(v) = map.get("and") {
        builder = builder.and(build_filter_and(id, v)?);
    }
    Ok(builder.build())
}

fn build_expiration(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleExpiration> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("expiration must be a map").for_resource(id.clone())
        );
    };
    let mut builder = LifecycleExpiration::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days") {
        builder = builder.days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::String(date))) = map.get("date") {
        builder = builder.date(parse_lifecycle_date(
            id,
            "expiration.date",
            date.expose_plaintext(),
        )?);
    }
    if let Some(Value::Concrete(ConcreteValue::Bool(b))) = map.get("expired_object_delete_marker") {
        builder = builder.expired_object_delete_marker(*b);
    }
    Ok(builder.build())
}

fn build_transition(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<Transition> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("transition must be a map").for_resource(id.clone())
        );
    };
    let storage_class = enum_attr_at_schema_path(
        schema,
        "rules",
        &["transitions", "storage_class"],
        map.get("storage_class"),
    )
    .map_err(|error| enum_attr_error(id, error))?
    .ok_or_else(|| {
        ProviderError::invalid_input("transition.storage_class is required")
            .for_resource(id.clone())
    })?;
    let mut builder = Transition::builder()
        .storage_class(TransitionStorageClass::from(storage_class.api_value()));
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days") {
        builder = builder.days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::String(date))) = map.get("date") {
        builder = builder.date(parse_lifecycle_date(
            id,
            "transition.date",
            date.expose_plaintext(),
        )?);
    }
    Ok(builder.build())
}

fn parse_lifecycle_date(id: &ResourceId, field: &str, value: &str) -> ProviderResult<DateTime> {
    DateTime::from_str(value, Format::DateTime).map_err(|error| {
        ProviderError::invalid_input(format!(
            "{field} must be a UTC RFC 3339 timestamp (for example 2026-01-01T00:00:00Z): {error}"
        ))
        .for_resource(id.clone())
    })
}

fn lifecycle_date_value(id: &ResourceId, field: &str, value: &DateTime) -> ProviderResult<Value> {
    value
        .fmt(Format::DateTime)
        .map(|value| Value::Concrete(ConcreteValue::String(value.into())))
        .map_err(|error| {
            ProviderError::api_error(format!("failed to format {field} returned by S3: {error}"))
                .for_resource(id.clone())
        })
}

fn build_ncv_expiration(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<NoncurrentVersionExpiration> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("noncurrent_version_expiration must be a map")
                .for_resource(id.clone()),
        );
    };
    let mut builder = NoncurrentVersionExpiration::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("noncurrent_days") {
        builder = builder.noncurrent_days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("newer_noncurrent_versions") {
        builder = builder.newer_noncurrent_versions(*n as i32);
    }
    Ok(builder.build())
}

fn build_ncv_transition(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<NoncurrentVersionTransition> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("noncurrent_version_transition must be a map")
                .for_resource(id.clone()),
        );
    };
    let storage_class = enum_attr_at_schema_path(
        schema,
        "rules",
        &["noncurrent_version_transitions", "storage_class"],
        map.get("storage_class"),
    )
    .map_err(|error| enum_attr_error(id, error))?
    .ok_or_else(|| {
        ProviderError::invalid_input("noncurrent_version_transition.storage_class is required")
            .for_resource(id.clone())
    })?;
    let mut builder = NoncurrentVersionTransition::builder()
        .storage_class(TransitionStorageClass::from(storage_class.api_value()));
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("noncurrent_days") {
        builder = builder.noncurrent_days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("newer_noncurrent_versions") {
        builder = builder.newer_noncurrent_versions(*n as i32);
    }
    Ok(builder.build())
}

fn build_abort_multipart(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<AbortIncompleteMultipartUpload> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input(
            "abort_incomplete_multipart_upload must be a map",
        )
        .for_resource(id.clone()));
    };
    let mut builder = AbortIncompleteMultipartUpload::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days_after_initiation") {
        builder = builder.days_after_initiation(*d as i32);
    }
    Ok(builder.build())
}

fn rule_to_value(id: &ResourceId, rule: &LifecycleRule) -> ProviderResult<Value> {
    let mut map = IndexMap::new();
    if let Some(s) = rule.id() {
        map.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string().into())),
        );
    }
    map.insert(
        "status".to_string(),
        Value::Concrete(ConcreteValue::String(
            rule.status().as_str().to_string().into(),
        )),
    );
    if let Some(filter) = rule.filter() {
        map.insert("filter".to_string(), filter_to_value(filter));
    }
    if let Some(exp) = rule.expiration() {
        let mut e = IndexMap::new();
        if let Some(d) = exp.days() {
            e.insert(
                "days".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        if let Some(date) = exp.date() {
            e.insert(
                "date".to_string(),
                lifecycle_date_value(id, "expiration.date", date)?,
            );
        }
        if let Some(b) = exp.expired_object_delete_marker() {
            e.insert(
                "expired_object_delete_marker".to_string(),
                Value::Concrete(ConcreteValue::Bool(b)),
            );
        }
        map.insert(
            "expiration".to_string(),
            Value::Concrete(ConcreteValue::Map(e)),
        );
    }
    let transitions = rule
        .transitions()
        .iter()
        .map(|transition| transition_to_value(id, transition))
        .collect::<ProviderResult<Vec<_>>>()?;
    map.insert(
        "transitions".to_string(),
        Value::Concrete(ConcreteValue::List(transitions)),
    );
    if let Some(ncv) = rule.noncurrent_version_expiration() {
        let mut e = IndexMap::new();
        if let Some(d) = ncv.noncurrent_days() {
            e.insert(
                "noncurrent_days".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        if let Some(n) = ncv.newer_noncurrent_versions() {
            e.insert(
                "newer_noncurrent_versions".to_string(),
                Value::Concrete(ConcreteValue::Int(n as i64)),
            );
        }
        map.insert(
            "noncurrent_version_expiration".to_string(),
            Value::Concrete(ConcreteValue::Map(e)),
        );
    }
    let ncv_transitions: Vec<Value> = rule
        .noncurrent_version_transitions()
        .iter()
        .map(ncv_transition_to_value)
        .collect();
    map.insert(
        "noncurrent_version_transitions".to_string(),
        Value::Concrete(ConcreteValue::List(ncv_transitions)),
    );
    if let Some(abort) = rule.abort_incomplete_multipart_upload() {
        let mut a = IndexMap::new();
        if let Some(d) = abort.days_after_initiation() {
            a.insert(
                "days_after_initiation".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        map.insert(
            "abort_incomplete_multipart_upload".to_string(),
            Value::Concrete(ConcreteValue::Map(a)),
        );
    }
    Ok(Value::Concrete(ConcreteValue::Map(map)))
}

/// Convert an S3 `Tag` into a `{ key, value }` map value.
fn tag_to_value(tag: &Tag) -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "key".to_string(),
        Value::Concrete(ConcreteValue::String(tag.key().to_string().into())),
    );
    m.insert(
        "value".to_string(),
        Value::Concrete(ConcreteValue::String(tag.value().to_string().into())),
    );
    Value::Concrete(ConcreteValue::Map(m))
}

fn filter_and_to_value(and: &LifecycleRuleAndOperator) -> Value {
    let mut m = IndexMap::new();
    if let Some(p) = and.prefix() {
        m.insert(
            "prefix".to_string(),
            Value::Concrete(ConcreteValue::String(p.to_string().into())),
        );
    }
    let tags = and.tags();
    m.insert(
        "tags".to_string(),
        Value::Concrete(ConcreteValue::List(tags.iter().map(tag_to_value).collect())),
    );
    if let Some(n) = and.object_size_greater_than() {
        m.insert(
            "object_size_greater_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(n) = and.object_size_less_than() {
        m.insert(
            "object_size_less_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

fn filter_to_value(filter: &LifecycleRuleFilter) -> Value {
    let mut m = IndexMap::new();
    if let Some(p) = filter.prefix() {
        m.insert(
            "prefix".to_string(),
            Value::Concrete(ConcreteValue::String(p.to_string().into())),
        );
    }
    if let Some(tag) = filter.tag() {
        m.insert("tag".to_string(), tag_to_value(tag));
    }
    if let Some(n) = filter.object_size_greater_than() {
        m.insert(
            "object_size_greater_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(n) = filter.object_size_less_than() {
        m.insert(
            "object_size_less_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(and) = filter.and() {
        m.insert("and".to_string(), filter_and_to_value(and));
    }
    Value::Concrete(ConcreteValue::Map(m))
}

fn transition_to_value(id: &ResourceId, t: &Transition) -> ProviderResult<Value> {
    let mut m = IndexMap::new();
    if let Some(d) = t.days() {
        m.insert(
            "days".to_string(),
            Value::Concrete(ConcreteValue::Int(d as i64)),
        );
    }
    if let Some(date) = t.date() {
        m.insert(
            "date".to_string(),
            lifecycle_date_value(id, "transition.date", date)?,
        );
    }
    if let Some(sc) = t.storage_class() {
        m.insert(
            "storage_class".to_string(),
            Value::Concrete(ConcreteValue::String(sc.as_str().to_string().into())),
        );
    }
    Ok(Value::Concrete(ConcreteValue::Map(m)))
}

fn ncv_transition_to_value(t: &NoncurrentVersionTransition) -> Value {
    let mut m = IndexMap::new();
    if let Some(d) = t.noncurrent_days() {
        m.insert(
            "noncurrent_days".to_string(),
            Value::Concrete(ConcreteValue::Int(d as i64)),
        );
    }
    if let Some(n) = t.newer_noncurrent_versions() {
        m.insert(
            "newer_noncurrent_versions".to_string(),
            Value::Concrete(ConcreteValue::Int(n as i64)),
        );
    }
    if let Some(sc) = t.storage_class() {
        m.insert(
            "storage_class".to_string(),
            Value::Concrete(ConcreteValue::String(sc.as_str().to_string().into())),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rid() -> ResourceId {
        ResourceId::with_identity("s3.bucket_lifecycle_configuration", "test")
    }

    fn schema() -> ResourceSchema {
        crate::schemas::generated::s3::bucket_lifecycle_configuration::s3_bucket_lifecycle_configuration_config().schema
    }

    fn str_val(s: &str) -> Value {
        Value::Concrete(ConcreteValue::String(s.to_string().into()))
    }

    fn map_val(pairs: Vec<(&str, Value)>) -> Value {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        Value::Concrete(ConcreteValue::Map(m))
    }

    #[test]
    fn invalid_rule_status_is_rejected() {
        let rule_value = map_val(vec![("status", str_val("enabled"))]);

        build_rule(&rid(), &schema(), &rule_value)
            .expect_err("a non-exact closed-enum leaf must be rejected");
    }

    #[test]
    fn non_list_transitions_are_rejected() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("transitions", Value::Concrete(ConcreteValue::Bool(true))),
        ]);

        build_rule(&rid(), &schema(), &rule_value).expect_err("present transitions must be a list");
    }

    #[test]
    fn non_list_noncurrent_version_transitions_are_rejected() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "noncurrent_version_transitions",
                Value::Concrete(ConcreteValue::Bool(true)),
            ),
        ]);

        build_rule(&rid(), &schema(), &rule_value)
            .expect_err("present noncurrent_version_transitions must be a list");
    }

    #[test]
    fn rule_without_filter_gets_empty_filter() {
        // Reproduces carina-rs/carina-provider-aws#273: a rule with no
        // `filter` must still emit a `Filter` element, otherwise S3
        // rejects the request with `MalformedXML`.
        let rule_value = map_val(vec![
            ("id", str_val("expire-all")),
            ("status", str_val("Enabled")),
            (
                "expiration",
                map_val(vec![("days", Value::Concrete(ConcreteValue::Int(365)))]),
            ),
        ]);
        let rule = build_rule(&rid(), &schema(), &rule_value).expect("build_rule should succeed");
        assert!(
            rule.filter().is_some(),
            "a lifecycle rule must always carry a Filter element"
        );
    }

    #[test]
    fn rule_with_prefix_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("filter", map_val(vec![("prefix", str_val("logs/"))])),
        ]);
        let rule = build_rule(&rid(), &schema(), &rule_value).expect("build_rule should succeed");
        let filter = rule.filter().expect("filter set");
        assert_eq!(filter.prefix(), Some("logs/"));
    }

    #[test]
    fn rule_with_tag_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "filter",
                map_val(vec![(
                    "tag",
                    map_val(vec![("key", str_val("env")), ("value", str_val("dev"))]),
                )]),
            ),
        ]);
        let rule = build_rule(&rid(), &schema(), &rule_value).expect("build_rule should succeed");
        let tag = rule.filter().and_then(|f| f.tag()).expect("tag set");
        assert_eq!(tag.key(), "env");
        assert_eq!(tag.value(), "dev");
    }

    #[test]
    fn rule_with_and_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "filter",
                map_val(vec![(
                    "and",
                    map_val(vec![
                        ("prefix", str_val("data/")),
                        (
                            "object_size_greater_than",
                            Value::Concrete(ConcreteValue::Int(1024)),
                        ),
                        (
                            "tags",
                            Value::Concrete(ConcreteValue::List(vec![map_val(vec![
                                ("key", str_val("tier")),
                                ("value", str_val("cold")),
                            ])])),
                        ),
                    ]),
                )]),
            ),
        ]);
        let rule = build_rule(&rid(), &schema(), &rule_value).expect("build_rule should succeed");
        let and = rule.filter().and_then(|f| f.and()).expect("and set");
        assert_eq!(and.prefix(), Some("data/"));
        assert_eq!(and.object_size_greater_than(), Some(1024));
        assert_eq!(and.tags().len(), 1);
    }

    #[test]
    fn non_list_filter_and_tags_are_rejected() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "filter",
                map_val(vec![(
                    "and",
                    map_val(vec![("tags", Value::Concrete(ConcreteValue::Bool(true)))]),
                )]),
            ),
        ]);

        let error = build_rule(&rid(), &schema(), &rule_value)
            .expect_err("present filter.and.tags must be a list");
        assert!(error.to_string().contains("filter.and.tags must be a list"));
    }

    #[test]
    fn empty_filter_is_preserved_on_read() {
        let empty = LifecycleRuleFilter::builder().build();
        assert_eq!(
            filter_to_value(&empty),
            Value::Concrete(ConcreteValue::Map(IndexMap::new()))
        );
    }

    #[test]
    fn prefix_filter_round_trips() {
        let built = build_rule(
            &rid(),
            &schema(),
            &map_val(vec![
                ("status", str_val("Enabled")),
                ("filter", map_val(vec![("prefix", str_val("logs/"))])),
            ]),
        )
        .unwrap();
        let value = rule_to_value(&rid(), &built).unwrap();
        let Value::Concrete(ConcreteValue::Map(m)) = value else {
            panic!("expected map");
        };
        let Some(Value::Concrete(ConcreteValue::Map(filter))) = m.get("filter") else {
            panic!("filter should round-trip");
        };
        assert_eq!(filter.get("prefix"), Some(&str_val("logs/")));
    }

    #[test]
    fn empty_fields_and_dates_round_trip() {
        let date = "2026-01-01T00:00:00Z";
        let built = build_rule(
            &rid(),
            &schema(),
            &map_val(vec![
                ("id", str_val("")),
                ("status", str_val("Enabled")),
                ("filter", map_val(vec![("prefix", str_val(""))])),
                ("expiration", map_val(vec![("date", str_val(date))])),
                (
                    "transitions",
                    Value::Concrete(ConcreteValue::List(vec![map_val(vec![
                        ("date", str_val(date)),
                        ("storage_class", str_val("GLACIER")),
                    ])])),
                ),
            ]),
        )
        .unwrap();
        let Value::Concrete(ConcreteValue::Map(read)) = rule_to_value(&rid(), &built).unwrap()
        else {
            panic!("expected rule map");
        };
        assert_eq!(read.get("id"), Some(&str_val("")));
        let Some(Value::Concrete(ConcreteValue::Map(filter))) = read.get("filter") else {
            panic!("expected filter map");
        };
        assert_eq!(filter.get("prefix"), Some(&str_val("")));
        let Some(Value::Concrete(ConcreteValue::Map(expiration))) = read.get("expiration") else {
            panic!("expected expiration map");
        };
        assert_eq!(expiration.get("date"), Some(&str_val(date)));
        let Some(Value::Concrete(ConcreteValue::List(transitions))) = read.get("transitions")
        else {
            panic!("expected transitions list");
        };
        let Value::Concrete(ConcreteValue::Map(transition)) = &transitions[0] else {
            panic!("expected transition map");
        };
        assert_eq!(transition.get("date"), Some(&str_val(date)));
    }

    #[test]
    fn present_empty_expiration_is_preserved() {
        let rule = LifecycleRule::builder()
            .status(ExpirationStatus::Enabled)
            .filter(LifecycleRuleFilter::builder().build())
            .expiration(LifecycleExpiration::builder().build())
            .build()
            .unwrap();
        let Value::Concrete(ConcreteValue::Map(read)) = rule_to_value(&rid(), &rule).unwrap()
        else {
            panic!("expected rule map");
        };
        assert_eq!(
            read.get("expiration"),
            Some(&Value::Concrete(ConcreteValue::Map(IndexMap::new())))
        );
    }
}
