use std::sync::LazyLock;

/// The AWS partition containing a region, shared by ARN and endpoint derivation.
///
/// Supported ARN partitions are exactly these variants, sourced from the
/// partitions metadata in aws-sdk-s3 1.156.0 (`src/endpoint_lib.rs`). This rule
/// applies to every service, including Organizations (#638); accepting a
/// partition is a syntax check, not a service-availability check.
///
/// The [ARN reference](https://docs.aws.amazon.com/IAM/latest/UserGuide/reference-arns.html)
/// defines the partition field; the
/// [European Sovereign Cloud guide](https://docs.aws.amazon.com/esc/latest/userguide/)
/// documents the separate `aws-eusc` partition.
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
    /// Every supported partition, shared by ARN validation and regex emission.
    pub const ALL: [Self; 8] = [
        Self::Aws,
        Self::AwsCn,
        Self::AwsUsGov,
        Self::AwsIso,
        Self::AwsIsoB,
        Self::AwsIsoE,
        Self::AwsIsoF,
        Self::AwsEusc,
    ];

    /// Resolve an exact ARN partition identifier.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|partition| partition.id() == id)
    }

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

/// Non-capturing regex fragment for exactly the supported ARN partitions.
pub fn arn_partition_pattern() -> &'static str {
    static PATTERN: LazyLock<String> =
        LazyLock::new(|| format!("(?:{})", Partition::ALL.map(Partition::id).join("|")));
    &PATTERN
}

/// Build an ARN regex, preserving the caller's service/resource grammar and
/// end anchor in `rest`.
pub fn arn_regex(rest: &str) -> String {
    format!("^arn:{}:{rest}", arn_partition_pattern())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_contains_every_partition_once() {
        // Keep this match exhaustive: a new enum variant must also update ALL.
        let indices = Partition::ALL.map(|partition| match partition {
            Partition::Aws => 0,
            Partition::AwsCn => 1,
            Partition::AwsUsGov => 2,
            Partition::AwsIso => 3,
            Partition::AwsIsoB => 4,
            Partition::AwsIsoE => 5,
            Partition::AwsIsoF => 6,
            Partition::AwsEusc => 7,
        });
        assert_eq!(indices, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn arn_partition_ids_and_pattern_accept_exactly_all() {
        let pattern = regex::Regex::new(&format!("^{}$", arn_partition_pattern())).unwrap();
        for partition in Partition::ALL {
            assert_eq!(Partition::from_id(partition.id()), Some(partition));
            assert!(pattern.is_match(partition.id()), "{partition:?}");
        }
        for unknown in ["", "aws-xx", "AWS", "aws-cn-extra", "aws-eusc-extra"] {
            assert_eq!(Partition::from_id(unknown), None, "{unknown}");
            assert!(!pattern.is_match(unknown), "{unknown}");
        }
    }
}
