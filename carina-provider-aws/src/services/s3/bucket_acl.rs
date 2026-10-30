use aws_sdk_s3::types::{BucketCannedAcl, Grant, Permission, Type as GranteeType};
use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, retry_aws_operation};
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketAclReadModel;
use crate::send_models_generated::{S3BucketAclCreateInput, S3BucketAclUpdateInput};
use crate::services::s3::bucket::is_s3_not_configured_error;

/// AWS group URIs used to identify canned-ACL grant patterns.
const URI_ALL_USERS: &str = "http://acs.amazonaws.com/groups/global/AllUsers";
const URI_AUTHENTICATED_USERS: &str = "http://acs.amazonaws.com/groups/global/AuthenticatedUsers";
/// Read sentinel for a grant set that cannot be represented by any writable
/// canned ACL. This is deliberately outside the closed schema enum: it keeps
/// `acl` present and unequal to every desired value so planning can propose
/// the corrective PutBucketAcl update.
const CUSTOM_GRANTS_SENTINEL: &str = "__carina_custom_grants__";

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
/// surface in the schema. The read path reports that state as
/// unrepresentable instead of silently omitting `acl`.
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

fn observed_acl_value(grants: &[Grant], owner_id: Option<&str>) -> &'static str {
    infer_canned_acl_from_grants(grants, owner_id).unwrap_or(CUSTOM_GRANTS_SENTINEL)
}

impl AwsProvider {
    /// Read an S3 BucketAcl.
    ///
    /// AWS does not return a canned-ACL name from `GetBucketAcl`; it
    /// returns the underlying grant list. We infer which canned ACL
    /// produced the grant shape (private / public-read / public-read-write
    /// / authenticated-read) and write that into state so post-apply
    /// `plan-verify` is idempotent. Custom grant sets that do not match
    /// any canned shape are represented by a non-writable sentinel so refresh
    /// succeeds and the differ proposes an update to the desired canned ACL.
    pub(crate) async fn read_s3_bucket_acl(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self.s3_client.get_bucket_acl().bucket(bucket).send().await;

        match result {
            Ok(output) => {
                let owner_id = output.owner().and_then(|o| o.id());
                let canned = observed_acl_value(output.grants(), owner_id);
                let model = S3BucketAclReadModel {
                    bucket: crate::read_model::applied_identifier_value(identifier),
                    acl: Value::Concrete(ConcreteValue::String(canned.to_string().into())),
                };
                found_managed_read(id, applied_identifier, model)
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to get bucket ACL", e).for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_acl(
        &self,
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = S3BucketAclCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::S3BucketAclCreateParts { acl, bucket } =
            input.into_parts();
        let acl = acl.into_inner();
        let bucket = bucket.into_inner();
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketAcl", bucket)?;
        let outcome = self
            .put_s3_bucket_acl(&resource.id, &identifier, acl)
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

    pub(crate) async fn update_s3_bucket_acl(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let crate::send_models_generated::S3BucketAclUpdateParts { acl } =
            S3BucketAclUpdateInput::decode(&to)?.into_parts();
        let acl = acl.into_inner();
        self.put_s3_bucket_acl(&id, identifier, acl).await
    }

    async fn put_s3_bucket_acl(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        acl: String,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        self.s3_client
            .put_bucket_acl()
            .bucket(bucket)
            .acl(BucketCannedAcl::from(acl.as_str()))
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket ACL", e).for_resource(id.clone())
            })?;

        Ok(self.read_s3_bucket_acl(id, applied_identifier).await)
    }

    /// "Delete" an S3 BucketAcl by resetting it to "private" — there is no
    /// DeleteBucketAcl API. NoSuchBucket is treated as success.
    pub(crate) async fn delete_s3_bucket_acl_reset(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client
                .put_bucket_acl()
                .bucket(identifier)
                .acl(BucketCannedAcl::Private)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => {
                Err(api_error_with_meta("Failed to reset bucket ACL", e).for_resource(id.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::Grantee;
    use carina_core::differ::{Diff, diff};
    use carina_core::provider::RawSavedAttrs;
    use carina_core::resource::{ResolvedResource, Resource};
    use carina_core::schema::SchemaRegistry;
    use std::collections::HashMap;

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
    fn custom_grants_refresh_and_plan_a_corrective_update() {
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
        let observed = observed_acl_value(&grants, Some("OWNER123"));
        assert_eq!(observed, CUSTOM_GRANTS_SENTINEL);

        let mut desired = Resource::with_provider("aws", "s3.BucketAcl", "example", None);
        desired.set_attr(
            "bucket",
            Value::Concrete(ConcreteValue::String("example".into())),
        );
        desired.set_attr(
            "acl",
            Value::Concrete(ConcreteValue::String("private".into())),
        );
        let current = crate::read_model::test_existing_state(
            desired.id.clone(),
            HashMap::from([
                (
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String("example".into())),
                ),
                (
                    "acl".to_string(),
                    Value::Concrete(ConcreteValue::String(observed.into())),
                ),
            ]),
        )
        .with_identifier("example");
        let registry = SchemaRegistry::new();
        let saved = RawSavedAttrs::default().lift(&registry);
        let schema = crate::schemas::generated::s3::bucket_acl::s3_bucket_acl_config().schema;

        let result = diff(
            &ResolvedResource::new(desired),
            &current,
            &saved,
            None,
            Some(&schema),
        );
        assert!(matches!(result, Diff::Update { .. }), "{result:?}");
    }

    #[test]
    fn infer_returns_none_without_owner() {
        let grants = vec![owner_full_control("OWNER123")];
        assert_eq!(infer_canned_acl_from_grants(&grants, None), None);
    }
}
