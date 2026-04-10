//! Data source read for `aws.identitystore.user`.
//!
//! Looks a user up by `user_name` (via `GetUserId` → `DescribeUser`) or by
//! `user_id` (via `DescribeUser` directly). One of the two must be set.
//! `identity_store_id` is always required.

use std::collections::HashMap;

use aws_sdk_identitystore::types::{AlternateIdentifier, UniqueAttribute};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, State, Value};

use crate::AwsProvider;

impl AwsProvider {
    /// Read `identitystore.user` given a `Resource` with user-supplied
    /// lookup inputs.
    pub(crate) async fn read_identitystore_user(
        &self,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let identity_store_id = resource
            .get_attr("identity_store_id")
            .and_then(value_as_str)
            .ok_or_else(|| {
                ProviderError::new("identitystore.user requires `identity_store_id`")
                    .for_resource(resource.id.clone())
            })?;

        let user_id =
            resolve_user_id(&self.identitystore_client, identity_store_id, resource).await?;

        let describe = self
            .identitystore_client
            .describe_user()
            .identity_store_id(identity_store_id)
            .user_id(&user_id)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(format!(
                    "Failed to describe identitystore user '{user_id}' in store '{identity_store_id}'"
                ))
                .with_cause(e)
                .for_resource(resource.id.clone())
            })?;

        let attributes = extract_identitystore_user(&describe);
        Ok(State::existing(resource.id.clone(), attributes))
    }
}

/// Resolve a Carina `Resource` to the Identity Store `UserId` used by the
/// subsequent `DescribeUser` call. If the user supplied `user_id` directly
/// we use it as-is; otherwise we look it up by `user_name` via `GetUserId`.
///
/// All returned errors are already tagged with `resource.id`.
async fn resolve_user_id(
    client: &aws_sdk_identitystore::Client,
    identity_store_id: &str,
    resource: &Resource,
) -> Result<String, ProviderError> {
    if let Some(user_id) = resource.get_attr("user_id").and_then(value_as_str) {
        return Ok(user_id.to_string());
    }
    let user_name = resource
        .get_attr("user_name")
        .and_then(value_as_str)
        .ok_or_else(|| {
            ProviderError::new("identitystore.user requires either `user_id` or `user_name`")
                .for_resource(resource.id.clone())
        })?;

    let unique_attribute = UniqueAttribute::builder()
        .attribute_path("userName")
        .attribute_value(aws_smithy_types::Document::String(user_name.to_string()))
        .build()
        .map_err(|e| {
            ProviderError::new("Failed to build userName lookup request")
                .with_cause(e)
                .for_resource(resource.id.clone())
        })?;

    let alt = AlternateIdentifier::UniqueAttribute(unique_attribute);

    let resp = client
        .get_user_id()
        .identity_store_id(identity_store_id)
        .alternate_identifier(alt)
        .send()
        .await
        .map_err(|e| {
            ProviderError::new(format!(
                "Failed to look up user_id for user_name '{user_name}' in store '{identity_store_id}'"
            ))
            .with_cause(e)
            .for_resource(resource.id.clone())
        })?;

    Ok(resp.user_id().to_string())
}

/// Pure projection of a `DescribeUserOutput` into the DSL-visible attribute
/// map for `identitystore.user`. Extracted from the read path so it can be
/// unit-tested without hitting the AWS SDK.
pub(crate) fn extract_identitystore_user(
    resp: &aws_sdk_identitystore::operation::describe_user::DescribeUserOutput,
) -> HashMap<String, Value> {
    let mut attributes = HashMap::new();

    attributes.insert(
        "user_id".to_string(),
        Value::String(resp.user_id().to_string()),
    );
    attributes.insert(
        "identity_store_id".to_string(),
        Value::String(resp.identity_store_id().to_string()),
    );
    if let Some(user_name) = resp.user_name() {
        attributes.insert(
            "user_name".to_string(),
            Value::String(user_name.to_string()),
        );
    }
    if let Some(display_name) = resp.display_name() {
        attributes.insert(
            "display_name".to_string(),
            Value::String(display_name.to_string()),
        );
    }
    if let Some(name) = resp.name() {
        let mut name_fields: HashMap<String, Value> = HashMap::new();
        if let Some(v) = name.formatted() {
            name_fields.insert("formatted".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = name.family_name() {
            name_fields.insert("family_name".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = name.given_name() {
            name_fields.insert("given_name".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = name.middle_name() {
            name_fields.insert("middle_name".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = name.honorific_prefix() {
            name_fields.insert("honorific_prefix".to_string(), Value::String(v.to_string()));
        }
        if let Some(v) = name.honorific_suffix() {
            name_fields.insert("honorific_suffix".to_string(), Value::String(v.to_string()));
        }
        if !name_fields.is_empty() {
            attributes.insert("name".to_string(), Value::Map(name_fields));
        }
    }
    let email_values: Vec<Value> = resp
        .emails()
        .iter()
        .filter_map(|e: &aws_sdk_identitystore::types::Email| {
            e.value().map(|v| Value::String(v.to_string()))
        })
        .collect();
    if !email_values.is_empty() {
        attributes.insert("emails".to_string(), Value::List(email_values));
    }

    attributes
}

fn value_as_str(v: &Value) -> Option<&str> {
    if let Value::String(s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_identitystore::operation::describe_user::DescribeUserOutput;
    use aws_sdk_identitystore::types::{Email, Name};

    fn name_block() -> Name {
        Name::builder()
            .formatted("Gosuke Miyashita")
            .family_name("Miyashita")
            .given_name("Gosuke")
            .build()
    }

    fn primary_email() -> Email {
        Email::builder().value("gosukenator@gmail.com").build()
    }

    fn describe_user_full() -> DescribeUserOutput {
        DescribeUserOutput::builder()
            .user_id("37846ac8-4021-705b-1bf2-26861723348f")
            .identity_store_id("d-9567916d09")
            .user_name("gosukenator@gmail.com")
            .display_name("Gosuke Miyashita")
            .name(name_block())
            .emails(primary_email())
            .build()
            .unwrap()
    }

    #[test]
    fn extract_populates_required_fields() {
        let resp = describe_user_full();
        let attrs = extract_identitystore_user(&resp);

        assert_eq!(
            attrs.get("user_id"),
            Some(&Value::String(
                "37846ac8-4021-705b-1bf2-26861723348f".to_string()
            ))
        );
        assert_eq!(
            attrs.get("identity_store_id"),
            Some(&Value::String("d-9567916d09".to_string()))
        );
        assert_eq!(
            attrs.get("user_name"),
            Some(&Value::String("gosukenator@gmail.com".to_string()))
        );
        assert_eq!(
            attrs.get("display_name"),
            Some(&Value::String("Gosuke Miyashita".to_string()))
        );
    }

    #[test]
    fn extract_populates_name_struct() {
        let resp = describe_user_full();
        let attrs = extract_identitystore_user(&resp);

        let Some(Value::Map(name)) = attrs.get("name") else {
            panic!("expected name map, got {:?}", attrs.get("name"));
        };
        assert_eq!(
            name.get("formatted"),
            Some(&Value::String("Gosuke Miyashita".to_string()))
        );
        assert_eq!(
            name.get("family_name"),
            Some(&Value::String("Miyashita".to_string()))
        );
        assert_eq!(
            name.get("given_name"),
            Some(&Value::String("Gosuke".to_string()))
        );
        // Unset optional fields must not appear in the map.
        assert!(!name.contains_key("middle_name"));
        assert!(!name.contains_key("honorific_prefix"));
    }

    #[test]
    fn extract_populates_emails_list() {
        let resp = describe_user_full();
        let attrs = extract_identitystore_user(&resp);

        let Some(Value::List(emails)) = attrs.get("emails") else {
            panic!("expected emails list, got {:?}", attrs.get("emails"));
        };
        assert_eq!(emails.len(), 1);
        assert_eq!(
            emails[0],
            Value::String("gosukenator@gmail.com".to_string())
        );
    }

    #[test]
    fn extract_omits_optional_fields_when_absent() {
        // Minimal response: only required fields (user_id, identity_store_id).
        let resp = DescribeUserOutput::builder()
            .user_id("uid-min")
            .identity_store_id("d-min")
            .build()
            .unwrap();
        let attrs = extract_identitystore_user(&resp);

        assert_eq!(
            attrs.get("user_id"),
            Some(&Value::String("uid-min".to_string()))
        );
        assert_eq!(
            attrs.get("identity_store_id"),
            Some(&Value::String("d-min".to_string()))
        );
        assert!(!attrs.contains_key("user_name"));
        assert!(!attrs.contains_key("display_name"));
        assert!(!attrs.contains_key("name"));
        assert!(!attrs.contains_key("emails"));
    }

    #[test]
    fn extract_populates_partial_name_struct() {
        // Only `given_name` is set — the `name` map must still be emitted
        // (non-empty) and must contain only the fields that were present.
        let resp = DescribeUserOutput::builder()
            .user_id("uid")
            .identity_store_id("d")
            .name(Name::builder().given_name("Gosuke").build())
            .build()
            .unwrap();
        let attrs = extract_identitystore_user(&resp);

        let Some(Value::Map(name)) = attrs.get("name") else {
            panic!("expected name map, got {:?}", attrs.get("name"));
        };
        assert_eq!(name.len(), 1);
        assert_eq!(
            name.get("given_name"),
            Some(&Value::String("Gosuke".to_string()))
        );
        assert!(!name.contains_key("formatted"));
        assert!(!name.contains_key("family_name"));
    }

    #[test]
    fn extract_omits_empty_emails_list() {
        // DescribeUserOutput with empty emails should not add an empty list
        // attribute — that's just noise downstream.
        let resp = DescribeUserOutput::builder()
            .user_id("uid")
            .identity_store_id("d")
            .set_emails(Some(vec![]))
            .build()
            .unwrap();
        let attrs = extract_identitystore_user(&resp);
        assert!(!attrs.contains_key("emails"));
    }
}
