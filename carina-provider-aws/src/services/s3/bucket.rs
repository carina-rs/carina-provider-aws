use indexmap::IndexMap;
use std::collections::HashMap;

use carina_core::provider::{BoxFuture, ProviderError, ProviderResult};
use carina_core::resource::{
    ConcreteValue, DataSource, Directives, Resource, ResourceId, State, Value,
};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, retry_aws_operation, sdk_error_message};

/// The complete read-only projection shared by managed and data-source
/// reads of an S3 bucket.
struct S3BucketReadOnlyOutputs {
    arn: String,
    region: String,
    bucket_domain_name: String,
    bucket_regional_domain_name: String,
    hosted_zone_id: Option<String>,
}

impl S3BucketReadOnlyOutputs {
    /// Consume the projection as one unit so neither read path selects a
    /// different subset of outputs.
    fn into_attributes(self) -> HashMap<String, Value> {
        let mut attributes = HashMap::from([
            (
                "arn".to_string(),
                Value::Concrete(ConcreteValue::String(self.arn)),
            ),
            (
                "region".to_string(),
                Value::Concrete(ConcreteValue::String(self.region)),
            ),
            (
                "bucket_domain_name".to_string(),
                Value::Concrete(ConcreteValue::String(self.bucket_domain_name)),
            ),
            (
                "bucket_regional_domain_name".to_string(),
                Value::Concrete(ConcreteValue::String(self.bucket_regional_domain_name)),
            ),
        ]);

        // A miss must omit the attribute rather than break resource management. This
        // remains the safety net for regions AWS adds after the table was fetched;
        // isolated GovCloud and China partitions remain out of scope.
        if let Some(hosted_zone_id) = self.hosted_zone_id {
            attributes.insert(
                "hosted_zone_id".to_string(),
                Value::Concrete(ConcreteValue::String(hosted_zone_id)),
            );
        }

        attributes
    }
}

/// Compute every read-only output exposed by both views of `s3.Bucket`.
fn s3_bucket_read_only_outputs(bucket: &str, region: &str) -> S3BucketReadOnlyOutputs {
    // TODO: Partition and DNS-suffix context is not available here yet, so these
    // derived values currently hard-code the commercial `aws` / `amazonaws.com`
    // forms.
    S3BucketReadOnlyOutputs {
        arn: format!("arn:aws:s3:::{bucket}"),
        region: region.to_string(),
        bucket_domain_name: format!("{bucket}.s3.amazonaws.com"),
        bucket_regional_domain_name: format!("{bucket}.s3.{region}.amazonaws.com"),
        hosted_zone_id: s3_hosted_zone_id(region).ok().map(str::to_string),
    }
}

fn normalize_bucket_location(region: Option<&str>) -> String {
    // Older HeadBucket responses can omit `x-amz-bucket-region`. Normalize an
    // absent/empty value to us-east-1 and preserve S3's legacy EU alias.
    match region {
        None | Some("") => "us-east-1",
        Some("EU") => "eu-west-1",
        Some(region) => region,
    }
    .to_string()
}

fn create_bucket_location_constraint(provider_region: &str) -> Option<&str> {
    (provider_region != "us-east-1").then_some(provider_region)
}

impl AwsProvider {
    /// Read an S3 bucket
    pub(crate) async fn read_s3_bucket(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(name) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        match self.s3_client.head_bucket().bucket(name).send().await {
            Ok(output) => {
                // `HeadBucketOutput::bucket_region()` exposes the response's
                // `x-amz-bucket-region`. AWS's vendored Smithy model deprecates
                // GetBucketLocation for this purpose, so using this response avoids
                // another API round trip and another IAM permission.
                let region = normalize_bucket_location(output.bucket_region());
                let mut attributes = s3_bucket_read_only_outputs(name, &region).into_attributes();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(name.to_string())),
                );

                // Get tags
                self.read_s3_bucket_tags(id, name, &mut attributes).await?;

                // S3 bucket identifier is the bucket name
                Ok(State::existing(id.clone(), attributes).with_identifier(name))
            }
            Err(err) => {
                // Handle bucket not found
                use aws_sdk_s3::error::SdkError;

                let error_kind = match &err {
                    SdkError::ServiceError(service_err) => {
                        let status = service_err.raw().status().as_u16();
                        classify_head_bucket_status(status, service_err.err().is_not_found())
                    }
                    _ => HeadBucketErrorKind::Other,
                };

                match error_kind {
                    HeadBucketErrorKind::NotFound => Ok(State::not_found(id.clone())),
                    HeadBucketErrorKind::AccessDenied => Err(ProviderError::api_error(format!(
                        "Access denied for bucket '{}'. This may indicate insufficient IAM \
                         permissions or the bucket is owned by a different AWS account.",
                        name
                    ))
                    .for_resource(id.clone())),
                    HeadBucketErrorKind::Other => {
                        Err(
                            api_error_with_meta("Failed to read bucket", "s3.HeadBucket", err)
                                .for_resource(id.clone()),
                        )
                    }
                }
            }
        }
    }

    /// Create an S3 bucket
    pub(crate) async fn create_s3_bucket(&self, resource: &Resource) -> ProviderResult<State> {
        let bucket_name = match resource.get_attr("bucket") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
            _ => {
                return Err(ProviderError::invalid_input("Bucket name is required")
                    .for_resource(resource.id.clone()));
            }
        };

        // Create bucket
        let mut req = self.s3_client.create_bucket().bucket(&bucket_name);

        // `region` is a read-only output, not a placement input. Derive the
        // LocationConstraint only from the provider region even if a caller supplies
        // that attribute while carina-core still permits read-only writes (carina#3766).
        if let Some(region) = create_bucket_location_constraint(&self.region) {
            use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};
            let constraint = BucketLocationConstraint::from(region);
            let config = CreateBucketConfiguration::builder()
                .location_constraint(constraint)
                .build();
            req = req.create_bucket_configuration(config);
        }

        // ObjectLock / ObjectOwnership / ACL / Grants moved to standalone
        // resources (s3.BucketAcl, s3.BucketOwnershipControls). The bucket
        // here is identity-only.

        // Retry transient errors such as OperationAborted (HTTP 409) which
        // S3 returns when CreateBucket races a recent DeleteBucket for the
        // same name — the control plane clears after ~60–90s. See #156.
        let rid = resource.id.clone();
        retry_aws_operation("create S3 bucket", RetryPolicy::default(), || {
            let req = req.clone();
            async move { req.send().await }
        })
        .await
        .map_err(|e| {
            api_error_with_meta("Failed to create bucket", "s3.CreateBucket", e)
                .for_resource(rid.clone())
        })?;

        // Set tags
        let attrs = resource.resolved_attributes();
        self.write_s3_bucket_tags(&resource.id, &bucket_name, &attrs)
            .await?;

        // Return state after creation
        self.read_s3_bucket(&resource.id, Some(&bucket_name)).await
    }

    /// Update an S3 bucket
    pub(crate) async fn update_s3_bucket(
        &self,
        id: ResourceId,
        identifier: &str,
        from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        let bucket_name = identifier.to_string();

        let attrs = to.resolved_attributes();

        // Update tags: if tags were removed entirely, delete all tags
        if from.attributes.contains_key("tags") && !to.attributes.contains_key("tags") {
            self.s3_client
                .delete_bucket_tagging()
                .bucket(&bucket_name)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to delete bucket tags", "s3.DeleteBucketTagging", e)
                        .for_resource(id.clone())
                })?;
        } else {
            self.write_s3_bucket_tags(&id, &bucket_name, &attrs).await?;
        }

        self.read_s3_bucket(&id, Some(&bucket_name)).await
    }

    /// Delete an S3 bucket, honoring lifecycle.force_delete
    pub(crate) async fn delete_s3_bucket(
        &self,
        id: ResourceId,
        identifier: &str,
        directives: &Directives,
    ) -> ProviderResult<()> {
        // If force_delete is enabled, empty the bucket before deletion
        if directives.force_delete {
            self.empty_s3_bucket(&id, identifier).await?;
        }

        retry_aws_operation("delete S3 bucket", RetryPolicy::default(), || {
            let client = &self.s3_client;
            async move { client.delete_bucket().bucket(identifier).send().await }
        })
        .await
        .map_err(|e| {
            api_error_with_meta("Failed to delete bucket", "s3.DeleteBucket", e)
                .for_resource(id.clone())
        })?;
        Ok(())
    }

    /// Empty an S3 bucket by deleting all objects and versions
    async fn empty_s3_bucket(&self, id: &ResourceId, bucket_name: &str) -> ProviderResult<()> {
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            let mut req = self
                .s3_client
                .list_object_versions()
                .bucket(bucket_name)
                .max_keys(1000);
            if let Some(ref km) = key_marker {
                req = req.key_marker(km);
            }
            if let Some(ref vim) = version_id_marker {
                req = req.version_id_marker(vim);
            }

            let response = req.send().await.map_err(|e| {
                api_error_with_meta("Failed to list object versions", "s3.ListObjectVersions", e)
                    .for_resource(id.clone())
            })?;

            let mut objects_to_delete = Vec::new();

            // Collect versions
            for version in response.versions() {
                if let Some(key) = version.key() {
                    let mut oid = aws_sdk_s3::types::ObjectIdentifier::builder().key(key);
                    if let Some(vid) = version.version_id() {
                        oid = oid.version_id(vid);
                    }
                    objects_to_delete.push(oid.build().map_err(|e| {
                        ProviderError::api_error(sdk_error_message(
                            "Failed to build ObjectIdentifier",
                            &e,
                        ))
                        .for_resource(id.clone())
                    })?);
                }
            }

            // Collect delete markers
            for marker in response.delete_markers() {
                if let Some(key) = marker.key() {
                    let mut oid = aws_sdk_s3::types::ObjectIdentifier::builder().key(key);
                    if let Some(vid) = marker.version_id() {
                        oid = oid.version_id(vid);
                    }
                    objects_to_delete.push(oid.build().map_err(|e| {
                        ProviderError::api_error(sdk_error_message(
                            "Failed to build ObjectIdentifier",
                            &e,
                        ))
                        .for_resource(id.clone())
                    })?);
                }
            }

            // Batch delete (max 1000 per request)
            if !objects_to_delete.is_empty() {
                let delete = aws_sdk_s3::types::Delete::builder()
                    .set_objects(Some(objects_to_delete))
                    .quiet(true)
                    .build()
                    .map_err(|e| {
                        ProviderError::api_error(sdk_error_message(
                            "Failed to build Delete request",
                            &e,
                        ))
                        .for_resource(id.clone())
                    })?;

                self.s3_client
                    .delete_objects()
                    .bucket(bucket_name)
                    .delete(delete)
                    .send()
                    .await
                    .map_err(|e| {
                        api_error_with_meta("Failed to delete objects", "s3.DeleteObjects", e)
                            .for_resource(id.clone())
                    })?;
            }

            if response.is_truncated() == Some(true) {
                key_marker = response.next_key_marker().map(|s| s.to_string());
                version_id_marker = response.next_version_id_marker().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Read S3 bucket tags
    async fn read_s3_bucket_tags(
        &self,
        id: &ResourceId,
        identifier: &str,
        attributes: &mut HashMap<String, Value>,
    ) -> ProviderResult<()> {
        match self
            .s3_client
            .get_bucket_tagging()
            .bucket(identifier)
            .send()
            .await
        {
            Ok(output) => {
                let mut tag_map: IndexMap<String, Value> = IndexMap::new();
                for tag in output.tag_set() {
                    tag_map.insert(
                        tag.key().to_string(),
                        Value::Concrete(ConcreteValue::String(tag.value().to_string())),
                    );
                }
                if !tag_map.is_empty() {
                    attributes.insert(
                        "tags".to_string(),
                        Value::Concrete(ConcreteValue::Map(tag_map)),
                    );
                }
            }
            Err(err) => {
                if !is_s3_not_configured_error(&err, "NoSuchTagSet") {
                    return Err(api_error_with_meta(
                        "Failed to read bucket tagging",
                        "s3.GetBucketTagging",
                        err,
                    )
                    .for_resource(id.clone()));
                }
            }
        }
        Ok(())
    }

    /// Write S3 bucket tags
    pub(crate) async fn write_s3_bucket_tags(
        &self,
        id: &ResourceId,
        identifier: &str,
        attributes: &HashMap<String, Value>,
    ) -> ProviderResult<()> {
        if let Some(Value::Concrete(ConcreteValue::Map(tag_map))) = attributes.get("tags") {
            use aws_sdk_s3::types::{Tag, Tagging};
            let tags: Vec<Tag> = tag_map
                .iter()
                .filter_map(|(k, v)| {
                    if let Value::Concrete(ConcreteValue::String(val)) = v {
                        Some(Tag::builder().key(k).value(val).build().ok()?)
                    } else {
                        None
                    }
                })
                .collect();

            let tagging = Tagging::builder()
                .set_tag_set(Some(tags))
                .build()
                .map_err(|e| {
                    ProviderError::api_error(sdk_error_message("Failed to build tagging", &e))
                        .for_resource(id.clone())
                })?;

            self.s3_client
                .put_bucket_tagging()
                .bucket(identifier)
                .tagging(tagging)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to put bucket tags", "s3.PutBucketTagging", e)
                        .for_resource(id.clone())
                })?;
        }

        Ok(())
    }

    /// Read `s3.Bucket` as a data source. Looks the bucket up by name (the
    /// required `bucket` input) via `HeadBucket`, then computes the
    /// Terraform-parity attributes (`arn`,
    /// `bucket_domain_name`, `bucket_regional_domain_name`,
    /// `hosted_zone_id`).
    ///
    /// Wired into `DataSourceLookups` via
    /// `data_source_lookups::DataSourceLookups`.
    pub(crate) fn do_read_s3_bucket_data_source(
        &self,
        resource: &DataSource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.clone();
        Box::pin(async move {
            let bucket = match resource.get_attr("bucket") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
                _ => {
                    return Err(ProviderError::invalid_input("`bucket` is required")
                        .for_resource(resource.id.clone()));
                }
            };

            // 1. HeadBucket — existence check. Reuses the Managed-side
            //    classifier so the access-denied / not-found / other split
            //    is consistent. For a DataSource lookup the user explicitly
            //    asked to read this bucket, so missing bucket is an error
            //    (unlike read_s3_bucket which returns State::not_found).
            let output = self
                .s3_client
                .head_bucket()
                .bucket(&bucket)
                .send()
                .await
                .map_err(|err| {
                    use aws_sdk_s3::error::SdkError;
                    let kind = match &err {
                        SdkError::ServiceError(svc) => classify_head_bucket_status(
                            svc.raw().status().as_u16(),
                            svc.err().is_not_found(),
                        ),
                        _ => HeadBucketErrorKind::Other,
                    };
                    match kind {
                        HeadBucketErrorKind::NotFound => {
                            ProviderError::not_found(format!("Bucket '{bucket}' not found"))
                                .for_resource(resource.id.clone())
                        }
                        HeadBucketErrorKind::AccessDenied => ProviderError::api_error(format!(
                            "Access denied for bucket '{bucket}'. This may indicate \
                             insufficient IAM permissions or the bucket is owned by a \
                             different AWS account."
                        ))
                        .for_resource(resource.id.clone()),
                        HeadBucketErrorKind::Other => {
                            api_error_with_meta("Failed to head bucket", "s3.HeadBucket", err)
                                .for_resource(resource.id.clone())
                        }
                    }
                })?;

            // 2. Derive the region from HeadBucket's `x-amz-bucket-region` and
            //    use the same complete output projection as the managed read path.
            let region = normalize_bucket_location(output.bucket_region());
            let mut attrs = s3_bucket_read_only_outputs(&bucket, &region).into_attributes();
            attrs.insert(
                "bucket".into(),
                Value::Concrete(ConcreteValue::String(bucket.clone())),
            );

            Ok(State::existing(resource.id.clone(), attrs).with_identifier(&bucket))
        })
    }
}

/// Map AWS region → S3 website-endpoint hosted zone ID.
///
/// Source (fetched 2026-08-15):
/// https://docs.aws.amazon.com/general/latest/gr/s3.html
/// "Amazon S3 website endpoints and HostedZone IDs" table.
/// Limited to commercial regions; isolated partitions (GovCloud, China)
/// are out of scope.
pub(crate) fn s3_hosted_zone_id(region: &str) -> Result<&'static str, String> {
    let id = match region {
        "us-east-1" => "Z3AQBSTGFYJSTF",
        "us-east-2" => "Z2O1EMRO9K5GLX",
        "us-west-1" => "Z2F56UZL2M1ACD",
        "us-west-2" => "Z3BJ6K6RIION7M",
        "af-south-1" => "Z2OSFR5PIJ8TYW",
        "ap-east-1" => "ZNB98KWMFR0R6",
        "ap-east-2" => "Z064739330DAH7WJVOO93",
        "ap-south-1" => "Z11RGJOFQNVJUP",
        "ap-south-2" => "Z02976202B4EZMXIPMXF7",
        "ap-northeast-1" => "Z2M4EHUR26P7ZW",
        "ap-northeast-2" => "Z3W03O7B5YMIYP",
        "ap-northeast-3" => "Z2YQB5RD63NC85",
        "ap-southeast-1" => "Z3O0J2DXBE1FTB",
        "ap-southeast-2" => "Z1WCIGYICN2BYD",
        "ap-southeast-3" => "Z01846753K324LI26A3VV",
        "ap-southeast-4" => "Z0312387243XT5FE14WFO",
        "ap-southeast-5" => "Z08660063OXLMA7F1FJHU",
        "ap-southeast-6" => "Z05686083R66JX5C163TC",
        "ap-southeast-7" => "Z0031014GXUMRZG6I14G",
        "ca-central-1" => "Z1QDHH18159H29",
        "ca-west-1" => "Z03565811Z33SLEZTHOUL",
        "eu-central-1" => "Z21DNDUVLTQW6Q",
        "eu-central-2" => "Z030506016YDQGETNASS",
        "eu-west-1" => "Z1BKCTXD74EZPE",
        "eu-west-2" => "Z3GKZC51ZF0DB4",
        "eu-west-3" => "Z3R1K369G5AVDG",
        "eu-north-1" => "Z3BAZG2TWCNX0D",
        "eu-south-1" => "Z2OPA49AB41N7K",
        "eu-south-2" => "Z0081959F7139GRJC19J",
        "il-central-1" => "Z09640613K4A3MN55U7GU",
        "mx-central-1" => "Z057606446ZNVQJJ8WOP",
        "me-central-1" => "Z06143092I8HRXZRUZROF",
        "me-south-1" => "Z1MPMWCPA7YB62",
        "sa-east-1" => "Z7KQH4QJS55SO",
        _ => return Err(format!("Unknown S3 region: '{region}'")),
    };
    Ok(id)
}

/// Result of classifying an S3 HeadBucket error.
#[derive(Debug, PartialEq)]
enum HeadBucketErrorKind {
    /// Bucket does not exist (301 redirect to wrong region, or 404 not found).
    NotFound,
    /// Access denied (403) - could be permissions issue or bucket owned by another account.
    AccessDenied,
    /// Other error that should be propagated.
    Other,
}

/// Classify an HTTP status code from a HeadBucket error.
///
/// This is extracted as a pure function to enable unit testing.
fn classify_head_bucket_status(status: u16, is_not_found_error: bool) -> HeadBucketErrorKind {
    if is_not_found_error || status == 301 || status == 404 {
        HeadBucketErrorKind::NotFound
    } else if status == 403 {
        HeadBucketErrorKind::AccessDenied
    } else {
        HeadBucketErrorKind::Other
    }
}

/// Check if an S3 SDK error is a "not configured" error that should be silently ignored.
pub(crate) fn is_s3_not_configured_error<E: aws_sdk_s3::error::ProvideErrorMetadata>(
    err: &aws_sdk_s3::error::SdkError<E>,
    expected_code: &str,
) -> bool {
    use aws_sdk_s3::error::SdkError;
    match err {
        SdkError::ServiceError(service_err) => service_err.err().code() == Some(expected_code),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn concrete_string<'a>(attributes: &'a HashMap<String, Value>, name: &str) -> &'a str {
        match attributes.get(name) {
            Some(Value::Concrete(ConcreteValue::String(value))) => value,
            other => panic!("expected concrete string attribute `{name}`, got {other:?}"),
        }
    }

    #[test]
    fn normalize_bucket_location_handles_s3_spellings() {
        assert_eq!(normalize_bucket_location(None), "us-east-1");
        assert_eq!(normalize_bucket_location(Some("")), "us-east-1");
        assert_eq!(normalize_bucket_location(Some("EU")), "eu-west-1");
        assert_eq!(
            normalize_bucket_location(Some("ap-northeast-1")),
            "ap-northeast-1"
        );
    }

    #[test]
    fn create_bucket_location_constraint_uses_only_the_provider_region() {
        let provider_region = "ap-northeast-1";
        let supplied_read_only_region = "us-west-2";

        let constraint = create_bucket_location_constraint(provider_region);

        assert_eq!(constraint, Some(provider_region));
        assert_ne!(constraint, Some(supplied_read_only_region));
        assert_eq!(create_bucket_location_constraint("us-east-1"), None);
    }

    #[test]
    fn legacy_eu_location_produces_eu_west_1_bucket_outputs() {
        let region = normalize_bucket_location(Some("EU"));
        let attributes = s3_bucket_read_only_outputs("NAME", &region).into_attributes();

        assert_eq!(
            concrete_string(&attributes, "bucket_regional_domain_name"),
            "NAME.s3.eu-west-1.amazonaws.com"
        );
        assert_eq!(
            concrete_string(&attributes, "hosted_zone_id"),
            "Z1BKCTXD74EZPE"
        );
    }

    #[test]
    fn s3_bucket_read_only_outputs_are_computed_from_bucket_and_region() {
        let attributes =
            s3_bucket_read_only_outputs("example-bucket", "ap-northeast-1").into_attributes();

        assert_eq!(
            concrete_string(&attributes, "arn"),
            "arn:aws:s3:::example-bucket"
        );
        assert_eq!(concrete_string(&attributes, "region"), "ap-northeast-1");
        assert_eq!(
            concrete_string(&attributes, "bucket_domain_name"),
            "example-bucket.s3.amazonaws.com"
        );
        assert_eq!(
            concrete_string(&attributes, "bucket_regional_domain_name"),
            "example-bucket.s3.ap-northeast-1.amazonaws.com"
        );
        assert_eq!(
            concrete_string(&attributes, "hosted_zone_id"),
            "Z2M4EHUR26P7ZW"
        );
    }

    #[test]
    fn s3_bucket_read_only_outputs_omit_hosted_zone_id_for_unknown_region() {
        let attributes =
            s3_bucket_read_only_outputs("example-bucket", "xx-fake-1").into_attributes();

        assert_eq!(
            concrete_string(&attributes, "arn"),
            "arn:aws:s3:::example-bucket"
        );
        assert_eq!(concrete_string(&attributes, "region"), "xx-fake-1");
        assert_eq!(
            concrete_string(&attributes, "bucket_domain_name"),
            "example-bucket.s3.amazonaws.com"
        );
        assert_eq!(
            concrete_string(&attributes, "bucket_regional_domain_name"),
            "example-bucket.s3.xx-fake-1.amazonaws.com"
        );
        assert!(!attributes.contains_key("hosted_zone_id"));
    }

    #[test]
    fn generated_schema_read_only_names_match_runtime_projection() {
        const TABLE_REGION: &str = "ap-northeast-1";
        const NON_TABLE_REGION: &str = "xx-fake-1";
        const ALWAYS_PRESENT: [&str; 4] = [
            "arn",
            "region",
            "bucket_domain_name",
            "bucket_regional_domain_name",
        ];

        assert!(
            s3_hosted_zone_id(TABLE_REGION).is_ok(),
            "TABLE_REGION must exercise a hosted-zone table hit"
        );
        assert!(
            s3_hosted_zone_id(NON_TABLE_REGION).is_err(),
            "NON_TABLE_REGION must exercise a hosted-zone table miss"
        );

        let managed_config = crate::schemas::generated::s3::bucket::s3_bucket_config();
        let managed_declared: BTreeSet<_> = managed_config
            .schema
            .attributes
            .iter()
            .filter(|(_, attribute)| attribute.read_only)
            .map(|(name, _)| name.as_str())
            .collect();

        let data_source_config =
            crate::schemas::generated::s3::bucket_data_source::s3_bucket_data_source_config();
        assert!(
            data_source_config.schema.attributes["bucket"].required,
            "data-source `bucket` must remain its required input"
        );
        // Data-source codegen does not mark outputs read-only, and `bucket` is the
        // required input that is also echoed. Excluding that one input pins every
        // remaining generated output name to the shared runtime projection.
        let data_source_declared: BTreeSet<_> = data_source_config
            .schema
            .attributes
            .iter()
            .filter(|(name, _)| name.as_str() != "bucket")
            .map(|(name, _)| name.as_str())
            .collect();

        let table_attributes =
            s3_bucket_read_only_outputs("example-bucket", TABLE_REGION).into_attributes();
        let table_populated: BTreeSet<_> = table_attributes.keys().map(String::as_str).collect();
        let non_table_attributes =
            s3_bucket_read_only_outputs("example-bucket", NON_TABLE_REGION).into_attributes();
        let non_table_populated: BTreeSet<_> =
            non_table_attributes.keys().map(String::as_str).collect();

        for name in ALWAYS_PRESENT {
            assert!(
                managed_declared.contains(name),
                "managed schema omitted `{name}`"
            );
            assert!(
                data_source_declared.contains(name),
                "data-source schema omitted `{name}`"
            );
            assert!(
                table_populated.contains(name),
                "table-region projection omitted `{name}`"
            );
            assert!(
                non_table_populated.contains(name),
                "non-table-region projection omitted `{name}`"
            );
        }

        assert!(managed_declared.contains("hosted_zone_id"));
        assert!(data_source_declared.contains("hosted_zone_id"));
        assert!(table_populated.contains("hosted_zone_id"));
        assert!(!non_table_populated.contains("hosted_zone_id"));

        assert_eq!(
            managed_declared, data_source_declared,
            "managed and data-source s3.Bucket output names drifted"
        );
        assert_eq!(
            managed_declared, table_populated,
            "generated s3.Bucket output names drifted from table-region projection keys"
        );

        // `hosted_zone_id` is declared but conditional at runtime so regions added by
        // AWS after the static table was fetched do not break resource management.
        // Remove exactly that key before making the unknown-region comparison so
        // schema/runtime drift remains an exact invariant rather than a subset check.
        let mut expected_non_table = managed_declared;
        assert!(expected_non_table.remove("hosted_zone_id"));
        assert_eq!(
            expected_non_table, non_table_populated,
            "non-table-region projection drifted beyond conditional `hosted_zone_id`"
        );
    }

    #[test]
    fn s3_hosted_zone_id_known_regions() {
        assert_eq!(
            s3_hosted_zone_id("ap-northeast-1").unwrap(),
            "Z2M4EHUR26P7ZW"
        );
        assert_eq!(s3_hosted_zone_id("us-east-1").unwrap(), "Z3AQBSTGFYJSTF");
        assert_eq!(s3_hosted_zone_id("eu-west-1").unwrap(), "Z1BKCTXD74EZPE");
        assert_eq!(s3_hosted_zone_id("af-south-1").unwrap(), "Z2OSFR5PIJ8TYW");
        assert_eq!(
            s3_hosted_zone_id("ap-southeast-6").unwrap(),
            "Z05686083R66JX5C163TC"
        );
        assert_eq!(
            s3_hosted_zone_id("il-central-1").unwrap(),
            "Z09640613K4A3MN55U7GU"
        );
        assert_eq!(
            s3_hosted_zone_id("mx-central-1").unwrap(),
            "Z057606446ZNVQJJ8WOP"
        );
        assert_eq!(s3_hosted_zone_id("eu-south-1").unwrap(), "Z2OPA49AB41N7K");
    }

    #[test]
    fn s3_hosted_zone_id_unknown_region_errors() {
        let err = s3_hosted_zone_id("xx-fake-1").unwrap_err();
        assert!(err.contains("Unknown S3 region"));
    }

    #[test]
    fn test_classify_head_bucket_status_403_is_access_denied() {
        // 403 should be classified as AccessDenied, not NotFound
        assert_eq!(
            classify_head_bucket_status(403, false),
            HeadBucketErrorKind::AccessDenied
        );
    }

    #[test]
    fn test_classify_head_bucket_status_404_is_not_found() {
        assert_eq!(
            classify_head_bucket_status(404, false),
            HeadBucketErrorKind::NotFound
        );
    }

    #[test]
    fn test_classify_head_bucket_status_301_is_not_found() {
        assert_eq!(
            classify_head_bucket_status(301, false),
            HeadBucketErrorKind::NotFound
        );
    }

    #[test]
    fn test_classify_head_bucket_status_sdk_not_found_error() {
        // When the SDK itself reports is_not_found, it should be NotFound
        assert_eq!(
            classify_head_bucket_status(400, true),
            HeadBucketErrorKind::NotFound
        );
    }

    #[test]
    fn test_classify_head_bucket_status_other() {
        assert_eq!(
            classify_head_bucket_status(500, false),
            HeadBucketErrorKind::Other
        );
    }
}
