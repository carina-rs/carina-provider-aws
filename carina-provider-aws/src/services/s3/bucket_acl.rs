use std::collections::HashMap;

use aws_sdk_s3::types::{BucketCannedAcl, Grant, Permission, Type as GranteeType};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::utils::convert_enum_value;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation};
use crate::services::s3::bucket::is_s3_not_configured_error;

/// AWS group URIs used to identify canned-ACL grant patterns.
const URI_ALL_USERS: &str = "http://acs.amazonaws.com/groups/global/AllUsers";
const URI_AUTHENTICATED_USERS: &str = "http://acs.amazonaws.com/groups/global/AuthenticatedUsers";

/// Inspect the grant list returned by `GetBucketAcl` and infer which
/// canned ACL produced it, if any. Returns the wire-form name
/// (`"private"`, `"public-read"`, `"public-read-write"`,
/// `"authenticated-read"`) the SDK accepts as input to
/// `BucketCannedAcl::from`.
///
/// The four bucket-level canned ACLs Carina supports have fixed grant
/// shapes:
/// - `private`: owner FULL_CONTROL only.
/// - `public-read`: owner FULL_CONTROL + AllUsers READ.
/// - `public-read-write`: owner FULL_CONTROL + AllUsers READ + AllUsers WRITE.
/// - `authenticated-read`: owner FULL_CONTROL + AuthenticatedUsers READ.
///
/// Returns `None` for any grant shape that does not match — for
/// example, a custom grant set or a canned ACL Carina does not yet
/// surface in the schema. Callers should leave the `acl` attribute
/// absent in state in that case (state then disagrees with desired,
/// surfacing a real diff, which is the correct behavior).
fn infer_canned_acl_from_grants(grants: &[Grant], owner_id: Option<&str>) -> Option<&'static str> {
    let owner_id = owner_id?;
    // Bucket the grants by (grantee key, permission). Owner grants
    // are keyed by canonical-user ID; group grants by URI.
    let mut owner_perms: Vec<&Permission> = Vec::new();
    let mut all_users_perms: Vec<&Permission> = Vec::new();
    let mut auth_users_perms: Vec<&Permission> = Vec::new();
    for grant in grants {
        let Some(grantee) = grant.grantee() else {
            continue;
        };
        let Some(perm) = grant.permission() else {
            continue;
        };
        match (grantee.r#type(), grantee.id(), grantee.uri()) {
            (GranteeType::CanonicalUser, Some(id), _) if id == owner_id => {
                owner_perms.push(perm);
            }
            (GranteeType::CanonicalUser, _, _) => {
                // Non-owner canonical user grant — not a canned shape.
                return None;
            }
            (GranteeType::Group, _, Some(URI_ALL_USERS)) => all_users_perms.push(perm),
            (GranteeType::Group, _, Some(URI_AUTHENTICATED_USERS)) => {
                auth_users_perms.push(perm);
            }
            _ => return None,
        }
    }

    let owner_has_full = owner_perms
        .iter()
        .any(|p| matches!(p, Permission::FullControl));
    if !owner_has_full {
        return None;
    }

    match (
        all_users_perms.as_slice(),
        auth_users_perms.as_slice(),
        owner_perms.len(),
    ) {
        // private: owner FULL_CONTROL only.
        ([], [], 1) => Some("private"),
        // authenticated-read: owner FULL_CONTROL + AuthenticatedUsers READ.
        ([], [Permission::Read], 1) => Some("authenticated-read"),
        // public-read: owner FULL_CONTROL + AllUsers READ.
        ([Permission::Read], [], 1) => Some("public-read"),
        // public-read-write: owner FULL_CONTROL + AllUsers READ + AllUsers WRITE
        // (order returned by S3 is not guaranteed).
        (au, [], 1)
            if au.len() == 2
                && au.iter().any(|p| matches!(p, Permission::Read))
                && au.iter().any(|p| matches!(p, Permission::Write)) =>
        {
            Some("public-read-write")
        }
        _ => None,
    }
}

impl AwsProvider {
    /// Read an S3 BucketAcl.
    ///
    /// AWS does not return a canned-ACL name from `GetBucketAcl`; it
    /// returns the underlying grant list. We infer which canned ACL
    /// produced the grant shape (private / public-read / public-read-write
    /// / authenticated-read) and write that into state so post-apply
    /// `plan-verify` is idempotent. Custom grant sets that do not match
    /// any canned shape leave `acl` absent in state — the resulting diff
    /// is accurate (the bucket no longer matches the requested canned
    /// ACL).
    pub(crate) async fn read_s3_bucket_acl(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self.s3_client.get_bucket_acl().bucket(bucket).send().await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                let owner_id = output.owner().and_then(|o| o.id());
                if let Some(canned) = infer_canned_acl_from_grants(output.grants(), owner_id) {
                    attributes.insert(
                        "acl".to_string(),
                        Value::Concrete(ConcreteValue::String(canned.to_string())),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    api_error_with_meta("Failed to get bucket ACL", "s3.GetBucketAcl", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_acl(&self, resource: Resource) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_acl(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_acl(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_acl(&id, identifier, &to).await
    }

    async fn put_s3_bucket_acl(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let acl_str = match resource.get_attr("acl") {
            // convert_enum_value normalizes namespaced/typed identifiers
            // (`aws.s3.BucketAcl.Acl.public_read`) and snake-cased aliases
            // (`public_read` ⇄ `public-read`) back to AWS canonical form.
            Some(Value::Concrete(ConcreteValue::String(s))) => convert_enum_value(s).to_string(),
            _ => {
                return Err(
                    ProviderError::invalid_input("acl is required").for_resource(id.clone())
                );
            }
        };

        self.s3_client
            .put_bucket_acl()
            .bucket(bucket)
            .acl(BucketCannedAcl::from(acl_str.as_str()))
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket ACL", "s3.PutBucketAcl", e)
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_acl(id, Some(bucket)).await
    }

    /// "Delete" an S3 BucketAcl by resetting it to "private" — there is no
    /// DeleteBucketAcl API. NoSuchBucket is treated as success.
    pub(crate) async fn delete_s3_bucket_acl_reset(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = retry_aws_operation("reset bucket ACL", RetryPolicy::default(), || {
            let client = &self.s3_client;
            async move {
                client
                    .put_bucket_acl()
                    .bucket(identifier)
                    .acl(BucketCannedAcl::Private)
                    .send()
                    .await
            }
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(
                api_error_with_meta("Failed to reset bucket ACL", "s3.PutBucketAcl", e)
                    .for_resource(id.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::Grantee;

    fn owner_full_control(owner_id: &str) -> Grant {
        Grant::builder()
            .grantee(
                Grantee::builder()
                    .r#type(GranteeType::CanonicalUser)
                    .id(owner_id)
                    .build()
                    .unwrap(),
            )
            .permission(Permission::FullControl)
            .build()
    }

    fn group_grant(uri: &str, perm: Permission) -> Grant {
        Grant::builder()
            .grantee(
                Grantee::builder()
                    .r#type(GranteeType::Group)
                    .uri(uri)
                    .build()
                    .unwrap(),
            )
            .permission(perm)
            .build()
    }

    #[test]
    fn infer_private_canned_acl() {
        let grants = vec![owner_full_control("OWNER123")];
        assert_eq!(
            infer_canned_acl_from_grants(&grants, Some("OWNER123")),
            Some("private")
        );
    }

    #[test]
    fn infer_public_read_canned_acl() {
        let grants = vec![
            owner_full_control("OWNER123"),
            group_grant(URI_ALL_USERS, Permission::Read),
        ];
        assert_eq!(
            infer_canned_acl_from_grants(&grants, Some("OWNER123")),
            Some("public-read")
        );
    }

    #[test]
    fn infer_public_read_write_canned_acl() {
        // S3 may return grants in any order.
        let grants = vec![
            owner_full_control("OWNER123"),
            group_grant(URI_ALL_USERS, Permission::Write),
            group_grant(URI_ALL_USERS, Permission::Read),
        ];
        assert_eq!(
            infer_canned_acl_from_grants(&grants, Some("OWNER123")),
            Some("public-read-write")
        );
    }

    #[test]
    fn infer_authenticated_read_canned_acl() {
        let grants = vec![
            owner_full_control("OWNER123"),
            group_grant(URI_AUTHENTICATED_USERS, Permission::Read),
        ];
        assert_eq!(
            infer_canned_acl_from_grants(&grants, Some("OWNER123")),
            Some("authenticated-read")
        );
    }

    #[test]
    fn infer_returns_none_for_custom_grant() {
        // A non-owner canonical user grant doesn't match any canned ACL.
        let grants = vec![
            owner_full_control("OWNER123"),
            Grant::builder()
                .grantee(
                    Grantee::builder()
                        .r#type(GranteeType::CanonicalUser)
                        .id("ANOTHER_USER")
                        .build()
                        .unwrap(),
                )
                .permission(Permission::Read)
                .build(),
        ];
        assert_eq!(
            infer_canned_acl_from_grants(&grants, Some("OWNER123")),
            None
        );
    }

    #[test]
    fn infer_returns_none_without_owner() {
        let grants = vec![owner_full_control("OWNER123")];
        assert_eq!(infer_canned_acl_from_grants(&grants, None), None);
    }
}
