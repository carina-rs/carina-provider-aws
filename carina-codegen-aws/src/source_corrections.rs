//! Narrow, reviewed corrections to the Smithy source, separate from overrides.
use anyhow::{Context, Result};

/// This table cannot replace a type or supply an arbitrary pattern rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCorrection {
    ArnPartition,
}

impl SourceCorrection {
    /// Rewrite only the leading partition; preserve the complete suffix.
    pub fn apply(self, source: &str) -> Result<String> {
        match self {
            Self::ArnPartition => {
                let suffix = source
                    .strip_prefix("^arn:aws:")
                    .context("expected the Smithy pattern to start with the literal ^arn:aws:")?;
                Ok(carina_aws_types::arn_regex(suffix))
            }
        }
    }

    pub const fn documentation(self) -> &'static str {
        match self {
            Self::ArnPartition => {
                "Partition widened from the Smithy model to all partitions supported by this provider."
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SourceCorrectionSite {
    pub resource: &'static str,
    pub path: &'static str,
    pub correction: SourceCorrection,
}

pub const SOURCE_CORRECTIONS: &[SourceCorrectionSite] = &[
    SourceCorrectionSite {
        resource: "organizations.Account",
        path: "arn",
        correction: SourceCorrection::ArnPartition,
    },
    SourceCorrectionSite {
        resource: "organizations.Organization",
        path: "arn",
        correction: SourceCorrection::ArnPartition,
    },
    SourceCorrectionSite {
        resource: "organizations.Organization",
        path: "master_account_arn",
        correction: SourceCorrection::ArnPartition,
    },
];

pub fn correction(resource: &str, path: &str) -> Option<SourceCorrection> {
    SOURCE_CORRECTIONS
        .iter()
        .find(|site| site.resource == resource && site.path == path)
        .map(|site| site.correction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_662_round2_source_correction_can_only_rewrite_the_partition() {
        let suffix = r"organizations::[0-9]{12}:account\/o-[a-z0-9]{10,32}\/[0-9]{12}$";
        let corrected = SourceCorrection::ArnPartition
            .apply(&format!("^arn:aws:{suffix}"))
            .unwrap();
        assert_eq!(
            corrected,
            format!(
                "^arn:{}:{suffix}",
                carina_aws_types::arn_partition_pattern()
            )
        );
        assert_eq!(SOURCE_CORRECTIONS.len(), 3);
        assert!(correction("organizations.Account", "nested.arn").is_none());
        assert!(correction("organizations.Other", "arn").is_none());
        assert!(correction("iam.Role", "arn").is_none());
    }
}
