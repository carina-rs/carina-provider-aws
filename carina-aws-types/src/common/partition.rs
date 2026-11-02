/// The AWS partition containing a region, shared by ARN and endpoint derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Partition {
    Aws,
    AwsCn,
    AwsUsGov,
    AwsIso,
    AwsIsoB,
    AwsIsoE,
    AwsIsoF,
    AwsEusc,
}

impl Partition {
    /// Resolve a region using the prefix rules shared with KMS reference checks.
    /// Unrecognized regions retain the commercial partition fallback; this does
    /// not validate region names or service availability.
    pub fn from_region(region: &str) -> Self {
        if region.starts_with("cn-") {
            Self::AwsCn
        } else if region.starts_with("us-gov-") {
            Self::AwsUsGov
        } else if region.starts_with("us-iso-") {
            Self::AwsIso
        } else if region.starts_with("us-isob-") {
            Self::AwsIsoB
        } else if region.starts_with("eu-isoe-") {
            Self::AwsIsoE
        } else if region.starts_with("us-isof-") {
            Self::AwsIsoF
        } else if region.starts_with("eusc-") {
            Self::AwsEusc
        } else {
            Self::Aws
        }
    }

    /// Partition identifier used in ARNs.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::AwsCn => "aws-cn",
            Self::AwsUsGov => "aws-us-gov",
            Self::AwsIso => "aws-iso",
            Self::AwsIsoB => "aws-iso-b",
            Self::AwsIsoE => "aws-iso-e",
            Self::AwsIsoF => "aws-iso-f",
            Self::AwsEusc => "aws-eusc",
        }
    }

    /// DNS suffix from the partitions metadata embedded in aws-sdk-s3 1.156.0
    /// (`src/endpoint_lib.rs`). Its partition resolver is crate-private, so this
    /// shared table supports both native and wasm32-wasip2 without SDK internals.
    pub const fn dns_suffix(self) -> &'static str {
        match self {
            Self::Aws | Self::AwsUsGov => "amazonaws.com",
            Self::AwsCn => "amazonaws.com.cn",
            Self::AwsIso => "c2s.ic.gov",
            Self::AwsIsoB => "sc2s.sgov.gov",
            Self::AwsIsoE => "cloud.adc-e.uk",
            Self::AwsIsoF => "csp.hci.ic.gov",
            Self::AwsEusc => "amazonaws.eu",
        }
    }
}
