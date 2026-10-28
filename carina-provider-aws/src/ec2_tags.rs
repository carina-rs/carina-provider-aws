//! EC2 tag helper functions

use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::ProviderResult;
use carina_core::resource::{ConcreteValue, ResourceId, Value};

use crate::AwsProvider;
use crate::aws_operation::EC2_CREATE_TOKEN_TAG;
use crate::error_helpers::api_error_with_meta;

/// Convert user-authored tag values to SDK tags.
///
/// This is the single write-side filter for the provider-owned discovery tag.
/// User-supplied `carina:create-token` entries are silently ignored on both
/// create and update; callers that need a `TagSpecification` must build it
/// from this result so the policy cannot vary by call site.
pub(crate) fn value_to_ec2_tags(value: &Value) -> Vec<aws_sdk_ec2::types::Tag> {
    let mut tags = Vec::new();
    if let Value::Concrete(ConcreteValue::Map(map)) = value {
        for (key, val) in map {
            if key != EC2_CREATE_TOKEN_TAG
                && let Value::Concrete(ConcreteValue::String(v)) = val
            {
                tags.push(
                    aws_sdk_ec2::types::Tag::builder()
                        .key(key)
                        .value(v.expose_plaintext())
                        .build(),
                );
            }
        }
    }
    tags
}

impl AwsProvider {
    /// Extract tags from EC2 tag list into a Value::Map.
    ///
    /// `carina:create-token` is provider-owned discovery metadata. It is
    /// intentionally absent from state, so it cannot create drift.
    pub(crate) fn ec2_tags_to_value(tags: &[aws_sdk_ec2::types::Tag]) -> Option<Value> {
        let mut tag_map: IndexMap<String, Value> = IndexMap::new();
        for tag in tags {
            if let (Some(key), Some(value)) = (tag.key(), tag.value()) {
                if key == EC2_CREATE_TOKEN_TAG {
                    continue;
                }
                tag_map.insert(
                    key.to_string(),
                    Value::Concrete(ConcreteValue::String(value.to_string().into())),
                );
            }
        }
        if tag_map.is_empty() {
            None
        } else {
            Some(Value::Concrete(ConcreteValue::Map(tag_map)))
        }
    }

    /// Apply tags to an EC2 resource
    ///
    /// When `from_attributes` is provided, tags that exist in `from` but not in `to`
    /// will be deleted from the resource.
    pub(crate) async fn apply_ec2_tags(
        &self,
        resource_id: &ResourceId,
        ec2_resource_id: &str,
        attributes: &HashMap<String, Value>,
        from_attributes: Option<&HashMap<String, Value>>,
    ) -> ProviderResult<()> {
        // Delete tags that were removed (present in from but not in to)
        if let Some(from_attrs) = from_attributes {
            let old_keys: std::collections::HashSet<&String> = if let Some(Value::Concrete(
                ConcreteValue::Map(old_map),
            )) = from_attrs.get("tags")
            {
                old_map.keys().collect()
            } else {
                std::collections::HashSet::new()
            };
            let new_keys: std::collections::HashSet<&String> = if let Some(Value::Concrete(
                ConcreteValue::Map(new_map),
            )) = attributes.get("tags")
            {
                new_map.keys().collect()
            } else {
                std::collections::HashSet::new()
            };
            let removed_keys: Vec<&String> = old_keys
                .difference(&new_keys)
                .copied()
                .filter(|key| key.as_str() != EC2_CREATE_TOKEN_TAG)
                .collect();
            if !removed_keys.is_empty() {
                let mut req = self.ec2_client.delete_tags().resources(ec2_resource_id);
                for key in removed_keys {
                    req = req.tags(aws_sdk_ec2::types::Tag::builder().key(key.as_str()).build());
                }
                req.send().await.map_err(|e| {
                    api_error_with_meta("Failed to delete tags", "ec2.DeleteTags", e)
                        .for_resource(resource_id.clone())
                })?;
            }
        }

        // Add/update tags
        if let Some(tag_value) = attributes.get("tags") {
            let tags = value_to_ec2_tags(tag_value);
            if !tags.is_empty() {
                let mut req = self.ec2_client.create_tags().resources(ec2_resource_id);
                for tag in tags {
                    req = req.tags(tag);
                }
                req.send().await.map_err(|e| {
                    api_error_with_meta("Failed to tag resource", "ec2.CreateTags", e)
                        .for_resource(resource_id.clone())
                })?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use carina_core::differ::{Diff, diff};
    use carina_core::provider::RawSavedAttrs;
    use carina_core::resource::{ResolvedResource, Resource, State};
    use carina_core::schema::SchemaRegistry;
    use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

    #[derive(Default)]
    struct TagRecordingEc2Service {
        request_bodies: Mutex<Vec<String>>,
    }

    impl MockService for TagRecordingEc2Service {
        fn service_name(&self) -> &str {
            "ec2"
        }

        fn url_patterns(&self) -> Vec<&str> {
            vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
        }

        fn handle(
            &self,
            request: MockRequest,
        ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
            let body =
                String::from_utf8(request.body.to_vec()).expect("EC2 Query body must be UTF-8");
            self.request_bodies
                .lock()
                .expect("request-body lock")
                .push(body.clone());
            let action = body
                .split('&')
                .find_map(|pair| pair.strip_prefix("Action="))
                .expect("EC2 Query action");
            let response = match action {
                "DeleteTags" => MockResponse::xml(
                    200,
                    r#"<DeleteTagsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"/>"#,
                ),
                "CreateTags" => MockResponse::xml(
                    200,
                    r#"<CreateTagsResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/"/>"#,
                ),
                other => panic!("unexpected EC2 action {other}: {body}"),
            };
            Box::pin(async move { response })
        }
    }

    // --- ec2_tags_to_value tests ---

    #[test]
    fn test_ec2_tags_to_value_empty() {
        let tags: Vec<aws_sdk_ec2::types::Tag> = vec![];
        assert_eq!(AwsProvider::ec2_tags_to_value(&tags), None);
    }

    #[test]
    fn test_ec2_tags_to_value_single_tag() {
        let tags = vec![
            aws_sdk_ec2::types::Tag::builder()
                .key("Name")
                .value("my-resource")
                .build(),
        ];
        let result = AwsProvider::ec2_tags_to_value(&tags);
        assert!(result.is_some());
        if let Some(Value::Concrete(ConcreteValue::Map(map))) = result {
            assert_eq!(
                map.get("Name"),
                Some(&Value::Concrete(ConcreteValue::String(
                    "my-resource".to_string().into()
                )))
            );
        } else {
            panic!("Expected Value::Map");
        }
    }

    #[test]
    fn test_ec2_tags_to_value_multiple_tags() {
        let tags = vec![
            aws_sdk_ec2::types::Tag::builder()
                .key("Name")
                .value("test")
                .build(),
            aws_sdk_ec2::types::Tag::builder()
                .key("Environment")
                .value("production")
                .build(),
        ];
        let result = AwsProvider::ec2_tags_to_value(&tags);
        if let Some(Value::Concrete(ConcreteValue::Map(map))) = result {
            assert_eq!(map.len(), 2);
            assert_eq!(
                map.get("Name"),
                Some(&Value::Concrete(ConcreteValue::String(
                    "test".to_string().into()
                )))
            );
            assert_eq!(
                map.get("Environment"),
                Some(&Value::Concrete(ConcreteValue::String(
                    "production".to_string().into()
                )))
            );
        } else {
            panic!("Expected Value::Map with 2 entries");
        }
    }

    #[test]
    fn create_token_tag_is_hidden_from_read_state() {
        let tags = vec![
            aws_sdk_ec2::types::Tag::builder()
                .key("carina:create-token")
                .value("discovery-token")
                .build(),
            aws_sdk_ec2::types::Tag::builder()
                .key("Name")
                .value("visible")
                .build(),
        ];

        let Some(Value::Concrete(ConcreteValue::Map(map))) = AwsProvider::ec2_tags_to_value(&tags)
        else {
            panic!("expected visible tags");
        };
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("Name"));
        assert!(!map.contains_key("carina:create-token"));
    }

    #[test]
    fn test_ec2_tags_to_value_missing_key_or_value() {
        // Tag with no key set
        let tags = vec![aws_sdk_ec2::types::Tag::builder().build()];
        assert_eq!(AwsProvider::ec2_tags_to_value(&tags), None);
    }

    // --- value_to_ec2_tags tests ---

    #[test]
    fn test_value_to_ec2_tags_from_map() {
        let value = Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "Name".to_string(),
                Value::Concrete(ConcreteValue::String("test".to_string().into())),
            ),
            (
                "Env".to_string(),
                Value::Concrete(ConcreteValue::String("prod".to_string().into())),
            ),
        ])));
        let tags = value_to_ec2_tags(&value);
        assert_eq!(tags.len(), 2);
        // Check both tags exist (order not guaranteed from HashMap)
        let tag_map: HashMap<String, String> = tags
            .iter()
            .map(|t| {
                (
                    t.key().unwrap_or("").to_string(),
                    t.value().unwrap_or("").to_string(),
                )
            })
            .collect();
        assert_eq!(tag_map.get("Name"), Some(&"test".to_string()));
        assert_eq!(tag_map.get("Env"), Some(&"prod".to_string()));
    }

    #[test]
    fn test_value_to_ec2_tags_non_map_value() {
        let value = Value::Concrete(ConcreteValue::String("not a map".to_string().into()));
        let tags = value_to_ec2_tags(&value);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_value_to_ec2_tags_non_string_values_skipped() {
        let value = Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "Name".to_string(),
                Value::Concrete(ConcreteValue::String("test".to_string().into())),
            ),
            ("Count".to_string(), Value::Concrete(ConcreteValue::Int(42))),
        ])));
        let tags = value_to_ec2_tags(&value);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), Some("Name"));
        assert_eq!(tags[0].value(), Some("test"));
    }

    #[test]
    fn test_value_to_ec2_tags_empty_map() {
        let value = Value::Concrete(ConcreteValue::Map(IndexMap::new()));
        let tags = value_to_ec2_tags(&value);
        assert!(tags.is_empty());
    }

    #[test]
    fn create_token_tag_cannot_be_written_as_a_user_tag() {
        let value = Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "carina:create-token".to_string(),
                Value::Concrete(ConcreteValue::String("replacement".to_string().into())),
            ),
            (
                "Name".to_string(),
                Value::Concrete(ConcreteValue::String("visible".to_string().into())),
            ),
        ])));

        let tags = value_to_ec2_tags(&value);

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), Some("Name"));
    }

    #[tokio::test]
    async fn updating_user_tags_never_deletes_or_overwrites_create_token() {
        let service = Arc::new(TagRecordingEc2Service::default());
        let mock = MockAws::builder()
            .with_service(Arc::clone(&service))
            .build();
        let config = crate::BoundedSdkConfig::new(mock.sdk_config("us-east-1").await);
        let provider =
            AwsProvider::from_sdk_config("us-east-1".to_string(), Vec::new(), Vec::new(), &config);
        let id = ResourceId::with_provider_identity(
            "aws",
            "ec2.Vpc",
            "example",
            Some("example".to_string()),
        );
        let old_tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                EC2_CREATE_TOKEN_TAG.to_string(),
                Value::Concrete(ConcreteValue::String("original-token".into())),
            ),
            (
                "Old".to_string(),
                Value::Concrete(ConcreteValue::String("remove-me".into())),
            ),
        ])));
        let new_tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                EC2_CREATE_TOKEN_TAG.to_string(),
                Value::Concrete(ConcreteValue::String("replacement-token".into())),
            ),
            (
                "Name".to_string(),
                Value::Concrete(ConcreteValue::String("updated".into())),
            ),
        ])));

        provider
            .apply_ec2_tags(
                &id,
                "vpc-12345678",
                &HashMap::from([("tags".to_string(), new_tags)]),
                Some(&HashMap::from([("tags".to_string(), old_tags)])),
            )
            .await
            .expect("tag update");

        let bodies = service.request_bodies.lock().expect("request-body lock");
        assert_eq!(bodies.len(), 2, "expected DeleteTags and CreateTags");
        assert!(bodies.iter().any(|body| body.contains("Old")));
        assert!(bodies.iter().any(|body| body.contains("Name")));
        for body in bodies.iter() {
            assert!(!body.contains("carina:create-token"), "{body}");
            assert!(!body.contains("carina%3Acreate-token"), "{body}");
            assert!(!body.contains("original-token"), "{body}");
            assert!(!body.contains("replacement-token"), "{body}");
        }
    }

    #[test]
    fn read_state_without_create_token_has_no_plan_diff() {
        let desired_tags = Value::Concrete(ConcreteValue::Map(IndexMap::from([(
            "Name".to_string(),
            Value::Concrete(ConcreteValue::String("visible".into())),
        )])));
        let desired = Resource::with_provider("aws", "ec2.Vpc", "example", None)
            .with_attribute("tags", desired_tags);
        let aws_tags = vec![
            aws_sdk_ec2::types::Tag::builder()
                .key(EC2_CREATE_TOKEN_TAG)
                .value("discovery-token")
                .build(),
            aws_sdk_ec2::types::Tag::builder()
                .key("Name")
                .value("visible")
                .build(),
        ];
        let read_tags = AwsProvider::ec2_tags_to_value(&aws_tags).expect("visible user tag");
        let current = State::existing(
            desired.id.clone(),
            HashMap::from([("tags".to_string(), read_tags)]),
        )
        .with_identifier("vpc-12345678");
        let saved = RawSavedAttrs::default().lift(&SchemaRegistry::new());
        let schema = crate::schemas::generated::ec2::vpc::ec2_vpc_config().schema;

        let result = diff(
            &ResolvedResource::new(desired),
            &current,
            &saved,
            None,
            Some(&schema),
        );

        assert!(matches!(result, Diff::NoChange(_)), "{result:?}");
    }
}
