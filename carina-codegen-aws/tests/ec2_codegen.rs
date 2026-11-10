use std::path::Path;
use std::process::Command;

use heck::ToSnakeCase;

fn generate(resource: &str, format: &str) -> Option<(tempfile::TempDir, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let models = root.join("carina-provider-aws/tests/fixtures/smithy");
    if !models.join("ec2.json").exists() {
        eprintln!("Skipping EC2 generation: populate the Smithy model cache first");
        return None;
    }
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_smithy-codegen"))
        .arg("--model-dir")
        .arg(models)
        .arg("--output-dir")
        .arg(output.path())
        .args(["--resource", &format!("ec2.{resource}"), "--format", format])
        .arg("--examples-dir")
        .arg(root.join("examples"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stem = resource.to_snake_case();
    let suffix = if format == "markdown" { "md" } else { "rs" };
    let generated =
        std::fs::read_to_string(output.path().join(format!("ec2/{stem}.{suffix}"))).unwrap();
    Some((output, generated))
}

#[test]
fn ec2_codegen_exact_paths_refine_all_candidates_and_keep_siblings() {
    for (resource, expected) in [
        (
            "Eip",
            vec![
                "AttributeSchema::optional(\"address\", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::CoreIdentity::Ipv4Address.definition()))",
                "AttributeSchema::optional(\"public_ipv4_pool\", AttributeType::string())",
            ],
        ),
        (
            "FlowLog",
            vec![
                "carina_aws_types::AwsType::Ec2FlowLogMaxAggregationInterval.definition()",
                "AttributeType::refined_list(",
                "Some(3)",
                "Some(2)",
                "AttributeSchema::provider_populated(\"flow_log_id\", AttributeType::string())",
            ],
        ),
        (
            "SecurityGroup",
            vec![
                "carina_aws_types::AwsType::Ec2SecurityGroupName.definition()",
                "AttributeSchema::required(\"description\", AttributeType::string())",
            ],
        ),
        (
            "Subnet",
            vec![
                "carina_aws_types::AwsType::Ec2SubnetIpv4CidrBlock.definition()",
                "carina_aws_types::AwsType::Ec2SubnetIpv6CidrBlock.definition()",
                "carina_aws_types::AwsType::Ec2SubnetIpv6NetmaskLength.definition()",
                "Some(2147483647)",
                "Some(16), Some(28)",
            ],
        ),
        (
            "TransitGateway",
            vec![
                "carina_aws_types::AwsType::Ec2AmazonSideAsn.definition()",
                "carina_aws_types::AwsType::Ec2TransitGatewayCidrBlock.definition()",
            ],
        ),
        (
            "Vpc",
            vec![
                "carina_aws_types::AwsType::Ec2VpcCidrBlock.definition()",
                "Some(16), Some(28)",
                "\"host\".to_string()",
            ],
        ),
        (
            "VpnGateway",
            vec!["carina_aws_types::AwsType::Ec2VpnGatewayAmazonSideAsn.definition()"],
        ),
    ] {
        let Some((_output, generated)) = generate(resource, "rust") else {
            return;
        };
        for text in expected {
            assert!(
                generated.contains(text),
                "{resource}: missing {text}\n{generated}"
            );
        }
    }
    for (resource, unaffected) in [
        (
            "Route",
            "carina_aws_types::CoreIdentity::Ipv4Cidr.definition()",
        ),
        (
            "SecurityGroupIngress",
            "carina_aws_types::CoreIdentity::Ipv4Cidr.definition()",
        ),
    ] {
        let Some((_output, generated)) = generate(resource, "rust") else {
            return;
        };
        assert!(generated.contains(unaffected));
        assert!(!generated.contains("AwsType::Ec2VpcCidrBlock"));
        assert!(!generated.contains("AwsType::Ec2SubnetIpv4CidrBlock"));
    }
}

#[test]
fn ec2_vpn_gateway_codegen_documents_legacy_asns_without_broadening_transit_gateway() {
    let Some((_output, vgw)) = generate("VpnGateway", "rust") else {
        return;
    };
    assert!(
        vgw.contains("carina_aws_types::AwsType::Ec2VpnGatewayAmazonSideAsn.definition()"),
        "{vgw}"
    );
    let Some((_output, tgw)) = generate("TransitGateway", "rust") else {
        return;
    };
    assert!(tgw.contains("carina_aws_types::AwsType::Ec2AmazonSideAsn.definition()"));
    assert!(!tgw.contains("AwsType::Ec2VpnGatewayAmazonSideAsn"));
    let Some((_output, docs)) = generate("VpnGateway", "markdown") else {
        return;
    };
    for value in [
        "7224",
        "9059",
        "10124",
        "17493",
        "64512..=65534",
        "4200000000..=4294967294",
    ] {
        assert!(docs.contains(value), "VGW docs missing {value}: {docs}");
    }
    assert!(
        docs.contains("`; 64512..=65534 or 4200000000..=4294967294; legacy Amazon ASNs:"),
        "ASN validation note must use the shared suffix: {docs}"
    );
    assert!(
        !docs.contains("; Int;"),
        "the note must not restate the type: {docs}"
    );
}

#[test]
fn ec2_codegen_docs_include_numeric_sets_list_bounds_and_nested_paths() {
    for (resource, expected) in [
        (
            "FlowLog",
            vec![
                "allowed: 60, 600",
                "items: 1..=3",
                "items: 1..=2",
                "len: 1..=128",
            ],
        ),
        (
            "Subnet",
            vec![
                "allowed: 44, 48, 52, 56, 60, 64",
                "`Int (member constraints: range: 16..=28)`",
                "`Int (member constraints: range: 1..=2147483647)`",
            ],
        ),
        (
            "TransitGateway",
            vec![
                "Ec2AmazonSideAsn",
                "Ec2TransitGatewayCidrBlock",
                "transit_gateway_cidr_blocks",
            ],
        ),
        (
            "Vpc",
            vec![
                "`Int (member constraints: range: 16..=28)`",
                "Ec2VpcCidrBlock",
            ],
        ),
    ] {
        let Some((_output, generated)) = generate(resource, "markdown") else {
            return;
        };
        for text in expected {
            assert!(
                generated.contains(text),
                "{resource}: missing {text}\n{generated}"
            );
        }
    }
}
