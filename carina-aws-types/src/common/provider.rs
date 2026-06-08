use carina_core::schema::TypeIdentity;

const PROVIDER_NAME: &str = "aws";

/// Structured identity for an AWS resource-scoped custom type.
///
/// `service` + `resource` become the namespace segments and `kind` the
/// reference-kind tail, yielding `aws.<service>.<Resource>.<kind>` —
/// e.g. `aws.ec2.Vpc.Id`, `aws.iam.Role.Arn`. The provider axis keeps
/// the type distinct from any same-named type a future non-AWS provider
/// might define; the service/resource axis distinguishes, say,
/// `aws.iam.Role.Arn` from `aws.acm.Certificate.Arn`.
pub fn provider_type(service: &str, resource: &str, kind: &str) -> TypeIdentity {
    TypeIdentity::new(Some(PROVIDER_NAME), [service, resource], kind)
}

/// Structured identity for an AWS custom type with no service axis.
///
/// Used for `AvailabilityZone` (a cross-service concept owned by no
/// single service — `aws.AvailabilityZone.ZoneId`) and for the
/// fully-generic provider-scoped types (`aws.Arn`, `aws.ResourceId`,
/// `aws.AccountId`), which pass an empty `segments` slice.
pub fn provider_bare_type(segments: &[&str], kind: &str) -> TypeIdentity {
    TypeIdentity::new(Some(PROVIDER_NAME), segments.iter().copied(), kind)
}
