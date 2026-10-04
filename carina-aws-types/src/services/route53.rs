use carina_core::schema::AttributeType;

use crate::provider_type;

/// Route 53 hosted-zone ID shared by alias-target producers and consumers.
///
/// The identity distinguishes hosted-zone IDs from arbitrary strings while
/// accepting the IDs that AWS publishes for services and regions.
///
/// Evidence for the refinement:
/// - The Route 53 Smithy model makes `AliasTarget.HostedZoneId` a `ResourceId`
///   with `length: {min: 0, max: 32}` and no pattern.
/// - Measured by running Cloud Control create, read, and delete operations for
///   `AWS::Route53::HostedZone` in `us-east-1` on 2026-10-04. Create returned a
///   primary identifier and read-back returned `Id = Z05136711ZXUHBDOA8D5O`
///   (21 characters, without a `/hostedzone/` prefix); the resource was deleted
///   afterward. Directions checked: create response to primary identifier and
///   primary identifier to read-back state.
/// - AWS-published alias zone IDs — CloudFront `Z2FDTNDATAQYW2`, Global
///   Accelerator `Z2BJ6XQ5FK7U4H`, ELB `Z14GRHDCWA56QT`, and the S3 website
///   zone IDs in this repository's `s3_hosted_zone_id` table — all match.
pub fn route53_hosted_zone_id() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_type("route53", "HostedZone", "Id")),
        Some("^Z[A-Z0-9]+$".to_string()),
        Some((None, Some(32))),
        None,
    )
}
