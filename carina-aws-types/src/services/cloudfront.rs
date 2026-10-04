use carina_core::schema::AttributeType;

// ========== CloudFront helpers ==========

/// CloudFront's fixed Route 53 hosted zone ID, surfaced as a namespaced
/// constant.
///
/// AWS publishes `Z2FDTNDATAQYW2` as the global hosted zone ID for every
/// CloudFront distribution — it never varies by region or account.
/// Users were hard-coding the magic string in their `.crn` files; this
/// type lets them write `aws.cloudfront.HostedZoneId.global` instead,
/// which the normalizer resolves to the literal value before reaching
/// the SDK (aws#302).
///
/// Accepts:
/// - Namespaced identifier: `aws.cloudfront.HostedZoneId.global`
/// - Literal AWS spelling: `Z2FDTNDATAQYW2`
///
/// This enum itself accepts only CloudFront's fixed value. Other Route 53
/// alias-target zone IDs use [`crate::route53_hosted_zone_id`]; the
/// `alias_target.hosted_zone_id` schema accepts a union of the two types.
pub fn cloudfront_hosted_zone_id() -> AttributeType {
    AttributeType::enum_(
        carina_core::schema::enum_identity("HostedZoneId", Some("aws.cloudfront")),
        Some(vec!["Z2FDTNDATAQYW2".to_string()]),
        vec![("Z2FDTNDATAQYW2".to_string(), "global".to_string())],
        None,
        None,
    )
}
