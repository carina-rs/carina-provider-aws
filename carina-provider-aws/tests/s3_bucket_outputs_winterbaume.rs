//! Provider-level coverage for managed S3 bucket outputs against the
//! in-process winterbaume S3 service. No real AWS account is contacted.

#![cfg(not(target_arch = "wasm32"))]

use aws_sdk_acm::Client as AcmClient;
use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_identitystore::Client as IdentityStoreClient;
use aws_sdk_organizations::Client as OrganizationsClient;
use aws_sdk_route53::Client as Route53Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sts::Client as StsClient;
use carina_core::provider::{CreateRequest, Provider, ReadRequest};
use carina_core::resource::{ConcreteValue, DataSource, ResolvedResource, Resource, State, Value};
use carina_provider_aws::AwsProvider;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use winterbaume_core::{
    MockAws, MockRequest, MockResponse, MockService, StatefulService, default_account_id,
};
use winterbaume_s3::views::BucketStateView;
use winterbaume_s3::{S3Service, S3StateView};

const PROVIDER_REGION: &str = "us-east-1";
const BUCKET_REGION: &str = "ap-northeast-1";
const RESOURCE_TYPE: &str = "s3.Bucket";

struct RecordingS3Service {
    inner: Arc<S3Service>,
    get_bucket_location_calls: AtomicUsize,
}

impl RecordingS3Service {
    fn new(inner: Arc<S3Service>) -> Self {
        Self {
            inner,
            get_bucket_location_calls: AtomicUsize::new(0),
        }
    }

    fn get_bucket_location_calls(&self) -> usize {
        self.get_bucket_location_calls.load(Ordering::Relaxed)
    }
}

impl MockService for RecordingS3Service {
    fn service_name(&self) -> &str {
        self.inner.service_name()
    }

    fn url_patterns(&self) -> Vec<&str> {
        self.inner.url_patterns()
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let is_get_bucket_location = request.method == "GET"
            && request.uri.split_once('?').is_some_and(|(_, query)| {
                query
                    .split('&')
                    .any(|parameter| parameter.split('=').next() == Some("location"))
            });
        if is_get_bucket_location {
            self.get_bucket_location_calls
                .fetch_add(1, Ordering::Relaxed);
        }
        self.inner.handle(request)
    }
}

fn provider_from_config(sdk_config: &aws_types::SdkConfig) -> AwsProvider {
    AwsProvider::from_clients(
        PROVIDER_REGION.to_string(),
        Vec::new(),
        Vec::new(),
        S3Client::new(sdk_config),
        Ec2Client::new(sdk_config),
        IamClient::new(sdk_config),
        CloudWatchLogsClient::new(sdk_config),
        StsClient::new(sdk_config),
        OrganizationsClient::new(sdk_config),
        IdentityStoreClient::new(sdk_config),
        Route53Client::new(sdk_config),
        AcmClient::new(sdk_config),
        SqsClient::new(sdk_config),
    )
}

async fn provider_with_s3() -> (AwsProvider, S3Client) {
    let mock = MockAws::builder().with_service(S3Service::new()).build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    let s3_client = S3Client::new(&sdk_config);
    (provider_from_config(&sdk_config), s3_client)
}

async fn provider_with_staged_bucket(bucket: &str) -> (AwsProvider, Arc<RecordingS3Service>) {
    let s3_service = Arc::new(S3Service::new());

    // Winterbaume normally derives a created bucket's stored region from the
    // signed request. Its public restore seam lets this test place a bucket with
    // a distinct region in the provider/signing region's request scope.
    s3_service
        .restore(
            default_account_id(),
            PROVIDER_REGION,
            S3StateView {
                buckets: HashMap::from([(
                    bucket.to_string(),
                    BucketStateView {
                        name: bucket.to_string(),
                        region: BUCKET_REGION.to_string(),
                        ..Default::default()
                    },
                )]),
            },
        )
        .await
        .expect("stage S3 bucket with a distinct region");

    let recording_service = Arc::new(RecordingS3Service::new(s3_service));
    let mock = MockAws::builder()
        .with_service(Arc::clone(&recording_service))
        .build();
    let sdk_config = mock.sdk_config(PROVIDER_REGION).await;
    (provider_from_config(&sdk_config), recording_service)
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string()))
}

fn assert_bucket_outputs(state: &State, bucket: &str, region: &str, hosted_zone_id: &str) {
    assert!(state.exists);
    assert_eq!(state.attributes.get("bucket"), Some(&string(bucket)));
    assert_eq!(
        state.attributes.get("arn"),
        Some(&string(&format!("arn:aws:s3:::{bucket}")))
    );
    assert_eq!(state.attributes.get("region"), Some(&string(region)));
    assert_eq!(
        state.attributes.get("bucket_domain_name"),
        Some(&string(&format!("{bucket}.s3.amazonaws.com")))
    );
    assert_eq!(
        state.attributes.get("bucket_regional_domain_name"),
        Some(&string(&format!("{bucket}.s3.{region}.amazonaws.com")))
    );
    assert_eq!(
        state.attributes.get("hosted_zone_id"),
        Some(&string(hosted_zone_id))
    );
}

#[tokio::test]
async fn create_bucket_ignores_supplied_read_only_region_for_placement() {
    let (provider, client) = provider_with_s3().await;
    let bucket = "carina-bucket-outputs";
    let supplied_read_only_region = BUCKET_REGION;
    let mut resource = Resource::with_provider("aws", RESOURCE_TYPE, "outputs", None);
    resource.set_attr("bucket", string(bucket));
    // The provider region controls placement. Supplying this read-only value pins
    // the regression where create previously treated it as a placement override.
    resource.set_attr("region", string(supplied_read_only_region));
    let id = resource.id.clone();

    let state = provider
        .create(
            &id,
            CreateRequest {
                resource: ResolvedResource::new(resource),
            },
        )
        .await
        .expect("create S3 bucket")
        .into_state_for_writeback();

    let raw_location = client
        .get_bucket_location()
        .bucket(bucket)
        .send()
        .await
        .expect("read raw bucket location");
    assert_eq!(
        raw_location
            .location_constraint()
            .map(|constraint| constraint.as_str())
            .unwrap_or_default(),
        "",
        "winterbaume should reproduce S3's absent/empty us-east-1 location constraint"
    );

    assert_bucket_outputs(&state, bucket, PROVIDER_REGION, "Z3AQBSTGFYJSTF");
    assert_ne!(
        state.attributes.get("region"),
        Some(&string(supplied_read_only_region))
    );
}

#[tokio::test]
async fn managed_and_data_source_reads_use_head_bucket_region_without_get_bucket_location() {
    assert_ne!(
        PROVIDER_REGION, BUCKET_REGION,
        "the fixture must distinguish HeadBucket's region from the provider region"
    );

    let bucket = "carina-bucket-head-region";
    let (provider, service) = provider_with_staged_bucket(bucket).await;

    let managed_id = Resource::with_provider("aws", RESOURCE_TYPE, "managed", None).id;
    let managed_state = provider
        .read(&managed_id, Some(bucket), ReadRequest)
        .await
        .expect("read managed S3 bucket");
    assert_bucket_outputs(&managed_state, bucket, BUCKET_REGION, "Z2M4EHUR26P7ZW");

    let mut data_source = DataSource::with_provider("aws", RESOURCE_TYPE, "lookup", None);
    data_source.set_attr("bucket", string(bucket));
    let data_source_state = provider
        .read_data_source(&data_source)
        .await
        .expect("read S3 bucket data source");
    assert_bucket_outputs(&data_source_state, bucket, BUCKET_REGION, "Z2M4EHUR26P7ZW");

    assert_eq!(
        service.get_bucket_location_calls(),
        0,
        "both read paths must use HeadBucket's x-amz-bucket-region"
    );
}
