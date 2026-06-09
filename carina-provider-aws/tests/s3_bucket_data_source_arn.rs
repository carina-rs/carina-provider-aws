use carina_core::schema::RawShape;
use carina_provider_aws::schemas::generated::s3;

#[test]
fn bucket_data_source_arn_identity_is_provider_scoped() {
    let t = s3::bucket_data_source::arn();
    let RawShape::String { identity, .. } = t.raw_shape() else {
        panic!("s3::bucket_data_source::arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.s3.Bucket.Arn")
    );
}
