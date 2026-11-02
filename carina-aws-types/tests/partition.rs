use carina_aws_types::Partition;

#[test]
fn partition_from_region_id_and_dns_suffix() {
    for (region, expected, id, dns_suffix) in [
        ("us-east-1", Partition::Aws, "aws", "amazonaws.com"),
        ("cn-north-1", Partition::AwsCn, "aws-cn", "amazonaws.com.cn"),
        (
            "us-gov-west-1",
            Partition::AwsUsGov,
            "aws-us-gov",
            "amazonaws.com",
        ),
        ("us-iso-east-1", Partition::AwsIso, "aws-iso", "c2s.ic.gov"),
        (
            "us-isob-east-1",
            Partition::AwsIsoB,
            "aws-iso-b",
            "sc2s.sgov.gov",
        ),
        (
            "eu-isoe-west-1",
            Partition::AwsIsoE,
            "aws-iso-e",
            "cloud.adc-e.uk",
        ),
        (
            "us-isof-south-1",
            Partition::AwsIsoF,
            "aws-iso-f",
            "csp.hci.ic.gov",
        ),
        (
            "eusc-de-east-1",
            Partition::AwsEusc,
            "aws-eusc",
            "amazonaws.eu",
        ),
    ] {
        let partition = Partition::from_region(region);
        assert_eq!(partition, expected, "{region}");
        assert_eq!(partition.id(), id, "{region}");
        assert_eq!(partition.dns_suffix(), dns_suffix, "{region}");
    }
}

#[test]
fn partition_from_region_preserves_prefix_rules() {
    for (prefix, expected) in [
        ("cn-", Partition::AwsCn),
        ("us-gov-", Partition::AwsUsGov),
        ("us-iso-", Partition::AwsIso),
        ("us-isob-", Partition::AwsIsoB),
        ("eu-isoe-", Partition::AwsIsoE),
        ("us-isof-", Partition::AwsIsoF),
        ("eusc-", Partition::AwsEusc),
    ] {
        // Preserve KMS's prefix-based behavior even for unlisted future regions.
        assert_eq!(
            Partition::from_region(&format!("{prefix}future-99")),
            expected
        );
    }
    for region in ["", "xx-fake-1", "us-isobad-east-1", "euscandidate-1"] {
        assert_eq!(Partition::from_region(region), Partition::Aws, "{region}");
    }
}
