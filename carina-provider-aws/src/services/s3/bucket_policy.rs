use std::collections::HashMap;

use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::iam::role::{iam_policy_json_to_value, value_to_iam_policy_json};

impl AwsProvider {
    /// Read an S3 BucketPolicy.
    ///
    /// `identifier` is the bucket name. Returns `State::not_found` when the
    /// bucket has no policy (`NoSuchBucketPolicy`); other errors propagate.
    pub(crate) async fn read_s3_bucket_policy(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_policy()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));

                if let Some(policy_json) = output.policy() {
                    let policy_value = iam_policy_json_to_value(policy_json).map_err(|e| {
                        ProviderError::new(format!("Failed to parse bucket policy: {}", e))
                            .for_resource(id.clone())
                    })?;
                    attributes.insert("policy".to_string(), policy_value);
                }

                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if let Some(service_err) = e.as_service_error()
                    && service_err.code() == Some("NoSuchBucketPolicy")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::new(sdk_error_message("Failed to get bucket policy", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    /// Create an S3 BucketPolicy via PutBucketPolicy.
    pub(crate) async fn create_s3_bucket_policy(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        let policy_json = resolve_policy_attr(&resource)?;

        self.s3_client
            .put_bucket_policy()
            .bucket(&bucket)
            .policy(&policy_json)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to put bucket policy", &e))
                    .for_resource(resource.id.clone())
            })?;

        self.read_s3_bucket_policy(&resource.id, Some(&bucket))
            .await
    }

    /// Update an S3 BucketPolicy. PutBucketPolicy replaces the existing policy.
    pub(crate) async fn update_s3_bucket_policy(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        let policy_json = resolve_policy_attr(&to)?;

        self.s3_client
            .put_bucket_policy()
            .bucket(identifier)
            .policy(&policy_json)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to update bucket policy", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_policy(&id, Some(identifier)).await
    }
}

/// Resolve the `policy` attribute on a BucketPolicy resource into a JSON
/// string suitable for the AWS API. Accepts either a pre-serialized JSON
/// string or a Carina `Value::Map` (block-syntax DSL form).
fn resolve_policy_attr(resource: &Resource) -> ProviderResult<String> {
    match resource.get_attr("policy") {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(value @ Value::Map(_)) => value_to_iam_policy_json(value).map_err(|e| {
            ProviderError::new(format!("Failed to convert policy: {}", e))
                .for_resource(resource.id.clone())
        }),
        _ => Err(
            ProviderError::new("policy is required (must be a JSON string or block)")
                .for_resource(resource.id.clone()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_resource(policy: Option<Value>) -> Resource {
        let mut r = Resource::new("s3.BucketPolicy", "test");
        r.set_attr("bucket", Value::String("my-bucket".to_string()));
        if let Some(v) = policy {
            r.set_attr("policy", v);
        }
        r
    }

    #[test]
    fn resolve_policy_attr_accepts_json_string_passthrough() {
        let json = r#"{"Version":"2012-10-17","Statement":[]}"#;
        let r = make_resource(Some(Value::String(json.to_string())));
        let resolved = resolve_policy_attr(&r).expect("string passthrough");
        assert_eq!(resolved, json);
    }

    #[test]
    fn resolve_policy_attr_serializes_map_to_pascal_case_json() {
        let mut policy = IndexMap::new();
        policy.insert(
            "version".to_string(),
            Value::String("2012-10-17".to_string()),
        );
        let mut stmt = IndexMap::new();
        stmt.insert("effect".to_string(), Value::String("Allow".to_string()));
        stmt.insert(
            "action".to_string(),
            Value::String("s3:GetObject".to_string()),
        );
        policy.insert("statement".to_string(), Value::List(vec![Value::Map(stmt)]));

        let r = make_resource(Some(Value::Map(policy)));
        let resolved = resolve_policy_attr(&r).expect("map → JSON");

        assert!(resolved.contains("\"Version\""));
        assert!(resolved.contains("\"Statement\""));
        assert!(resolved.contains("\"Effect\""));
        assert!(resolved.contains("\"Action\""));
        assert!(!resolved.contains("\"version\""));
    }

    #[test]
    fn resolve_policy_attr_errors_when_missing() {
        let r = make_resource(None);
        let err = resolve_policy_attr(&r).expect_err("missing policy");
        assert!(format!("{}", err).contains("policy is required"));
    }

    #[test]
    fn resolve_policy_attr_errors_on_non_string_non_map() {
        let r = make_resource(Some(Value::Bool(true)));
        let err = resolve_policy_attr(&r).expect_err("invalid type");
        assert!(format!("{}", err).contains("policy is required"));
    }
}
